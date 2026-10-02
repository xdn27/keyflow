//! Undo stack berlapis dan persistent intent log (T1.6).
//!
//! Mematuhi aturan `docs/agents/safety.md`:
//! 1. Log persisten berbasis JSON Lines: niat dicatat dan di-flush sebelum eksekusi,
//!    hasil dicatat dan di-flush sesudahnya.
//! 2. Undo aman: mengembalikan file ke lokasi asal. Jika path asal sudah ditempati,
//!    GAGAL DENGAN AMAN (jangan menimpa; laporkan).
//! 3. Undo dari aksi `copy` menghapus salinan hanya lewat Trash (`trash::delete`).
//! 4. Batas kapasitas stack diatur oleh `undo_history_limit`.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::ActionType;

/// Status entri log persisten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEntryStatus {
    Intent,
    Completed,
    Failed,
    Undone,
}

/// Entri log persisten JSON Lines (crash-proof).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentLogEntry {
    pub transaction_id: String,
    pub timestamp_epoch_ms: u64,
    pub action: ActionType,
    pub source: PathBuf,
    pub destination: Option<PathBuf>,
    pub status: LogEntryStatus,
    pub error_message: Option<String>,
    #[serde(default)]
    pub file_len: Option<u64>,
}

/// Rekaman operasi satu file dalam stack undo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoRecord {
    pub action: ActionType,
    pub original_source: PathBuf,
    pub actual_destination: PathBuf,
    #[serde(default)]
    pub file_len: Option<u64>,
}

impl UndoRecord {
    /// Membuat UndoRecord baru dan secara otomatis merekam ukuran file jika target ada.
    pub fn new(action: ActionType, original_source: PathBuf, actual_destination: PathBuf) -> Self {
        let file_len = std::fs::metadata(&actual_destination).ok().map(|m| m.len());
        Self {
            action,
            original_source,
            actual_destination,
            file_len,
        }
    }

    /// Membuat UndoRecord dengan ukuran file eksplisit.
    pub fn with_len(
        action: ActionType,
        original_source: PathBuf,
        actual_destination: PathBuf,
        file_len: Option<u64>,
    ) -> Self {
        Self {
            action,
            original_source,
            actual_destination,
            file_len,
        }
    }
}

/// Transaksi undo yang mengelompokkan operasi multi-file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoTransaction {
    pub id: String,
    pub timestamp_epoch_ms: u64,
    pub records: Vec<UndoRecord>,
}

/// Laporan hasil eksekusi satu langkah undo.
#[derive(Debug, Clone)]
pub struct UndoReport {
    pub transaction_id: String,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub details: Vec<UndoItemResult>,
}

#[derive(Debug, Clone)]
pub struct UndoItemResult {
    pub current_path: PathBuf,
    pub restored_path: PathBuf,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Pengelola Undo dan Log Persisten.
pub struct UndoManager {
    log_file_path: Option<PathBuf>,
    log_file: Option<File>,
    history_limit: usize,
    stack: VecDeque<UndoTransaction>,
}

impl UndoManager {
    /// Inisialisasi UndoManager di memori (tanpa log disk, mis. untuk test).
    pub fn memory_only(history_limit: usize) -> Self {
        Self {
            log_file_path: None,
            log_file: None,
            history_limit,
            stack: VecDeque::new(),
        }
    }

    /// Inisialisasi UndoManager dengan file log persisten.
    pub fn with_log_file(log_path: PathBuf, history_limit: usize) -> io::Result<Self> {
        if let Some(parent) = log_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let mut manager = Self {
            log_file_path: Some(log_path.clone()),
            log_file: Some(file),
            history_limit,
            stack: VecDeque::new(),
        };

        // Muat riwayat transaksi yang belum di-undo dari file log jika ada
        manager.recover_from_log(&log_path)?;

        Ok(manager)
    }

    /// Mengambil path file log persisten bila ada.
    pub fn log_file_path(&self) -> Option<&Path> {
        self.log_file_path.as_deref()
    }

    /// Mengambil jumlah transaksi yang tersimpan di stack undo.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Memeriksa apakah stack undo kosong.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Mencatat niat (INTENT) operasi file dan melakukan flush ke disk sebelum eksekusi dimulai.
    pub fn log_intent(
        &mut self,
        transaction_id: &str,
        action: &ActionType,
        source: &Path,
        destination: Option<&Path>,
    ) -> io::Result<()> {
        let entry = PersistentLogEntry {
            transaction_id: transaction_id.to_string(),
            timestamp_epoch_ms: current_epoch_ms(),
            action: action.clone(),
            source: source.to_path_buf(),
            destination: destination.map(Path::to_path_buf),
            status: LogEntryStatus::Intent,
            error_message: None,
            file_len: None,
        };
        self.append_log_entry(&entry)
    }

    /// Mencatat hasil (COMPLETED / FAILED) operasi file dan melakukan flush ke disk.
    pub fn log_completion(
        &mut self,
        transaction_id: &str,
        action: &ActionType,
        source: &Path,
        destination: Option<&Path>,
        success: bool,
        error_message: Option<String>,
    ) -> io::Result<()> {
        let file_len = destination.and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()));
        let entry = PersistentLogEntry {
            transaction_id: transaction_id.to_string(),
            timestamp_epoch_ms: current_epoch_ms(),
            action: action.clone(),
            source: source.to_path_buf(),
            destination: destination.map(Path::to_path_buf),
            status: if success {
                LogEntryStatus::Completed
            } else {
                LogEntryStatus::Failed
            },
            error_message,
            file_len,
        };
        self.append_log_entry(&entry)
    }

    /// Menambahkan transaksi sukses ke undo stack.
    pub fn push_transaction(&mut self, tx: UndoTransaction) {
        if tx.records.is_empty() {
            return;
        }

        if self.stack.len() >= self.history_limit {
            self.stack.pop_front();
        }

        self.stack.push_back(tx);
    }

    /// Mengeksekusi undo untuk transaksi paling baru (LIFO).
    pub fn undo_latest(&mut self) -> Option<UndoReport> {
        let tx = self.stack.pop_back()?;
        let mut report = UndoReport {
            transaction_id: tx.id.clone(),
            total: tx.records.len(),
            succeeded: 0,
            failed: 0,
            details: Vec::with_capacity(tx.records.len()),
        };

        // Eksekusi undo dalam urutan terbalik dari operasi asli
        for record in tx.records.iter().rev() {
            let res = undo_single_record(record);
            if res.success {
                report.succeeded += 1;
                let _ = self.append_log_entry(&PersistentLogEntry {
                    transaction_id: tx.id.clone(),
                    timestamp_epoch_ms: current_epoch_ms(),
                    action: record.action.clone(),
                    source: record.actual_destination.clone(),
                    destination: Some(record.original_source.clone()),
                    status: LogEntryStatus::Undone,
                    error_message: None,
                    file_len: record.file_len,
                });
            } else {
                report.failed += 1;
                let _ = self.append_log_entry(&PersistentLogEntry {
                    transaction_id: tx.id.clone(),
                    timestamp_epoch_ms: current_epoch_ms(),
                    action: record.action.clone(),
                    source: record.actual_destination.clone(),
                    destination: Some(record.original_source.clone()),
                    status: LogEntryStatus::Failed,
                    error_message: res.error_message.clone(),
                    file_len: record.file_len,
                });
            }
            report.details.push(res);
        }

        Some(report)
    }

    /// Menulis satu entri log persisten secara atomic append + flush.
    fn append_log_entry(&mut self, entry: &PersistentLogEntry) -> io::Result<()> {
        if let Some(ref mut file) = self.log_file {
            let json = serde_json::to_string(entry)
                .map_err(|e| io::Error::other(format!("Serialisasi log JSON gagal: {e}")))?;
            writeln!(file, "{json}")?;
            file.flush()?;
        }
        Ok(())
    }

    /// Memulihkan riwayat undo dari file log persisten.
    fn recover_from_log(&mut self, log_path: &Path) -> io::Result<()> {
        let file = File::open(log_path)?;
        let reader = BufReader::new(file);

        let mut completed_records: std::collections::HashMap<String, Vec<UndoRecord>> =
            std::collections::HashMap::new();
        let mut transaction_order: Vec<String> = Vec::new();
        let mut undone_transactions: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<PersistentLogEntry>(trimmed) {
                match entry.status {
                    LogEntryStatus::Completed => {
                        if let Some(dst) = entry.destination {
                            let rec = UndoRecord {
                                action: entry.action,
                                original_source: entry.source,
                                actual_destination: dst,
                                file_len: entry.file_len,
                            };
                            if !completed_records.contains_key(&entry.transaction_id) {
                                transaction_order.push(entry.transaction_id.clone());
                            }
                            completed_records
                                .entry(entry.transaction_id)
                                .or_default()
                                .push(rec);
                        }
                    }
                    LogEntryStatus::Undone => {
                        undone_transactions.insert(entry.transaction_id);
                    }
                    _ => {}
                }
            }
        }

        // Susun transaksi yang belum di-undo ke stack dalam urutan kronologis yang stabil
        for tx_id in transaction_order {
            if !undone_transactions.contains(&tx_id) {
                if let Some(records) = completed_records.remove(&tx_id) {
                    if !records.is_empty() {
                        self.push_transaction(UndoTransaction {
                            id: tx_id,
                            timestamp_epoch_ms: 0,
                            records,
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Menjalankan undo untuk satu file rekaman operasi.
fn undo_single_record(record: &UndoRecord) -> UndoItemResult {
    let current_path = &record.actual_destination;
    let target_restore = &record.original_source;

    // Periksa apakah file yang ingin dipulihkan masih ada di lokasi saat ini
    if !current_path.exists() {
        return UndoItemResult {
            current_path: current_path.clone(),
            restored_path: target_restore.clone(),
            success: false,
            error_message: Some(format!(
                "File tidak ditemukan di '{}' (mungkin sudah dipindahkan atau dihapus)",
                current_path.display()
            )),
        };
    }

    match record.action {
        ActionType::Move | ActionType::Rename => {
            // ATURAN KEAMANAN WAJIB: Jika path asal sudah ditempati file lain,
            // GAGAL DENGAN AMAN! Jangan menimpa file tersebut.
            if target_restore.exists() {
                return UndoItemResult {
                    current_path: current_path.clone(),
                    restored_path: target_restore.clone(),
                    success: false,
                    error_message: Some(format!(
                        "Undo dibatalkan demi keamanan data: path asal '{}' sudah ditempati file lain",
                        target_restore.display()
                    )),
                };
            }

            // Pastikan direktori induk asal ada
            if let Some(parent) = target_restore.parent() {
                if !parent.exists() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }

            match std::fs::rename(current_path, target_restore) {
                Ok(()) => UndoItemResult {
                    current_path: current_path.clone(),
                    restored_path: target_restore.clone(),
                    success: true,
                    error_message: None,
                },
                Err(e) if crate::actions::is_cross_device_error(&e) => {
                    match crate::actions::move_cross_device(current_path, target_restore) {
                        Ok(()) => UndoItemResult {
                            current_path: current_path.clone(),
                            restored_path: target_restore.clone(),
                            success: true,
                            error_message: None,
                        },
                        Err(err) => UndoItemResult {
                            current_path: current_path.clone(),
                            restored_path: target_restore.clone(),
                            success: false,
                            error_message: Some(format!(
                                "Gagal mengembalikan file lintas-drive saat undo: {err}"
                            )),
                        },
                    }
                }
                Err(e) => UndoItemResult {
                    current_path: current_path.clone(),
                    restored_path: target_restore.clone(),
                    success: false,
                    error_message: Some(format!("Gagal mengembalikan file: {e}")),
                },
            }
        }
        ActionType::Copy => {
            if current_path.is_file() {
                if let Some(expected_len) = record.file_len {
                    if let Ok(meta) = std::fs::metadata(current_path) {
                        if meta.len() != expected_len {
                            return UndoItemResult {
                                current_path: current_path.clone(),
                                restored_path: target_restore.clone(),
                                success: false,
                                error_message: Some(format!(
                                    "Undo dibatalkan demi keamanan data: file salinan '{}' telah dimodifikasi (ukuran berubah)",
                                    current_path.display()
                                )),
                            };
                        }
                    }
                }
            } else if current_path.is_dir() {
                // Verifikasi integritas direktori: jika folder salinan berisi file baru atau ada file yang diubah,
                // batalkan undo demi mencegah hilangnya pekerjaan baru pengguna!
                if is_dir_modified_or_has_new_files(current_path, target_restore) {
                    return UndoItemResult {
                        current_path: current_path.clone(),
                        restored_path: target_restore.clone(),
                        success: false,
                        error_message: Some(format!(
                            "Undo dibatalkan demi keamanan data: direktori salinan '{}' telah dimodifikasi atau berisi berkas baru",
                            current_path.display()
                        )),
                    };
                }
            }

            // Undo dari copy: menghapus file salinan HANYA lewat Recycle Bin/Trash
            match trash::delete(current_path) {
                Ok(()) => UndoItemResult {
                    current_path: current_path.clone(),
                    restored_path: target_restore.clone(),
                    success: true,
                    error_message: None,
                },
                Err(e) => UndoItemResult {
                    current_path: current_path.clone(),
                    restored_path: target_restore.clone(),
                    success: false,
                    error_message: Some(format!("Gagal menghapus salinan ke Trash saat undo: {e}")),
                },
            }
        }
        ActionType::Trash | ActionType::Undo => UndoItemResult {
            current_path: current_path.clone(),
            restored_path: target_restore.clone(),
            success: false,
            error_message: Some(
                "Aksi Trash tidak dapat di-undo otomatis tanpa restore COM/AppleScript Trash"
                    .to_string(),
            ),
        },
    }
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Memeriksa apakah direktori salinan telah dimodifikasi atau berisi berkas/folder baru
/// yang tidak ada di direktori sumber asli.
fn is_dir_modified_or_has_new_files(copied_dir: &Path, original_source: &Path) -> bool {
    if !original_source.is_dir() {
        return true;
    }

    let entries = match std::fs::read_dir(copied_dir) {
        Ok(e) => e,
        Err(_) => return true,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let rel_path = match path.strip_prefix(copied_dir) {
            Ok(p) => p,
            Err(_) => return true,
        };
        let orig_counterpart = original_source.join(rel_path);

        if !orig_counterpart.exists() {
            // Berkas atau subdirektori baru ditemukan di dalam salinan
            return true;
        }

        if path.is_file() {
            let copied_meta = match path.metadata() {
                Ok(m) => m,
                Err(_) => return true,
            };
            let orig_meta = match orig_counterpart.metadata() {
                Ok(m) => m,
                Err(_) => return true,
            };
            if copied_meta.len() != orig_meta.len() {
                // Ukuran berkas di dalam salinan telah berubah
                return true;
            }
        } else if path.is_dir() && is_dir_modified_or_has_new_files(&path, &orig_counterpart) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    #[test]
    fn test_undo_move_success() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("asal.txt");
        let dst = temp.path().join("tujuan.txt");

        // Simulasikan file sudah dipindah ke dst
        File::create(&dst).unwrap().write_all(b"data").unwrap();

        let mut manager = UndoManager::memory_only(10);
        manager.push_transaction(UndoTransaction {
            id: "tx-1".to_string(),
            timestamp_epoch_ms: 1000,
            records: vec![UndoRecord {
                action: ActionType::Move,
                original_source: src.clone(),
                actual_destination: dst.clone(),
                file_len: None,
            }],
        });

        let report = manager.undo_latest().expect("Harus ada transaksi");
        assert_eq!(report.succeeded, 1);
        assert_eq!(report.failed, 0);
        assert!(src.exists());
        assert!(!dst.exists());
    }

    #[test]
    fn test_undo_safe_fail_when_origin_occupied() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("asal.txt");
        let dst = temp.path().join("tujuan.txt");

        // Dua file ada sekaligus (path asal sudah ditempati file baru!)
        File::create(&src)
            .unwrap()
            .write_all(b"file baru di asal")
            .unwrap();
        File::create(&dst)
            .unwrap()
            .write_all(b"file yang dipindah")
            .unwrap();

        let mut manager = UndoManager::memory_only(10);
        manager.push_transaction(UndoTransaction {
            id: "tx-2".to_string(),
            timestamp_epoch_ms: 1000,
            records: vec![UndoRecord {
                action: ActionType::Move,
                original_source: src.clone(),
                actual_destination: dst.clone(),
                file_len: None,
            }],
        });

        let report = manager.undo_latest().unwrap();
        assert_eq!(report.succeeded, 0);
        assert_eq!(report.failed, 1); // GAGAL DENGAN AMAN!
        assert_eq!(fs::read_to_string(&src).unwrap(), "file baru di asal"); // Tidak tertimpa!
        assert_eq!(fs::read_to_string(&dst).unwrap(), "file yang dipindah"); // Tetap utuh!
    }

    #[test]
    fn test_undo_history_limit() {
        let mut manager = UndoManager::memory_only(2);
        for i in 1..=5 {
            manager.push_transaction(UndoTransaction {
                id: format!("tx-{i}"),
                timestamp_epoch_ms: i,
                records: vec![UndoRecord {
                    action: ActionType::Move,
                    original_source: PathBuf::from(format!("/a/{i}")),
                    actual_destination: PathBuf::from(format!("/b/{i}")),
                    file_len: None,
                }],
            });
        }

        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_undo_copy_fails_if_modified() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("original.txt");
        let dst = temp.path().join("copy.txt");

        File::create(&dst)
            .unwrap()
            .write_all(b"modified content that is longer")
            .unwrap();

        let mut manager = UndoManager::memory_only(10);
        manager.push_transaction(UndoTransaction {
            id: "tx-copy".to_string(),
            timestamp_epoch_ms: 1000,
            records: vec![UndoRecord {
                action: ActionType::Copy,
                original_source: src,
                actual_destination: dst.clone(),
                file_len: Some(4), // Ukuran asli 4 byte, tapi sekarang lebih panjang
            }],
        });

        let report = manager.undo_latest().unwrap();
        assert_eq!(report.failed, 1); // Harus dibatalkan demi keamanan
        assert!(dst.exists()); // File salinan tidak boleh dihapus!
    }

    #[test]
    fn test_recover_from_log_preserves_order() {
        let temp = tempfile::tempdir().unwrap();
        let log_file = temp.path().join("test_log.jsonl");

        {
            let mut file = File::create(&log_file).unwrap();
            // Tulis 3 transaksi secara kronologis
            for i in 1..=3 {
                let entry = PersistentLogEntry {
                    transaction_id: format!("tx-{i}"),
                    timestamp_epoch_ms: i * 100,
                    action: ActionType::Move,
                    source: PathBuf::from(format!("/src/{i}")),
                    destination: Some(PathBuf::from(format!("/dst/{i}"))),
                    status: LogEntryStatus::Completed,
                    error_message: None,
                    file_len: None,
                };
                writeln!(file, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
            }
        }

        let mut manager = UndoManager::with_log_file(log_file, 10).unwrap();
        assert_eq!(manager.len(), 3);

        // LIFO: pop harus menghasilkan tx-3, lalu tx-2, lalu tx-1
        assert_eq!(manager.undo_latest().unwrap().transaction_id, "tx-3");
        assert_eq!(manager.undo_latest().unwrap().transaction_id, "tx-2");
        assert_eq!(manager.undo_latest().unwrap().transaction_id, "tx-1");
        assert!(manager.is_empty());
    }

    #[test]
    fn test_undo_copy_dir_fails_if_new_files_added() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("proyek_asli");
        let copied_dir = temp.path().join("proyek_salinan");

        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&copied_dir).unwrap();

        let base_file = src_dir.join("kode.rs");
        File::create(&base_file)
            .unwrap()
            .write_all(b"fn main() {}")
            .unwrap();
        let copy_base_file = copied_dir.join("kode.rs");
        File::create(&copy_base_file)
            .unwrap()
            .write_all(b"fn main() {}")
            .unwrap();

        // Tambahkan file baru di dalam direktori salinan (skenario pengguna bekerja di folder copy)
        let new_user_file = copied_dir.join("catatan_penting.txt");
        File::create(&new_user_file)
            .unwrap()
            .write_all(b"catatan")
            .unwrap();

        let mut manager = UndoManager::memory_only(10);
        manager.push_transaction(UndoTransaction {
            id: "tx-dir-copy".to_string(),
            timestamp_epoch_ms: 1000,
            records: vec![UndoRecord {
                action: ActionType::Copy,
                original_source: src_dir,
                actual_destination: copied_dir.clone(),
                file_len: None,
            }],
        });

        let report = manager.undo_latest().unwrap();
        assert_eq!(report.failed, 1);
        assert!(copied_dir.exists());
        assert!(new_user_file.exists()); // File baru pengguna dijamin TIDAK terhapus!
    }
}

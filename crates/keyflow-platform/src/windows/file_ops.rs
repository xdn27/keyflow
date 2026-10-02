//! Logika pemindahan file spike M0 (T0.3).
//!
//! Mengikuti aturan ketat `docs/agents/safety.md`:
//! 1. Tidak menimpa file: jika file tujuan sudah ada, otomatis rename sufiks ` (1).ext`.
//! 2. Asal == tujuan: no-op aman.
//! 3. Tolak path berbahaya (root drive, direktori sistem Windows).
//! 4. Tolak pemindahan folder ke dalam dirinya sendiri.
//! 5. Penanganan lintas-drive: copy -> verifikasi ukuran -> hapus sumber jika lolos.
//!    Jika verifikasi gagal, salinan parsial dibersihkan dan sumber dibiarkan utuh.
//! 6. Log dicatat sebelum dan sesudah eksekusi setiap file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Laporan hasil eksekusi pemindahan file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOperationResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub success: bool,
    pub is_noop: bool,
    pub error_message: Option<String>,
}

/// Ringkasan keseluruhan pemindahan sejumlah file.
#[derive(Debug, Clone, Default)]
pub struct MoveSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub noop: usize,
    pub results: Vec<FileOperationResult>,
}

/// Memeriksa apakah path merupakan path sistem berbahaya yang dilarang disentuh.
pub fn is_dangerous_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let trimmed = s.trim().trim_end_matches(['\\', '/']);

    // Cek root drive (mis. "C:", "D:")
    if trimmed.len() <= 3 && trimmed.ends_with(':') {
        return true;
    }
    if trimmed == "/" || trimmed == "\\" {
        return true;
    }

    let upper = trimmed.to_uppercase();
    let dangerous_prefixes = [
        "C:\\WINDOWS",
        "C:\\PROGRAM FILES",
        "C:\\PROGRAM FILES (X86)",
        "C:\\SYSTEM VOLUME INFORMATION",
        "C:\\$RECYCLE.BIN",
        "C:\\BOOT",
        "C:\\RECOVERY",
    ];

    for prefix in dangerous_prefixes {
        if upper == prefix || upper.starts_with(&format!("{prefix}\\")) {
            return true;
        }
    }

    false
}

/// Memeriksa apakah tujuan berada di dalam folder sumber (mencegah loop rekursif).
pub fn is_destination_inside_source(source: &Path, destination: &Path) -> bool {
    let Ok(src_canon) = source.canonicalize() else {
        return false;
    };
    let Ok(dst_canon) = destination.canonicalize() else {
        // Jika tujuan belum ada, periksa path induknya
        if let Some(parent) = destination.parent() {
            if let Ok(p_canon) = parent.canonicalize() {
                return p_canon.starts_with(&src_canon);
            }
        }
        return false;
    };

    dst_canon.starts_with(&src_canon)
}

/// Menghasilkan nama tujuan yang unik bila terjadi konflik (`on_conflict: rename`).
/// Contoh: `foto.jpg` -> `foto (1).jpg` -> `foto (2).jpg`.
pub fn resolve_unique_destination(dest_path: &Path) -> PathBuf {
    if !dest_path.exists() {
        return dest_path.to_path_buf();
    }

    let parent = dest_path.parent().unwrap_or_else(|| Path::new(""));
    let file_stem = dest_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let extension = dest_path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let new_file_name = format!("{file_stem} ({counter}){extension}");
        let candidate = parent.join(new_file_name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Memindahkan satu file dengan aturan keamanan penuh.
pub fn move_single_file_safe(source: &Path, target_dir: &Path) -> FileOperationResult {
    let file_name = match source.file_name() {
        Some(name) => name,
        None => {
            return FileOperationResult {
                source: source.to_path_buf(),
                destination: target_dir.to_path_buf(),
                success: false,
                is_noop: false,
                error_message: Some("Path sumber tidak memiliki nama file yang valid".to_string()),
            };
        }
    };

    let planned_dest = target_dir.join(file_name);

    // 1. Validasi path berbahaya
    if is_dangerous_path(source) {
        return FileOperationResult {
            source: source.to_path_buf(),
            destination: planned_dest,
            success: false,
            is_noop: false,
            error_message: Some(format!(
                "Operasi dibatalkan: path sumber berbahaya ({})",
                source.display()
            )),
        };
    }
    if is_dangerous_path(target_dir) {
        return FileOperationResult {
            source: source.to_path_buf(),
            destination: planned_dest,
            success: false,
            is_noop: false,
            error_message: Some(format!(
                "Operasi dibatalkan: folder tujuan berbahaya ({})",
                target_dir.display()
            )),
        };
    }

    // 2. Cek apakah sumber ada
    if !source.exists() {
        return FileOperationResult {
            source: source.to_path_buf(),
            destination: planned_dest,
            success: false,
            is_noop: false,
            error_message: Some(format!("File sumber tidak ditemukan: {}", source.display())),
        };
    }

    // 3. Cek apakah tujuan di dalam sumber
    if source.is_dir() && is_destination_inside_source(source, target_dir) {
        return FileOperationResult {
            source: source.to_path_buf(),
            destination: planned_dest,
            success: false,
            is_noop: false,
            error_message: Some(
                "Tidak dapat memindahkan folder ke dalam dirinya sendiri".to_string(),
            ),
        };
    }

    // 4. Asal == tujuan: no-op aman
    if let (Ok(s_canon), Ok(d_canon)) = (source.canonicalize(), planned_dest.canonicalize()) {
        if s_canon == d_canon {
            tracing::info!(
                source = %source.display(),
                "Asal sama dengan tujuan, no-op"
            );
            return FileOperationResult {
                source: source.to_path_buf(),
                destination: planned_dest,
                success: true,
                is_noop: true,
                error_message: None,
            };
        }
    }

    // 5. Pastikan folder tujuan ada
    if !target_dir.exists() {
        if let Err(e) = fs::create_dir_all(target_dir) {
            return FileOperationResult {
                source: source.to_path_buf(),
                destination: planned_dest,
                success: false,
                is_noop: false,
                error_message: Some(format!(
                    "Gagal membuat folder tujuan {}: {e}",
                    target_dir.display()
                )),
            };
        }
    }

    // 6. Selesaikan konflik nama (on_conflict: rename)
    let final_dest = resolve_unique_destination(&planned_dest);

    // 7. Log SEBELUM eksekusi (intent logging)
    let timestamp = SystemTime::now();
    tracing::info!(
        source = %source.display(),
        destination = %final_dest.display(),
        timestamp = ?timestamp,
        "INTENT: Akan memindahkan file"
    );

    // 8. Eksekusi pemindahan file
    let move_res = match fs::rename(source, &final_dest) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device_error(&e) => {
            // Skenario lintas-drive: copy -> verifikasi -> hapus sumber
            tracing::warn!(
                source = %source.display(),
                destination = %final_dest.display(),
                "Rename gagal karena lintas-drive, melakukan copy + verify + delete"
            );
            move_cross_device_safe(source, &final_dest)
        }
        Err(e) => Err(e),
    };

    // 9. Log SESUDAH eksekusi
    match move_res {
        Ok(()) => {
            tracing::info!(
                source = %source.display(),
                destination = %final_dest.display(),
                "SUCCESS: File berhasil dipindahkan"
            );
            FileOperationResult {
                source: source.to_path_buf(),
                destination: final_dest,
                success: true,
                is_noop: false,
                error_message: None,
            }
        }
        Err(e) => {
            tracing::error!(
                source = %source.display(),
                destination = %final_dest.display(),
                error = %e,
                "FAILED: Gagal memindahkan file"
            );
            FileOperationResult {
                source: source.to_path_buf(),
                destination: final_dest,
                success: false,
                is_noop: false,
                error_message: Some(e.to_string()),
            }
        }
    }
}

/// Memeriksa apakah io::Error merupakan error batas drive / filesystem.
fn is_cross_device_error(err: &io::Error) -> bool {
    // Windows ERROR_NOT_SAME_DEVICE = 17
    if let Some(17) = err.raw_os_error() {
        return true;
    }
    // Linux EXDEV = 18
    if let Some(18) = err.raw_os_error() {
        return true;
    }
    err.kind() == io::ErrorKind::CrossesDevices
}

/// Pemindahan aman untuk operasi lintas-drive:
/// copy -> verifikasi ukuran -> hapus sumber hanya bila verifikasi sukses.
fn move_cross_device_safe(source: &Path, destination: &Path) -> io::Result<()> {
    // 1. Dapatkan ukuran sumber
    let src_meta = fs::metadata(source)?;
    let src_len = src_meta.len();

    // 2. Salin file
    fs::copy(source, destination)?;

    // 3. Verifikasi ukuran file tujuan
    let dst_meta = match fs::metadata(destination) {
        Ok(m) => m,
        Err(e) => {
            // Bersihkan salinan jika metadata gagal dibaca
            let _ = fs::remove_file(destination);
            return Err(e);
        }
    };

    if dst_meta.len() != src_len {
        // Verifikasi gagal! Bersihkan salinan dan biarkan sumber utuh
        let _ = fs::remove_file(destination);
        return Err(io::Error::other(format!(
            "Verifikasi lintas-drive gagal: ukuran tidak cocok (sumber: {src_len}, tujuan: {})",
            dst_meta.len()
        )));
    }

    // 4. Hapus sumber setelah terverifikasi
    if source.is_dir() {
        fs::remove_dir_all(source)?;
    } else {
        fs::remove_file(source)?;
    }

    Ok(())
}

/// Memindahkan beberapa file terpilih ke satu folder tujuan.
pub fn move_selected_files(items: &[PathBuf], target_dir: &Path) -> MoveSummary {
    let mut summary = MoveSummary {
        total: items.len(),
        succeeded: 0,
        failed: 0,
        noop: 0,
        results: Vec::with_capacity(items.len()),
    };

    for item in items {
        let res = move_single_file_safe(item, target_dir);
        if res.is_noop {
            summary.noop += 1;
        } else if res.success {
            summary.succeeded += 1;
        } else {
            summary.failed += 1;
        }
        summary.results.push(res);
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_is_dangerous_path() {
        assert!(is_dangerous_path(Path::new("C:\\")));
        assert!(is_dangerous_path(Path::new("C:")));
        assert!(is_dangerous_path(Path::new("C:\\Windows")));
        assert!(is_dangerous_path(Path::new("C:\\Windows\\System32")));
        assert!(is_dangerous_path(Path::new("C:\\Program Files")));
        assert!(!is_dangerous_path(Path::new("C:\\Users\\d4n\\Documents")));
        assert!(!is_dangerous_path(Path::new("D:\\Foto\\Mentah")));
    }

    #[test]
    fn test_resolve_unique_destination() {
        let temp_dir = std::env::temp_dir().join("keyflow_test_unique_dest");
        let _ = fs::create_dir_all(&temp_dir);

        let file_path = temp_dir.join("test_foto.jpg");
        // Jika file belum ada, return file itu sendiri
        let u0 = resolve_unique_destination(&file_path);
        assert_eq!(u0, file_path);

        // Buat file
        File::create(&file_path).unwrap();
        let u1 = resolve_unique_destination(&file_path);
        assert_eq!(u1, temp_dir.join("test_foto (1).jpg"));

        // Buat file (1)
        File::create(&u1).unwrap();
        let u2 = resolve_unique_destination(&file_path);
        assert_eq!(u2, temp_dir.join("test_foto (2).jpg"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_move_single_file_safe() {
        let temp_dir = std::env::temp_dir().join("keyflow_test_file_ops");
        let src_dir = temp_dir.join("src");
        let dst_dir = temp_dir.join("dst");
        let _ = fs::create_dir_all(&src_dir);
        let _ = fs::create_dir_all(&dst_dir);

        let file_path = src_dir.join("sample.txt");
        {
            let mut f = File::create(&file_path).unwrap();
            f.write_all(b"hello keyflow").unwrap();
        }

        let res = move_single_file_safe(&file_path, &dst_dir);
        assert!(res.success);
        assert!(!file_path.exists());
        assert!(dst_dir.join("sample.txt").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

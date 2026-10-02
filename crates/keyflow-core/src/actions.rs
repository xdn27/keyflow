//! Pelaksana aksi file (move/copy/trash/rename) dan penanganan konflik nama (T1.5).
//!
//! WAJIB mematuhi seluruh aturan di `docs/agents/safety.md`:
//! 1. Tidak ada hapus permanen: `trash` selalu ke Recycle Bin/Trash via crate `trash`.
//! 2. Tidak pernah menimpa file tanpa persetujuan eksplisit (`on_conflict: rename` default).
//! 3. Tolak path berbahaya: root drive, direktori sistem OS.
//! 4. Tolak pemindahan folder ke dalam dirinya sendiri.
//! 5. Asal == tujuan: no-op aman.
//! 6. Skenario lintas-drive: copy -> verifikasi ukuran -> hapus sumber hanya bila lolos.
//! 7. Mode `dry_run`: tidak mengubah disk sama sekali.
//! 8. Atomik per file pada multi-file: kegagalan satu file dilaporkan tanpa membatalkan file lain.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::{is_dangerous_system_path, ActionType, OnConflict};

/// Hasil eksekusi untuk satu file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleActionResult {
    pub source: PathBuf,
    pub destination: Option<PathBuf>,
    pub action: ActionType,
    pub success: bool,
    pub is_noop: bool,
    pub is_skipped: bool,
    pub is_dry_run: bool,
    pub error_message: Option<String>,
}

/// Ringkasan hasil eksekusi kumpulan file.
#[derive(Debug, Clone, Default)]
pub struct ActionSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub noop: usize,
    pub results: Vec<SingleActionResult>,
}

/// Konfigurasi eksekusi aksi file.
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    pub dry_run: bool,
    pub on_conflict: OnConflict,
    pub create_missing_dirs: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            on_conflict: OnConflict::Rename,
            create_missing_dirs: true,
        }
    }
}

/// Memeriksa apakah tujuan berada di dalam sumber (mencegah loop rekursif direktori).
pub fn is_destination_inside_source(source: &Path, destination: &Path) -> bool {
    let Ok(src_canon) = source.canonicalize() else {
        return false;
    };
    if let Ok(dst_canon) = destination.canonicalize() {
        return dst_canon.starts_with(&src_canon);
    }
    // Jika destination belum ada di disk, telusuri ancestor terdekat yang ada
    for ancestor in destination.ancestors() {
        if let Ok(anc_canon) = ancestor.canonicalize() {
            return anc_canon.starts_with(&src_canon);
        }
    }
    false
}

/// Menghasilkan path tujuan yang unik untuk mode `on_conflict: rename`.
/// Contoh: `foto.jpg` -> `foto (1).jpg` -> `foto (2).jpg`.
pub fn resolve_unique_destination(dest_path: &Path) -> PathBuf {
    if !dest_path.exists() {
        return dest_path.to_path_buf();
    }

    let parent = dest_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = dest_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let ext = dest_path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let new_name = format!("{stem} ({counter}){ext}");
        let candidate = parent.join(new_name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Menerapkan template penamaan file untuk aksi `rename`.
/// Variabel yang didukung:
/// - `{name}`: nama file lengkap (mis. `foto.jpg`)
/// - `{stem}`: nama file tanpa ekstensi (mis. `foto`)
/// - `{ext}`: ekstensi file tanpa titik (mis. `jpg`)
pub fn apply_rename_template(file_path: &Path, template: &str) -> String {
    let original_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = file_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = file_path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut result = template.replace("{name}", &original_name);
    result = result.replace("{stem}", &stem);
    result = result.replace("{ext}", &ext);
    result
}

/// Menjalankan aksi file pada daftar item terpilih.
pub fn execute_file_action(
    action: &ActionType,
    items: &[PathBuf],
    target_dir: Option<&Path>,
    rename_template: Option<&str>,
    opts: &ExecutionOptions,
) -> ActionSummary {
    let mut summary = ActionSummary {
        total: items.len(),
        succeeded: 0,
        failed: 0,
        skipped: 0,
        noop: 0,
        results: Vec::with_capacity(items.len()),
    };

    for item in items {
        let res = match action {
            ActionType::Move => execute_move_single(item, target_dir, opts),
            ActionType::Copy => execute_copy_single(item, target_dir, opts),
            ActionType::Trash => execute_trash_single(item, opts),
            ActionType::Rename => execute_rename_single(item, rename_template, opts),
            ActionType::Undo => SingleActionResult {
                source: item.clone(),
                destination: None,
                action: ActionType::Undo,
                success: false,
                is_noop: false,
                is_skipped: true,
                is_dry_run: opts.dry_run,
                error_message: Some("Aksi Undo dieksekusi melalui UndoManager".to_string()),
            },
        };

        if res.is_noop {
            summary.noop += 1;
        } else if res.is_skipped {
            summary.skipped += 1;
        } else if res.success {
            summary.succeeded += 1;
        } else {
            summary.failed += 1;
        }

        summary.results.push(res);
    }

    summary
}

/// Eksekusi pemindahan satu file (`move`).
fn execute_move_single(
    source: &Path,
    target_dir: Option<&Path>,
    opts: &ExecutionOptions,
) -> SingleActionResult {
    let Some(dest_dir) = target_dir else {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Move,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Aksi move membutuhkan direktori tujuan ('to')".to_string()),
        };
    };

    let file_name = match source.file_name() {
        Some(name) => name,
        None => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(dest_dir.to_path_buf()),
                action: ActionType::Move,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: Some("Path sumber tidak memiliki nama file yang valid".to_string()),
            };
        }
    };

    let planned_dest = dest_dir.join(file_name);

    // Validasi keamanan path
    if is_dangerous_system_path(source) || is_dangerous_system_path(dest_dir) {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(planned_dest),
            action: ActionType::Move,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Dibatalkan: path sumber atau tujuan berbahaya".to_string()),
        };
    }

    if !source.exists() {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(planned_dest),
            action: ActionType::Move,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some(format!("File sumber tidak ditemukan: {}", source.display())),
        };
    }

    if source.is_dir() && is_destination_inside_source(source, dest_dir) {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(planned_dest),
            action: ActionType::Move,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some(
                "Dibatalkan: direktori tujuan berada di dalam direktori sumber".to_string(),
            ),
        };
    }

    // Asal == tujuan: no-op aman
    if let (Ok(s_canon), Ok(d_canon)) = (source.canonicalize(), planned_dest.canonicalize()) {
        if s_canon == d_canon {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Move,
                success: true,
                is_noop: true,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: None,
            };
        }
    }

    // Resolusi konflik nama
    let final_dest = match resolve_conflict(&planned_dest, opts.on_conflict) {
        Ok(ConflictResolution::Target(path)) => path,
        Ok(ConflictResolution::Skip) => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Move,
                success: false,
                is_noop: false,
                is_skipped: true,
                is_dry_run: opts.dry_run,
                error_message: Some(
                    "File dilewati karena konflik nama (on_conflict: skip)".to_string(),
                ),
            };
        }
        Err(err_msg) => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Move,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: Some(err_msg),
            };
        }
    };

    if opts.dry_run {
        tracing::info!(
            source = %source.display(),
            destination = %final_dest.display(),
            "DRY RUN: Aksi move disimulasikan"
        );
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(final_dest),
            action: ActionType::Move,
            success: true,
            is_noop: false,
            is_skipped: false,
            is_dry_run: true,
            error_message: None,
        };
    }

    // Pastikan folder tujuan ada jika create_missing_dirs = true
    if let Some(parent) = final_dest.parent() {
        if !parent.exists() {
            if opts.create_missing_dirs {
                if let Err(e) = fs::create_dir_all(parent) {
                    return SingleActionResult {
                        source: source.to_path_buf(),
                        destination: Some(final_dest),
                        action: ActionType::Move,
                        success: false,
                        is_noop: false,
                        is_skipped: false,
                        is_dry_run: false,
                        error_message: Some(format!("Gagal membuat folder tujuan: {e}")),
                    };
                }
            } else {
                return SingleActionResult {
                    source: source.to_path_buf(),
                    destination: Some(final_dest),
                    action: ActionType::Move,
                    success: false,
                    is_noop: false,
                    is_skipped: false,
                    is_dry_run: false,
                    error_message: Some(
                        "Folder tujuan tidak ada dan create_missing_dirs dimatikan".to_string(),
                    ),
                };
            }
        }
    }

    // Intent log SEBELUM eksekusi
    tracing::info!(
        source = %source.display(),
        destination = %final_dest.display(),
        timestamp = ?SystemTime::now(),
        "INTENT: Memindahkan file"
    );

    // Eksekusi move
    let res = match fs::rename(source, &final_dest) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device_error(&e) => {
            tracing::warn!(
                source = %source.display(),
                destination = %final_dest.display(),
                "Rename gagal karena batas lintas-drive, melakukan copy-verify-delete"
            );
            move_cross_device(source, &final_dest)
        }
        Err(e) => Err(e),
    };

    match res {
        Ok(()) => {
            tracing::info!(
                source = %source.display(),
                destination = %final_dest.display(),
                "COMPLETED: File berhasil dipindahkan"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(final_dest),
                action: ActionType::Move,
                success: true,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
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
            SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(final_dest),
                action: ActionType::Move,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: Some(e.to_string()),
            }
        }
    }
}

/// Eksekusi penyalinan satu file (`copy`).
fn execute_copy_single(
    source: &Path,
    target_dir: Option<&Path>,
    opts: &ExecutionOptions,
) -> SingleActionResult {
    let Some(dest_dir) = target_dir else {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Copy,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Aksi copy membutuhkan direktori tujuan ('to')".to_string()),
        };
    };

    let file_name = match source.file_name() {
        Some(name) => name,
        None => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(dest_dir.to_path_buf()),
                action: ActionType::Copy,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: Some("Path sumber tidak memiliki nama file yang valid".to_string()),
            };
        }
    };

    let planned_dest = dest_dir.join(file_name);

    if is_dangerous_system_path(source) || is_dangerous_system_path(dest_dir) {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(planned_dest),
            action: ActionType::Copy,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Dibatalkan: path sumber atau tujuan berbahaya".to_string()),
        };
    }

    if !source.exists() {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(planned_dest),
            action: ActionType::Copy,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some(format!("File sumber tidak ditemukan: {}", source.display())),
        };
    }

    if source.is_dir() && is_destination_inside_source(source, dest_dir) {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(planned_dest),
            action: ActionType::Copy,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some(
                "Dibatalkan: direktori tujuan berada di dalam direktori sumber".to_string(),
            ),
        };
    }

    // Asal == tujuan: no-op aman (mencegah O_TRUNC memotong file menjadi 0-byte saat overwrite)
    if let (Ok(s_canon), Ok(d_canon)) = (source.canonicalize(), planned_dest.canonicalize()) {
        if s_canon == d_canon {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Copy,
                success: true,
                is_noop: true,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: None,
            };
        }
    }

    let final_dest = match resolve_conflict(&planned_dest, opts.on_conflict) {
        Ok(ConflictResolution::Target(path)) => path,
        Ok(ConflictResolution::Skip) => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Copy,
                success: false,
                is_noop: false,
                is_skipped: true,
                is_dry_run: opts.dry_run,
                error_message: Some(
                    "File dilewati karena konflik nama (on_conflict: skip)".to_string(),
                ),
            };
        }
        Err(err_msg) => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Copy,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: Some(err_msg),
            };
        }
    };

    if opts.dry_run {
        tracing::info!(
            source = %source.display(),
            destination = %final_dest.display(),
            "DRY RUN: Aksi copy disimulasikan"
        );
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(final_dest),
            action: ActionType::Copy,
            success: true,
            is_noop: false,
            is_skipped: false,
            is_dry_run: true,
            error_message: None,
        };
    }

    if let Some(parent) = final_dest.parent() {
        if !parent.exists() {
            if opts.create_missing_dirs {
                if let Err(e) = fs::create_dir_all(parent) {
                    return SingleActionResult {
                        source: source.to_path_buf(),
                        destination: Some(final_dest),
                        action: ActionType::Copy,
                        success: false,
                        is_noop: false,
                        is_skipped: false,
                        is_dry_run: false,
                        error_message: Some(format!("Gagal membuat folder tujuan: {e}")),
                    };
                }
            } else {
                return SingleActionResult {
                    source: source.to_path_buf(),
                    destination: Some(final_dest),
                    action: ActionType::Copy,
                    success: false,
                    is_noop: false,
                    is_skipped: false,
                    is_dry_run: false,
                    error_message: Some(
                        "Folder tujuan tidak ada dan create_missing_dirs dimatikan".to_string(),
                    ),
                };
            }
        }
    }

    tracing::info!(
        source = %source.display(),
        destination = %final_dest.display(),
        "INTENT: Menyalin file"
    );

    let res = if source.is_dir() {
        copy_dir_recursive(source, &final_dest)
    } else {
        fs::copy(source, &final_dest).map(|_| ())
    };

    match res {
        Ok(()) => {
            tracing::info!(
                source = %source.display(),
                destination = %final_dest.display(),
                "COMPLETED: File berhasil disalin"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(final_dest),
                action: ActionType::Copy,
                success: true,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: None,
            }
        }
        Err(e) => {
            tracing::error!(
                source = %source.display(),
                destination = %final_dest.display(),
                error = %e,
                "FAILED: Gagal menyalin file"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(final_dest),
                action: ActionType::Copy,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: Some(e.to_string()),
            }
        }
    }
}

/// Eksekusi pemindahan ke Recycle Bin (`trash`).
fn execute_trash_single(source: &Path, opts: &ExecutionOptions) -> SingleActionResult {
    if is_dangerous_system_path(source) {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Trash,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Dibatalkan: dilarang menghapus path sistem berbahaya".to_string()),
        };
    }

    if !source.exists() {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Trash,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some(format!("File sumber tidak ditemukan: {}", source.display())),
        };
    }

    if opts.dry_run {
        tracing::info!(
            source = %source.display(),
            "DRY RUN: Aksi trash disimulasikan"
        );
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Trash,
            success: true,
            is_noop: false,
            is_skipped: false,
            is_dry_run: true,
            error_message: None,
        };
    }

    tracing::info!(
        source = %source.display(),
        "INTENT: Memindahkan file ke Recycle Bin/Trash"
    );

    match trash::delete(source) {
        Ok(()) => {
            tracing::info!(
                source = %source.display(),
                "COMPLETED: File berhasil dipindahkan ke Recycle Bin/Trash"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: None,
                action: ActionType::Trash,
                success: true,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: None,
            }
        }
        Err(e) => {
            tracing::error!(
                source = %source.display(),
                error = %e,
                "FAILED: Gagal memindahkan ke Trash"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: None,
                action: ActionType::Trash,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: Some(format!("Gagal memindahkan ke Trash: {e}")),
            }
        }
    }
}

/// Eksekusi penggantian nama file sesuai template (`rename`).
fn execute_rename_single(
    source: &Path,
    template: Option<&str>,
    opts: &ExecutionOptions,
) -> SingleActionResult {
    let Some(tmpl) = template else {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Rename,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Aksi rename membutuhkan template penamaan baru".to_string()),
        };
    };

    if is_dangerous_system_path(source) {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Rename,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Dibatalkan: path sumber berbahaya".to_string()),
        };
    }

    if !source.exists() {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Rename,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some(format!("File sumber tidak ditemukan: {}", source.display())),
        };
    }

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    let new_name = apply_rename_template(source, tmpl);
    if new_name.trim().is_empty() {
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: None,
            action: ActionType::Rename,
            success: false,
            is_noop: false,
            is_skipped: false,
            is_dry_run: opts.dry_run,
            error_message: Some("Hasil template nama file kosong".to_string()),
        };
    }

    let planned_dest = parent.join(new_name);

    // Asal == tujuan: no-op aman
    if let (Ok(s_canon), Ok(d_canon)) = (source.canonicalize(), planned_dest.canonicalize()) {
        if s_canon == d_canon {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Rename,
                success: true,
                is_noop: true,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: None,
            };
        }
    }

    let final_dest = match resolve_conflict(&planned_dest, opts.on_conflict) {
        Ok(ConflictResolution::Target(path)) => path,
        Ok(ConflictResolution::Skip) => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Rename,
                success: false,
                is_noop: false,
                is_skipped: true,
                is_dry_run: opts.dry_run,
                error_message: Some(
                    "File dilewati karena konflik nama (on_conflict: skip)".to_string(),
                ),
            };
        }
        Err(err_msg) => {
            return SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(planned_dest),
                action: ActionType::Rename,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: opts.dry_run,
                error_message: Some(err_msg),
            };
        }
    };

    if opts.dry_run {
        tracing::info!(
            source = %source.display(),
            destination = %final_dest.display(),
            "DRY RUN: Aksi rename disimulasikan"
        );
        return SingleActionResult {
            source: source.to_path_buf(),
            destination: Some(final_dest),
            action: ActionType::Rename,
            success: true,
            is_noop: false,
            is_skipped: false,
            is_dry_run: true,
            error_message: None,
        };
    }

    tracing::info!(
        source = %source.display(),
        destination = %final_dest.display(),
        "INTENT: Mengubah nama file"
    );

    match fs::rename(source, &final_dest) {
        Ok(()) => {
            tracing::info!(
                source = %source.display(),
                destination = %final_dest.display(),
                "COMPLETED: File berhasil di-rename"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(final_dest),
                action: ActionType::Rename,
                success: true,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: None,
            }
        }
        Err(e) => {
            tracing::error!(
                source = %source.display(),
                destination = %final_dest.display(),
                error = %e,
                "FAILED: Gagal me-rename file"
            );
            SingleActionResult {
                source: source.to_path_buf(),
                destination: Some(final_dest),
                action: ActionType::Rename,
                success: false,
                is_noop: false,
                is_skipped: false,
                is_dry_run: false,
                error_message: Some(e.to_string()),
            }
        }
    }
}

enum ConflictResolution {
    Target(PathBuf),
    Skip,
}

/// Menentukan path akhir berdasarkan aturan `on_conflict`.
fn resolve_conflict(
    dest_path: &Path,
    on_conflict: OnConflict,
) -> Result<ConflictResolution, String> {
    if !dest_path.exists() {
        return Ok(ConflictResolution::Target(dest_path.to_path_buf()));
    }

    match on_conflict {
        OnConflict::Rename => Ok(ConflictResolution::Target(resolve_unique_destination(
            dest_path,
        ))),
        OnConflict::Skip => Ok(ConflictResolution::Skip),
        OnConflict::Overwrite => Ok(ConflictResolution::Target(dest_path.to_path_buf())),
        OnConflict::Ask => {
            // Pada mode non-interaktif, ask secara aman beralih ke skip untuk mencegah penimpaan
            tracing::warn!(
                dest = %dest_path.display(),
                "Mode 'ask' di lingkungan non-interaktif: file dilewati secara aman"
            );
            Ok(ConflictResolution::Skip)
        }
    }
}

/// Memeriksa apakah error merupakan error batas lintas-drive/filesystem.
pub(crate) fn is_cross_device_error(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    #[cfg(windows)]
    {
        // ERROR_NOT_SAME_DEVICE = 17 pada Windows
        if let Some(17) = err.raw_os_error() {
            return true;
        }
    }
    #[cfg(unix)]
    {
        // EXDEV = 18 pada Linux/Unix (17 adalah EEXIST, jangan salah deteksi!)
        if let Some(18) = err.raw_os_error() {
            return true;
        }
    }
    false
}

/// Memverifikasi bahwa seluruh isi direktori sumber berhasil disalin ke tujuan dengan benar.
fn verify_dir_copied(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.is_dir() {
        return Err(io::Error::other("Tujuan bukan direktori yang valid"));
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = dst.join(entry.file_name());

        if !target_path.exists() {
            return Err(io::Error::other(format!(
                "Verifikasi direktori gagal: '{}' tidak ditemukan di tujuan",
                target_path.display()
            )));
        }

        if entry_path.is_dir() {
            verify_dir_copied(&entry_path, &target_path)?;
        } else {
            let s_meta = fs::metadata(&entry_path)?;
            let d_meta = fs::metadata(&target_path)?;
            if s_meta.len() != d_meta.len() {
                return Err(io::Error::other(format!(
                    "Verifikasi direktori gagal: ukuran '{}' ({}) != '{}' ({})",
                    entry_path.display(),
                    s_meta.len(),
                    target_path.display(),
                    d_meta.len()
                )));
            }
        }
    }

    Ok(())
}

/// Eksekusi pemindahan lintas-drive:
/// copy -> verifikasi ukuran -> hapus sumber hanya bila lolos.
pub(crate) fn move_cross_device(source: &Path, destination: &Path) -> io::Result<()> {
    let src_meta = fs::metadata(source)?;
    let src_len = src_meta.len();

    if source.is_dir() {
        if let Err(e) = copy_dir_recursive(source, destination) {
            let _ = fs::remove_dir_all(destination);
            return Err(e);
        }
        if let Err(e) = verify_dir_copied(source, destination) {
            let _ = fs::remove_dir_all(destination);
            return Err(e);
        }
        fs::remove_dir_all(source)?;
    } else {
        fs::copy(source, destination)?;
        let dst_meta = match fs::metadata(destination) {
            Ok(m) => m,
            Err(e) => {
                let _ = fs::remove_file(destination);
                return Err(e);
            }
        };

        if dst_meta.len() != src_len {
            let _ = fs::remove_file(destination);
            return Err(io::Error::other(format!(
                "Verifikasi lintas-drive gagal: ukuran tidak cocok (sumber: {src_len}, tujuan: {})",
                dst_meta.len()
            )));
        }

        fs::remove_file(source)?;
    }

    Ok(())
}

/// Menyalin folder secara rekursif.
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = dst.join(entry.file_name());

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &target_path)?;
        } else {
            fs::copy(&entry_path, &target_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_move_with_rename_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let src_file = temp.path().join("gambar.png");
        let dest_dir = temp.path().join("tujuan");
        fs::create_dir_all(&dest_dir).unwrap();

        File::create(&src_file)
            .unwrap()
            .write_all(b"konten 1")
            .unwrap();
        File::create(dest_dir.join("gambar.png"))
            .unwrap()
            .write_all(b"konten 2")
            .unwrap();

        let opts = ExecutionOptions {
            dry_run: false,
            on_conflict: OnConflict::Rename,
            create_missing_dirs: true,
        };

        let summary =
            execute_file_action(&ActionType::Move, &[src_file], Some(&dest_dir), None, &opts);
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 0);

        let final_dest = dest_dir.join("gambar (1).png");
        assert!(final_dest.exists());
        assert!(dest_dir.join("gambar.png").exists()); // File asli tetap ada, tidak tertimpa!
    }

    #[test]
    fn test_dry_run_no_disk_changes() {
        let temp = tempfile::tempdir().unwrap();
        let src_file = temp.path().join("dokumen.txt");
        let dest_dir = temp.path().join("tujuan");

        File::create(&src_file).unwrap().write_all(b"asli").unwrap();

        let opts = ExecutionOptions {
            dry_run: true,
            on_conflict: OnConflict::Rename,
            create_missing_dirs: true,
        };

        let summary = execute_file_action(
            &ActionType::Move,
            std::slice::from_ref(&src_file),
            Some(&dest_dir),
            None,
            &opts,
        );
        assert_eq!(summary.succeeded, 1);
        assert!(src_file.exists()); // File sumber tetap ada di tempat
        assert!(!dest_dir.exists()); // Tidak ada folder atau file yang dibuat di disk
    }

    #[test]
    fn test_rename_template() {
        let p = Path::new("/folder/dokumen.pdf");
        assert_eq!(
            apply_rename_template(p, "{stem}_backup.{ext}"),
            "dokumen_backup.pdf"
        );
        assert_eq!(
            apply_rename_template(p, "PREFIX_{name}"),
            "PREFIX_dokumen.pdf"
        );
    }

    #[test]
    fn test_noop_same_src_dst() {
        let temp = tempfile::tempdir().unwrap();
        let src_file = temp.path().join("tetap.txt");
        File::create(&src_file).unwrap().write_all(b"isi").unwrap();

        let opts = ExecutionOptions::default();
        let summary = execute_file_action(
            &ActionType::Move,
            std::slice::from_ref(&src_file),
            Some(temp.path()),
            None,
            &opts,
        );
        assert_eq!(summary.noop, 1);

        // Copy ke diri sendiri dengan Overwrite: harus tetap no-op dan tidak mengosongkan file (O_TRUNC)
        let copy_opts = ExecutionOptions {
            dry_run: false,
            on_conflict: OnConflict::Overwrite,
            create_missing_dirs: true,
        };
        let copy_summary = execute_file_action(
            &ActionType::Copy,
            std::slice::from_ref(&src_file),
            Some(temp.path()),
            None,
            &copy_opts,
        );
        assert_eq!(copy_summary.noop, 1);
        let content = fs::read_to_string(&src_file).unwrap();
        assert_eq!(content, "isi");
    }

    #[test]
    fn test_prevent_nested_folder_loop() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("parent_folder");
        fs::create_dir_all(&src_dir).unwrap();

        let nested_nonexistent = src_dir.join("sub1").join("sub2");

        assert!(is_destination_inside_source(&src_dir, &nested_nonexistent));

        let opts = ExecutionOptions::default();
        let summary = execute_file_action(
            &ActionType::Move,
            std::slice::from_ref(&src_dir),
            Some(&nested_nonexistent),
            None,
            &opts,
        );
        assert_eq!(summary.failed, 1);

        let copy_summary = execute_file_action(
            &ActionType::Copy,
            std::slice::from_ref(&src_dir),
            Some(&nested_nonexistent),
            None,
            &opts,
        );
        assert_eq!(copy_summary.failed, 1);
    }
}

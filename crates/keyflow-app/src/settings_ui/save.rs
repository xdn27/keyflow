//! Alur muat dan simpan `config.yaml` untuk GUI pengaturan.
//!
//! Aturan keselamatan: config yang tidak valid tidak pernah ditimpa, berkas yang
//! berubah di tempat lain tidak pernah ditimpa diam-diam, hasil selalu divalidasi
//! sebelum menyentuh disk, dan penulisan bersifat atomik (temp file lalu rename).

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use keyflow_core::config::{Config, Settings};
use keyflow_core::config_edit::{patch_settings, render_full, PatchReason, PatchResult};
use thiserror::Error;

/// Hasil memuat config dari disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Berkas belum ada; GUI menampilkan default aman.
    Missing,
    /// Berkas valid; `text` disimpan untuk mendeteksi perubahan di tempat lain.
    Loaded { text: String, settings: Settings },
    /// Berkas ada tetapi tidak valid atau tidak terbaca; GUI tidak boleh menimpanya.
    Invalid { message: String },
}

/// Memuat dan memvalidasi config dari `path`.
pub fn load(path: &Path) -> LoadOutcome {
    match fs::read_to_string(path) {
        Ok(text) => match Config::from_yaml(&text) {
            Ok(config) => LoadOutcome::Loaded {
                settings: config.settings,
                text,
            },
            Err(e) => LoadOutcome::Invalid {
                message: e.to_string(),
            },
        },
        Err(e) if e.kind() == ErrorKind::NotFound => LoadOutcome::Missing,
        Err(e) => LoadOutcome::Invalid {
            message: format!("Gagal membaca {}: {e}", path.display()),
        },
    }
}

/// Cara menyimpan bila penambalan teks tidak memungkinkan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// Hanya menambal blok `settings:`; bila tidak bisa, minta konfirmasi.
    PatchOnly,
    /// Boleh menulis ulang seluruh berkas (komentar hilang, cadangan `.bak` dibuat).
    AllowFullRewrite,
}

/// Hasil upaya menyimpan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    /// Penambalan ditolak; GUI harus meminta konfirmasi sebelum `AllowFullRewrite`.
    NeedsFullRewrite(PatchReason),
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("Berkas config diubah di tempat lain sejak dimuat. Muat ulang sebelum menyimpan.")]
    ChangedOnDisk,
    #[error("Config tidak valid, tidak ada yang ditulis: {0}")]
    Invalid(String),
    #[error("config.yaml bersifat hanya-baca; ubah izinnya dulu bila ingin menyimpan dari GUI.")]
    ReadOnly,
    #[error("Gagal menulis config: {0}")]
    Io(#[from] std::io::Error),
    #[error("Gagal menyiapkan isi config: {0}")]
    Render(String),
}

/// Menyimpan `new` ke `path`. `loaded_text` adalah isi berkas saat dimuat
/// (`None` bila berkas belum ada saat itu).
pub fn save(
    path: &Path,
    loaded_text: Option<&str>,
    new: &Settings,
    mode: SaveMode,
) -> Result<SaveOutcome, SaveError> {
    // Bila config.yaml adalah symlink (mis. dotfiles), tulis ke targetnya agar link tetap utuh.
    let target = resolve_target(path);
    let path = target.as_path();

    let on_disk = read_optional(path)?;
    if on_disk.as_deref() != loaded_text {
        return Err(SaveError::ChangedOnDisk);
    }
    if on_disk.is_some() && fs::metadata(path)?.permissions().readonly() {
        return Err(SaveError::ReadOnly);
    }

    let (new_text, needs_backup) = match &on_disk {
        None => {
            let mut config = Config::default_safe();
            config.settings = new.clone();
            let text = render_full(&config).map_err(|e| SaveError::Render(e.to_string()))?;
            (text, false)
        }
        Some(text) => match patch_settings(text, new) {
            PatchResult::Patched(patched) => (patched, false),
            PatchResult::Unpatchable(PatchReason::InvalidOriginal(message)) => {
                return Err(SaveError::Invalid(message));
            }
            PatchResult::Unpatchable(reason) => match mode {
                SaveMode::PatchOnly => return Ok(SaveOutcome::NeedsFullRewrite(reason)),
                SaveMode::AllowFullRewrite => {
                    let mut config =
                        Config::from_yaml(text).map_err(|e| SaveError::Invalid(e.to_string()))?;
                    config.settings = new.clone();
                    let rendered =
                        render_full(&config).map_err(|e| SaveError::Render(e.to_string()))?;
                    (rendered, true)
                }
            },
        },
    };

    Config::from_yaml(&new_text).map_err(|e| SaveError::Invalid(e.to_string()))?;

    if let (true, Some(old_text)) = (needs_backup, &on_disk) {
        create_backup(path, old_text)?;
    }
    atomic_write(path, &new_text, on_disk.as_deref())?;
    Ok(SaveOutcome::Saved)
}

/// Mengikuti symlink ke berkas sebenarnya; bila tidak bisa (mis. belum ada), pakai path apa adanya.
fn resolve_target(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Membaca teks berkas; `None` bila belum ada.
fn read_optional(path: &Path) -> std::io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Menulis cadangan `config.yaml.bak`, atau `.bak.1`, `.bak.2`, ... bila namanya sudah
/// dipakai. Tidak pernah menimpa berkas yang ada dan tidak mengikuti symlink (`create_new`).
fn create_backup(path: &Path, old_text: &str) -> std::io::Result<PathBuf> {
    let base = path.file_name().unwrap_or_default().to_os_string();
    for n in 0..1000u32 {
        let mut name = base.clone();
        name.push(if n == 0 {
            ".bak".to_string()
        } else {
            format!(".bak.{n}")
        });
        let candidate = path.with_file_name(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                file.write_all(old_text.as_bytes())?;
                file.sync_all()?;
                return Ok(candidate);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "terlalu banyak berkas cadangan config.yaml.bak.*",
    ))
}

/// Menulis lewat berkas sementara di folder yang sama lalu `rename`, sehingga
/// pembaca (termasuk hot-reload) tidak pernah melihat berkas setengah tertulis.
/// Isi berkas diperiksa ulang tepat sebelum `rename`; celah yang tersisa hanya selebar
/// satu panggilan sistem (tanpa lock antarproses).
fn atomic_write(path: &Path, content: &str, expected: Option<&str>) -> Result<(), SaveError> {
    atomic_write_with(path, content, expected, || {})
}

fn atomic_write_with(
    path: &Path,
    content: &str,
    expected: Option<&str>,
    before_rename: impl FnOnce(),
) -> Result<(), SaveError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".tmp-{}", std::process::id()));
    let tmp = path.with_file_name(name);

    let result = (|| -> Result<(), SaveError> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        if let Ok(meta) = fs::metadata(path) {
            // Pertahankan izin asli (mis. 0600) pada berkas pengganti.
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        file.sync_all()?;
        before_rename();
        if read_optional(path)?.as_deref() != expected {
            return Err(SaveError::ChangedOnDisk);
        }
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use keyflow_core::config::{ConfigManager, OnConflict};

    use super::*;

    const SAMPLE: &str = "\
# KeyFlow config
version: 1

settings:
  dry_run: false  # simulasi
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 200

# Profil
profiles:
  - name: \"Foto\"
    context:
      app: file_manager
    rules:
      - key: \"1\"
        action: move
        to: \"D:/Foto/01\"
";

    fn write_sample(dir: &Path) -> PathBuf {
        let path = dir.join("config.yaml");
        fs::write(&path, SAMPLE).unwrap();
        path
    }

    fn loaded(path: &Path) -> (String, Settings) {
        match load(path) {
            LoadOutcome::Loaded { text, settings } => (text, settings),
            other => panic!("seharusnya Loaded: {other:?}"),
        }
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn load_berkas_tidak_ada_valid_dan_rusak() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        assert_eq!(load(&path), LoadOutcome::Missing);

        fs::write(&path, SAMPLE).unwrap();
        assert!(matches!(load(&path), LoadOutcome::Loaded { .. }));

        fs::write(&path, "version: 1\nsettings:\n  dry_run: [salah\n").unwrap();
        assert!(matches!(load(&path), LoadOutcome::Invalid { .. }));
    }

    #[test]
    fn simpan_menambal_nilai_dan_menjaga_komentar() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            on_conflict: OnConflict::Skip,
            ..settings
        };

        let outcome = save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("  dry_run: true  # simulasi\n"));
        assert!(written.contains("# Profil\n"));
        assert_eq!(Config::from_yaml(&written).unwrap().settings, new);
        assert_eq!(files_in(dir.path()), vec!["config.yaml"]);
    }

    #[test]
    fn simpan_ditolak_bila_berkas_berubah_di_tempat_lain() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);
        let edited_elsewhere = SAMPLE.replace("# KeyFlow config", "# diedit di editor");
        fs::write(&path, &edited_elsewhere).unwrap();

        let result = save(&path, Some(&text), &settings, SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(&path).unwrap(), edited_elsewhere);
    }

    #[test]
    fn simpan_ditolak_bila_berkas_baru_muncul_setelah_dimuat_sebagai_tidak_ada() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, SAMPLE).unwrap();

        let result = save(&path, None, &Settings::default(), SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(&path).unwrap(), SAMPLE);
    }

    #[test]
    fn berkas_belum_ada_dibuat_dengan_default_aman_plus_pengaturan_baru() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.yaml");
        let new = Settings {
            dry_run: true,
            ..Settings::default()
        };

        let outcome = save(&path, None, &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.settings, new);
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn bentuk_tak_dikenal_meminta_konfirmasi_dan_tidak_menulis() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let inline = "version: 1\nsettings: {dry_run: false}\n";
        fs::write(&path, inline).unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        let outcome = save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(
            outcome,
            SaveOutcome::NeedsFullRewrite(PatchReason::InlineBlock)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), inline);
        assert_eq!(files_in(dir.path()), vec!["config.yaml"]);
    }

    #[test]
    fn tulis_ulang_penuh_membuat_cadangan_dan_menjaga_profil() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let inline = "version: 1\nsettings: {dry_run: false}\nprofiles:\n  - name: Foto\n";
        fs::write(&path, inline).unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        let outcome = save(&path, Some(&text), &new, SaveMode::AllowFullRewrite).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        assert_eq!(
            fs::read_to_string(dir.path().join("config.yaml.bak")).unwrap(),
            inline
        );
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.settings, new);
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(files_in(dir.path()), vec!["config.yaml", "config.yaml.bak"]);
    }

    #[test]
    fn galat_io_dikembalikan_tanpa_panic() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("bukan_folder");
        fs::write(&blocker, "x").unwrap();
        let path = blocker.join("config.yaml");

        let result = save(&path, None, &Settings::default(), SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::Io(_))));
    }

    #[test]
    fn rename_atomik_memicu_hot_reload_yang_sudah_ada() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);

        let mut manager = ConfigManager::new(path.clone()).unwrap();
        let (tx, rx) = mpsc::channel();
        manager
            .start_hot_reload(move |result| {
                let _ = tx.send(result.map(|cfg| cfg.settings.dry_run));
            })
            .unwrap();
        // Beri waktu watcher terpasang sebelum berkas diubah.
        std::thread::sleep(Duration::from_millis(1000));

        let new = Settings {
            dry_run: true,
            ..settings
        };
        save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        let reloaded = rx.recv_timeout(Duration::from_secs(10));
        manager.stop_hot_reload();
        assert!(
            matches!(reloaded, Ok(Ok(true))),
            "hot-reload tidak menerima config baru: {reloaded:?}"
        );
    }

    #[test]
    fn cadangan_manual_pengguna_tidak_tertimpa() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let inline = "version: 1\nsettings: {dry_run: false}\n";
        fs::write(&path, inline).unwrap();
        fs::write(dir.path().join("config.yaml.bak"), "CADANGAN MANUAL").unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        save(&path, Some(&text), &new, SaveMode::AllowFullRewrite).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join("config.yaml.bak")).unwrap(),
            "CADANGAN MANUAL"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("config.yaml.bak.1")).unwrap(),
            inline
        );
    }

    #[cfg(unix)]
    #[test]
    fn cadangan_tidak_mengikuti_symlink_bak() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let inline = "version: 1\nsettings: {dry_run: false}\n";
        fs::write(&path, inline).unwrap();
        let victim = dir.path().join("dokumen_penting.txt");
        fs::write(&victim, "PENTING").unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join("config.yaml.bak")).unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        save(&path, Some(&text), &new, SaveMode::AllowFullRewrite).unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "PENTING");
    }

    #[cfg(unix)]
    #[test]
    fn config_berupa_symlink_tetap_symlink_dan_targetnya_diperbarui() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().join("dotfiles");
        fs::create_dir(&real_dir).unwrap();
        let real = real_dir.join("keyflow.yaml");
        fs::write(&real, SAMPLE).unwrap();
        let link = dir.path().join("config.yaml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let (text, settings) = loaded(&link);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        let outcome = save(&link, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(fs::read_to_string(&real)
            .unwrap()
            .contains("  dry_run: true  # simulasi\n"));
    }

    #[test]
    fn edit_luar_tepat_sebelum_rename_tidak_tertimpa() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());

        let result = atomic_write_with(&path, "version: 1\n", Some(SAMPLE), || {
            fs::write(&path, "# edit luar\n").unwrap();
        });

        assert!(matches!(result, Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# edit luar\n");
        assert_eq!(files_in(dir.path()), vec!["config.yaml"]);
    }

    #[test]
    fn berkas_yang_muncul_tepat_sebelum_rename_tidak_tertimpa() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");

        let result = atomic_write_with(&path, "version: 1\n", None, || {
            fs::write(&path, "# dibuat proses lain\n").unwrap();
        });

        assert!(matches!(result, Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# dibuat proses lain\n");
        assert_eq!(files_in(dir.path()), vec!["config.yaml"]);
    }

    #[test]
    fn berkas_hanya_baca_ditolak_dan_tidak_berubah() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).unwrap();
        let new = Settings {
            dry_run: true,
            ..settings
        };

        let result = save(&path, Some(&text), &new, SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::ReadOnly)));
        assert_eq!(fs::read_to_string(&path).unwrap(), SAMPLE);
    }

    #[cfg(unix)]
    #[test]
    fn izin_berkas_asli_dipertahankan() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

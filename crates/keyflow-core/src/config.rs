//! Model konfigurasi, parser YAML, validasi, dan hot-reload untuk KeyFlow.
//!
//! Mengimplementasikan T1.1, T1.2, T1.3, dan T1.7 sesuai `docs/agents/config.md`
//! dan `docs/agents/safety.md`.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{Builder, JoinHandle};
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Galat terkait konfigurasi.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("File konfigurasi tidak ditemukan: {0}")]
    NotFound(PathBuf),

    #[error("Gagal membaca file konfigurasi: {0}")]
    Io(#[from] std::io::Error),

    #[error("Kesalahan sintaks YAML pada baris {line}, kolom {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },

    #[error("Versi konfigurasi '{0}' tidak didukung (versi yang didukung: 1)")]
    UnsupportedVersion(u32),

    #[error("Validasi konfigurasi gagal ({count} kesalahan ditemukan):\n{details}")]
    Validation { count: usize, details: String },
}

/// Kesalahan spesifik pada satu elemen konfigurasi (dengan nomor baris/konteks bila ada).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub location: String,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

/// Parser kombinasi tombol (T1.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub key: String,
}

impl KeyCombo {
    /// Membuat KeyCombo sederhana tanpa modifier.
    pub fn simple(key: impl Into<String>) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            key: key.into().to_uppercase(),
        }
    }

    /// Mem-parse kombinasi tombol dari string, contoh: "1", "Ctrl+Shift+Z", "Alt+F4".
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("Tombol tidak boleh kosong".to_string());
        }

        let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
        if parts.is_empty() {
            return Err("Format tombol tidak valid".to_string());
        }

        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut meta = false;
        let mut key_part = None;

        for (idx, part) in parts.iter().enumerate() {
            let lower = part.to_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => {
                    if ctrl {
                        return Err(format!("Modifier 'Ctrl' terduplikasi pada '{s}'"));
                    }
                    ctrl = true;
                }
                "shift" => {
                    if shift {
                        return Err(format!("Modifier 'Shift' terduplikasi pada '{s}'"));
                    }
                    shift = true;
                }
                "alt" => {
                    if alt {
                        return Err(format!("Modifier 'Alt' terduplikasi pada '{s}'"));
                    }
                    alt = true;
                }
                "meta" | "win" | "cmd" | "command" => {
                    if meta {
                        return Err(format!("Modifier 'Meta' terduplikasi pada '{s}'"));
                    }
                    meta = true;
                }
                _ => {
                    if idx != parts.len() - 1 {
                        return Err(format!(
                            "Modifier tidak dikenal atau posisi tombol salah: '{part}' dalam '{s}'"
                        ));
                    }
                    key_part = Some(part.to_string());
                }
            }
        }

        let key = key_part
            .ok_or_else(|| format!("Kombinasi tombol tidak memiliki tombol utama: '{s}'"))?;
        if key.is_empty() {
            return Err(format!("Tombol utama kosong pada '{s}'"));
        }

        Ok(Self {
            ctrl,
            shift,
            alt,
            meta,
            key: key.to_uppercase(),
        })
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.meta {
            parts.push("Meta");
        }
        parts.push(&self.key);
        write!(f, "{}", parts.join("+"))
    }
}

/// Mode penanganan saat terjadi konflik nama file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    #[default]
    Rename,
    Skip,
    Overwrite,
    Ask,
}

/// Pengaturan global KeyFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub dry_run: bool,

    #[serde(default = "default_true")]
    pub notifications: bool,

    #[serde(default)]
    pub on_conflict: OnConflict,

    #[serde(default = "default_true")]
    pub create_missing_dirs: bool,

    #[serde(default = "default_history_limit")]
    pub undo_history_limit: usize,
}

fn default_true() -> bool {
    true
}

fn default_history_limit() -> usize {
    200
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dry_run: false,
            notifications: true,
            on_conflict: OnConflict::Rename,
            create_missing_dirs: true,
            undo_history_limit: 200,
        }
    }
}

/// Filter konteks aktif.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextFilter {
    #[serde(default = "default_app")]
    pub app: String,

    #[serde(default)]
    pub path: Option<String>,

    #[serde(default)]
    pub selection: Option<String>,
}

fn default_app() -> String {
    "file_manager".to_string()
}

impl Default for ContextFilter {
    fn default() -> Self {
        Self {
            app: default_app(),
            path: None,
            selection: None,
        }
    }
}

/// Jenis aksi file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Move,
    Copy,
    Trash,
    Rename,
    Undo,
}

/// Aksi lanjutan setelah rule dijalankan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThenAction {
    SelectNext,
}

/// Konfigurasi rule tombol tunggal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleConfig {
    pub key: String,
    pub action: ActionType,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub then: Option<ThenAction>,
}

/// Profil konfigurasi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub dry_run: Option<bool>,
    #[serde(default)]
    pub context: ContextFilter,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

/// Konfigurasi utama KeyFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub profiles: Vec<ProfileConfig>,
}

impl Config {
    /// Membuat konfigurasi default aman saat belum ada config.
    pub fn default_safe() -> Self {
        Self {
            version: 1,
            settings: Settings::default(),
            profiles: Vec::new(),
        }
    }

    /// Membaca dan memvalidasi konfigurasi dari string YAML.
    pub fn from_yaml(yaml_content: &str) -> Result<Self, ConfigError> {
        let config: Config = serde_norway::from_str(yaml_content).map_err(|e| {
            let (line, column) = match e.location() {
                Some(loc) => (loc.line(), loc.column()),
                None => (1, 1),
            };
            ConfigError::Syntax {
                line,
                column,
                message: e.to_string(),
            }
        })?;

        if config.version != 1 {
            return Err(ConfigError::UnsupportedVersion(config.version));
        }

        config.validate()?;
        Ok(config)
    }

    /// Membaca konfigurasi dari file path.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotFound(path.to_path_buf()));
        }
        let content = fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Memvalidasi seluruh elemen konfigurasi secara komprehensif (mengumpulkan semua galat).
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut errors = Vec::new();

        if self.profiles.is_empty() {
            // Profil kosong diperbolehkan (mis. config awal aman)
            return Ok(());
        }

        for (p_idx, profile) in self.profiles.iter().enumerate() {
            let p_loc = format!("Profil #{}: '{}'", p_idx + 1, profile.name);
            if profile.name.trim().is_empty() {
                errors.push(ValidationError {
                    location: p_loc.clone(),
                    message: "Nama profil tidak boleh kosong".to_string(),
                });
            }

            // Validasi pola glob pada path context bila ada
            if let Some(ref path_pattern) = profile.context.path {
                if let Err(e) = globset::Glob::new(path_pattern) {
                    errors.push(ValidationError {
                        location: format!("{p_loc} -> context.path"),
                        message: format!("Pola glob tidak valid '{path_pattern}': {e}"),
                    });
                }
            }

            // Validasi filter selection bila ada
            if let Some(ref sel) = profile.context.selection {
                let lower = sel.to_lowercase();
                let is_standard = matches!(
                    lower.as_str(),
                    "any" | "image" | "video" | "audio" | "document"
                );
                let is_ext_list = lower.starts_with("ext:[") && lower.ends_with(']');
                if !is_standard && !is_ext_list {
                    errors.push(ValidationError {
                        location: format!("{p_loc} -> context.selection"),
                        message: format!(
                            "Filter seleksi '{sel}' tidak dikenal (pilihan: any, image, video, audio, document, atau ext:[...])"
                        ),
                    });
                }
            }

            // Validasi duplikasi tombol dan aturan per rule
            let mut seen_keys = HashSet::new();
            for (r_idx, rule) in profile.rules.iter().enumerate() {
                let r_loc = format!("{p_loc} -> Rule #{}: '{}'", r_idx + 1, rule.key);

                // 1. Validasi format tombol
                match KeyCombo::parse(&rule.key) {
                    Ok(combo) => {
                        let normalized = combo.to_string();
                        if !seen_keys.insert(normalized) {
                            errors.push(ValidationError {
                                location: r_loc.clone(),
                                message: format!(
                                    "Tombol '{}' terduplikasi dalam profil ini",
                                    rule.key
                                ),
                            });
                        }
                    }
                    Err(e) => {
                        errors.push(ValidationError {
                            location: r_loc.clone(),
                            message: format!("Format tombol tidak valid: {e}"),
                        });
                    }
                }

                // 2. Validasi field wajib per aksi
                match rule.action {
                    ActionType::Move | ActionType::Copy => {
                        if rule.to.is_none() || rule.to.as_deref().unwrap_or("").trim().is_empty() {
                            errors.push(ValidationError {
                                location: r_loc.clone(),
                                message: format!(
                                    "Aksi '{:?}' membutuhkan target folder tujuan ('to')",
                                    rule.action
                                ),
                            });
                        } else if let Some(ref target) = rule.to {
                            let p = Path::new(target);
                            if is_dangerous_system_path(p) {
                                errors.push(ValidationError {
                                    location: r_loc.clone(),
                                    message: format!(
                                        "Target tujuan '{}' dilarang karena merupakan path sistem berbahaya",
                                        target
                                    ),
                                });
                            }
                        }
                    }
                    ActionType::Rename => {
                        if rule.template.is_none()
                            || rule.template.as_deref().unwrap_or("").trim().is_empty()
                        {
                            errors.push(ValidationError {
                                location: r_loc.clone(),
                                message:
                                    "Aksi 'rename' membutuhkan template penamaan baru ('template')"
                                        .to_string(),
                            });
                        }
                    }
                    ActionType::Trash | ActionType::Undo => {
                        // Tidak butuh parameter 'to' atau 'template'
                    }
                }
            }
        }

        if !errors.is_empty() {
            let count = errors.len();
            let details = errors
                .iter()
                .map(|e| format!("  - {e}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(ConfigError::Validation { count, details });
        }

        Ok(())
    }
}

/// Memeriksa apakah path merupakan root drive atau path sistem berbahaya.
pub fn is_dangerous_system_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let normalized = s.trim().replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');

    // Root drive atau direktori kosong
    if trimmed.is_empty() || trimmed == "/" {
        return true;
    }

    // Windows drive root: e.g. "C:" atau "D:"
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        return true;
    }

    let upper = trimmed.to_uppercase();

    // Direktori sistem yang seluruh isinya dilarang
    let system_trees = [
        "C:/WINDOWS",
        "C:/PROGRAM FILES",
        "C:/PROGRAM FILES (X86)",
        "C:/SYSTEM VOLUME INFORMATION",
        "/BIN",
        "/SBIN",
        "/USR",
        "/ETC",
        "/SYSTEM",
        "/BOOT",
        "/ROOT",
    ];

    for d in system_trees {
        if upper == d || upper.starts_with(&format!("{d}/")) {
            return true;
        }
    }

    // Root folder pengguna (dilarang menaruh langsung di root pengguna tanpa spesifikasi user)
    let user_roots = ["C:/USERS", "/HOME", "/USERS"];
    for u in user_roots {
        if upper == u {
            return true;
        }
    }

    false
}

/// Pengelola hot-reload config secara atomik (T1.7).
pub struct ConfigManager {
    config_path: PathBuf,
    current_config: Arc<RwLock<Arc<Config>>>,
    is_watching: Arc<AtomicBool>,
    watcher_thread: Option<JoinHandle<()>>,
}

impl ConfigManager {
    /// Inisialisasi pengelola config dengan path target.
    pub fn new(config_path: PathBuf) -> Result<Self, ConfigError> {
        let abs_path = if config_path.is_absolute() {
            config_path
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&config_path))
                .unwrap_or(config_path)
        };

        let initial_config = if abs_path.exists() {
            Config::from_file(&abs_path)?
        } else {
            Config::default_safe()
        };

        Ok(Self {
            config_path: abs_path,
            current_config: Arc::new(RwLock::new(Arc::new(initial_config))),
            is_watching: Arc::new(AtomicBool::new(false)),
            watcher_thread: None,
        })
    }

    /// Mengambil salinan thread-safe (Arc) dari konfigurasi aktif saat ini.
    pub fn get_config(&self) -> Arc<Config> {
        self.current_config
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| Arc::new(Config::default_safe()))
    }

    /// Memulai watcher hot-reload file dengan debounce.
    pub fn start_hot_reload<F>(&mut self, on_reload: F) -> Result<(), std::io::Error>
    where
        F: Fn(Result<Arc<Config>, ConfigError>) + Send + Sync + 'static,
    {
        if self.is_watching.load(Ordering::SeqCst) {
            return Ok(());
        }

        let is_watching = self.is_watching.clone();
        let path = self.config_path.clone();
        let current_config = self.current_config.clone();
        let on_reload = Arc::new(on_reload);

        let parent_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let handle = Builder::new()
            .name("keyflow-config-watcher".into())
            .spawn(move || {
                let (tx, rx) = std::sync::mpsc::channel();
                let mut watcher = match notify::recommended_watcher(tx) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::error!("Gagal membuat notify watcher: {e}");
                        return;
                    }
                };

                if let Err(e) = watcher.watch(&parent_dir, RecursiveMode::NonRecursive) {
                    tracing::error!("Gagal memantau direktori config: {e}");
                    return;
                }

                is_watching.store(true, Ordering::SeqCst);

                let debounce_dur = Duration::from_millis(250);
                let mut last_event_time = std::time::Instant::now();

                while is_watching.load(Ordering::SeqCst) {
                    if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(500)) {
                        let is_modify = matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_)
                        );
                        let targets_config = event.paths.iter().any(|p| p == &path);

                        if is_modify && targets_config {
                            if last_event_time.elapsed() < debounce_dur {
                                continue;
                            }
                            last_event_time = std::time::Instant::now();

                            tracing::info!("Perubahan pada file config terdeteksi: {}", path.display());
                            match Config::from_file(&path) {
                                Ok(new_cfg) => {
                                    let new_arc = Arc::new(new_cfg);
                                    if let Ok(mut guard) = current_config.write() {
                                        *guard = new_arc.clone();
                                    }
                                    tracing::info!("Config berhasil diperbarui via hot-reload");
                                    on_reload(Ok(new_arc));
                                }
                                Err(err) => {
                                    tracing::error!(
                                        "Gagal memuat config baru (tetap memakai config lama): {err}"
                                    );
                                    on_reload(Err(err));
                                }
                            }
                        }
                    }
                }
            })?;

        self.watcher_thread = Some(handle);
        Ok(())
    }

    /// Menghentikan hot-reload.
    pub fn stop_hot_reload(&mut self) {
        self.is_watching.store(false, Ordering::SeqCst);
        if let Some(h) = self.watcher_thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for ConfigManager {
    fn drop(&mut self) {
        self.stop_hot_reload();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_yaml() {
        let yaml = r#"
version: 1
settings:
  dry_run: false
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 100

profiles:
  - name: "Sorting Foto"
    enabled: true
    context:
      app: file_manager
      path: "D:/Foto/**"
      selection: image
    rules:
      - key: "1"
        action: move
        to: "D:/Foto/Dipakai"
        then: select_next
      - key: "Ctrl+Shift+Z"
        action: undo
"#;
        let cfg = Config::from_yaml(yaml).expect("Harus valid");
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.settings.undo_history_limit, 100);
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].rules.len(), 2);
    }

    #[test]
    fn test_unsupported_version() {
        let yaml = "version: 2\nprofiles: []\n";
        let err = Config::from_yaml(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::UnsupportedVersion(2)));
    }

    #[test]
    fn test_key_combo_parsing() {
        let k1 = KeyCombo::parse("1").unwrap();
        assert_eq!(k1.key, "1");
        assert!(!k1.ctrl);

        let k2 = KeyCombo::parse("Ctrl+Shift+Z").unwrap();
        assert_eq!(k2.key, "Z");
        assert!(k2.ctrl);
        assert!(k2.shift);
        assert!(!k2.alt);

        assert!(KeyCombo::parse("").is_err());
        assert!(KeyCombo::parse("Ctrl+Ctrl+A").is_err());
    }

    #[test]
    fn test_validation_errors_collected() {
        let yaml = r#"
version: 1
profiles:
  - name: "Profil Rusak"
    rules:
      - key: "1"
        action: move
      - key: "1"
        action: copy
        to: "C:/Windows"
"#;
        let err = Config::from_yaml(yaml).unwrap_err();
        if let ConfigError::Validation { count, details } = err {
            assert!(
                count >= 2,
                "Harus mengumpulkan beberapa error sekaligus: {details}"
            );
        } else {
            panic!("Harus ConfigError::Validation");
        }
    }

    #[test]
    fn test_is_dangerous_system_path() {
        assert!(is_dangerous_system_path(Path::new("/")));
        assert!(is_dangerous_system_path(Path::new("C:")));
        assert!(is_dangerous_system_path(Path::new("C:/")));
        assert!(is_dangerous_system_path(Path::new("C:\\")));
        assert!(is_dangerous_system_path(Path::new("C:/Windows")));
        assert!(is_dangerous_system_path(Path::new("C:\\Windows\\System32")));
        assert!(is_dangerous_system_path(Path::new("/bin/bash")));
        assert!(is_dangerous_system_path(Path::new("/usr/lib")));
        assert!(is_dangerous_system_path(Path::new("/etc")));
        assert!(is_dangerous_system_path(Path::new("C:/Program Files")));
        assert!(is_dangerous_system_path(Path::new("C:/Users")));

        // Path aman
        assert!(!is_dangerous_system_path(Path::new("/home/user/Documents")));
        assert!(!is_dangerous_system_path(Path::new(
            "C:/Users/User/Downloads"
        )));
        assert!(!is_dangerous_system_path(Path::new("D:/Projects/Rust")));
    }
}

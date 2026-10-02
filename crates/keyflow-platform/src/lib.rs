//! Trait platform dan implementasi per OS. Lihat `docs/agents/architecture.md`
//! dan `docs/agents/platforms.md`.
//!
//! Kode `unsafe` hanya boleh ada di modul platform, dibungkus API aman, dan
//! setiap blok diberi komentar `// SAFETY:`.

use std::fmt;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub mod mock;

/// Kesalahan dari lapisan platform.
#[derive(Debug)]
pub enum PlatformError {
    /// Fitur tidak didukung di platform/sesi ini (mis. Wayland).
    Unsupported(String),
    /// Izin OS belum diberikan (mis. Accessibility di macOS).
    PermissionDenied(String),
    /// Kegagalan lain dari OS.
    Os(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(m) => write!(f, "tidak didukung: {m}"),
            Self::PermissionDenied(m) => write!(f, "izin ditolak: {m}"),
            Self::Os(m) => write!(f, "kesalahan OS: {m}"),
        }
    }
}

impl std::error::Error for PlatformError {}

pub type Result<T> = std::result::Result<T, PlatformError>;

/// Keputusan hook untuk satu key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookDecision {
    /// Tombol ditelan: tidak diteruskan ke aplikasi.
    Swallow,
    /// Tombol diteruskan seperti biasa. Default bila ragu (fail-open).
    PassThrough,
}

/// Informasi window yang sedang fokus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub process_name: String,
    pub title: String,
}

/// Event keyboard mentah dari hook.
///
/// TODO(M0/M2): lengkapi (kode tombol, modifier, down/up, penanda event buatan sendiri).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: String,
    pub pressed: bool,
}

/// Hook keyboard global.
///
/// `handler` dipanggil SINKRON di dalam hook dan harus selesai dalam
/// hitungan milidetik: tanpa I/O, COM, lock panjang, atau panic.
pub trait KeyboardHook {
    fn start(&self, handler: Box<dyn Fn(KeyEvent) -> HookDecision + Send + Sync>) -> Result<()>;
    fn stop(&self);
}

/// Konteks file manager yang sedang fokus.
pub trait FileManagerContext {
    fn focused_window(&self) -> Result<WindowInfo>;
    fn current_folder(&self) -> Result<Option<PathBuf>>;
    fn selected_items(&self) -> Result<Vec<PathBuf>>;
    /// Best-effort: memajukan seleksi ke item berikutnya.
    fn select_next(&self) -> Result<()>;
}

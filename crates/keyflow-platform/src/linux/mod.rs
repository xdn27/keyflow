//! Implementasi platform Linux (X11; deteksi Wayland) untuk KeyFlow (T4.1–T4.4).

pub mod cache;
pub mod context;
pub mod hook;
pub mod notification;
pub mod wayland;

pub use cache::{CachedContext, LinuxContextCache};
pub use context::LinuxFileManagerContext;
pub use hook::LinuxX11HookManager;
pub use notification::show_notification;
pub use wayland::{is_wayland_session, wayland_warning_message};

use std::sync::atomic::AtomicBool;

/// Flag penanda bahwa event keyboard saat ini dihasilkan secara sintetis oleh KeyFlow
/// (misalnya dari `select_next` atau fallback clipboard `Ctrl+C`).
/// Digunakan oleh hook keyboard Linux X11 untuk mencegah infinite event loop.
pub static IS_SYNTHETIC_LINUX_EVENT: AtomicBool = AtomicBool::new(false);

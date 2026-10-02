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

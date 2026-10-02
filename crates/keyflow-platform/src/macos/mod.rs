//! Implementasi macOS (`CGEventTap`, AppleScript). Lihat `docs/agents/platforms.md`.

pub mod cache;
pub mod finder;
pub mod hook;
pub mod notification;
pub mod permissions;

pub use cache::{CachedContext, ContextCache};
pub use finder::MacosFinderContext;
pub use hook::{MacosHookManager, KEYFLOW_MACOS_USER_DATA};
pub use notification::show_notification;
pub use permissions::{
    is_accessibility_trusted, request_accessibility_permissions, verify_macos_permissions,
};

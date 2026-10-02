//! Implementasi Windows (hook `WH_KEYBOARD_LL`, COM Shell). Lihat `docs/agents/platforms.md`.
//!
//! Modul spike M0:
//! - T0.1: `hook` (`WindowsHookManager`)
//! - T0.2: `shell` (`WindowsShellClient`, `ExplorerShellContext`)
//! - T0.3: `file_ops` (`move_selected_files`, `move_single_file_safe`)

pub mod file_ops;
pub mod hook;
pub mod shell;

pub use file_ops::{is_dangerous_path, move_selected_files, move_single_file_safe, MoveSummary};
pub use hook::{get_latency_stats, is_explorer_window, HookLatencyStats, WindowsHookManager};
pub use shell::{ExplorerShellContext, WindowsShellClient};

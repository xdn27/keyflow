//! Implementasi Windows (hook `WH_KEYBOARD_LL`, COM Shell). Lihat `docs/agents/platforms.md`.
//!
//! Modul spike M0:
//! - T0.1: `hook` (`WindowsHookManager`)
//! - T0.2: `shell` (`WindowsShellClient`, `ExplorerShellContext`)
//! - T0.3: `file_ops` (`move_selected_files`, `move_single_file_safe`)

pub mod cache;
pub mod file_ops;
pub mod hook;
pub mod notification;
pub mod shell;

pub use cache::{CachedContext, ContextCache};
pub use file_ops::{is_dangerous_path, move_selected_files, move_single_file_safe, MoveSummary};
pub use hook::{
    get_latency_stats, is_explorer_window, HookLatencyStats, WindowsHookManager, KEYFLOW_EXTRA_INFO,
};
pub use notification::show_notification;
pub use shell::{ExplorerShellContext, WindowsFileManagerContext, WindowsShellClient};

use ::windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, PostQuitMessage, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

/// Memproses pesan-pesan Windows yang tertunda untuk thread saat ini tanpa memblokir.
/// Mengembalikan `false` jika menerima pesan `WM_QUIT`, atau `true` jika terus berjalan.
pub fn process_pending_windows_messages() -> bool {
    let mut msg = MSG::default();
    // SAFETY: PeekMessageW dengan PM_REMOVE aman digunakan untuk memeriksa dan mengambil pesan thread.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

/// Mengirim sinyal keluar WM_QUIT ke antrian thread saat ini.
pub fn post_quit_message(exit_code: i32) {
    // SAFETY: PostQuitMessage mengirim WM_QUIT ke thread saat ini.
    unsafe {
        PostQuitMessage(exit_code);
    }
}

/// Mengambil handle (HWND) jendela yang sedang aktif saat ini sebagai integer.
pub fn get_foreground_window_hwnd() -> isize {
    use ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: GetForegroundWindow cepat (< 50ns) dan aman dipanggil kapan saja.
    unsafe { GetForegroundWindow().0 as isize }
}

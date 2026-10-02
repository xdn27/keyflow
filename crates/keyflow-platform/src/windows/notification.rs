//! Notifikasi desktop Windows native via Win32 Shell API (T2.5).

use ::windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_INFO, NIIF_INFO, NIM_ADD, NIM_MODIFY, NOTIFYICONDATAW,
};
use ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// Menampilkan notifikasi desktop Windows native.
pub fn show_notification(title: &str, message: &str) {
    tracing::info!(title, message, "Menampilkan notifikasi desktop Windows");

    // SAFETY: GetForegroundWindow untuk mendapatkan HWND asosiasi notifikasi
    let hwnd = unsafe { GetForegroundWindow() };

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 0x5946, // "YF"
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_INFO,
        ..Default::default()
    };

    // Salin szInfoTitle (maksimal 64 karakter UTF-16, dijamin null-terminated)
    let title_utf16: Vec<u16> = title.encode_utf16().collect();
    let max_title_chars = nid.szInfoTitle.len().saturating_sub(1);
    let title_copy_len = title_utf16.len().min(max_title_chars);
    nid.szInfoTitle[..title_copy_len].copy_from_slice(&title_utf16[..title_copy_len]);
    nid.szInfoTitle[title_copy_len] = 0;

    // Salin szInfo (maksimal 256 karakter UTF-16, dijamin null-terminated)
    let msg_utf16: Vec<u16> = message.encode_utf16().collect();
    let max_msg_chars = nid.szInfo.len().saturating_sub(1);
    let msg_copy_len = msg_utf16.len().min(max_msg_chars);
    nid.szInfo[..msg_copy_len].copy_from_slice(&msg_utf16[..msg_copy_len]);
    nid.szInfo[msg_copy_len] = 0;

    // SAFETY: Shell_NotifyIconW dengan NIM_MODIFY atau NIM_ADD menampilkan balloon/toast notification
    unsafe {
        if !Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
    }
}

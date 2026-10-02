//! Notifikasi desktop macOS native via AppleScript (T3.4).

use std::process::Command;

/// Menampilkan notifikasi desktop macOS native.
pub fn show_notification(title: &str, message: &str) {
    tracing::info!(title, message, "Menampilkan notifikasi desktop macOS");

    let clean_title = title
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', "");
    let clean_msg = message
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', "");

    let script = format!(r#"display notification "{clean_msg}" with title "{clean_title}""#);

    let _ = Command::new("osascript").arg("-e").arg(&script).spawn();
}

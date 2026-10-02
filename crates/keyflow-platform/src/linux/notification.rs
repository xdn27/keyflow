//! Notifikasi desktop Linux native via FreeDesktop notify-send (T4.4).

use std::process::Command;

/// Menampilkan notifikasi desktop native pada Linux.
pub fn show_notification(title: &str, message: &str) {
    tracing::info!(title, message, "Menampilkan notifikasi desktop Linux");

    // notify-send adalah standar FreeDesktop yang tersedia di semua DE Linux utama
    // (GNOME, KDE Plasma, XFCE, Cinnamon, MATE, Sway, Hyprland, i3)
    let res = Command::new("notify-send")
        .arg("-a")
        .arg("KeyFlow")
        .arg(title)
        .arg(message)
        .spawn();

    if let Err(e) = res {
        tracing::warn!("Gagal memanggil notify-send: {e}. Notifikasi hanya dicatat di log.");
    }
}

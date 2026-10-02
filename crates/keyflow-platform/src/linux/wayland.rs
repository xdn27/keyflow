//! Deteksi sesi Wayland dan penanganan peringatan keterbatasan protokol (T4.3).

use std::env;

/// Mendeteksi apakah sesi desktop saat ini berjalan di bawah Wayland.
pub fn is_wayland_session() -> bool {
    if let Ok(session_type) = env::var("XDG_SESSION_TYPE") {
        if session_type.eq_ignore_ascii_case("wayland") {
            return true;
        }
    }

    if env::var("WAYLAND_DISPLAY").is_ok() {
        return true;
    }

    false
}

/// Menghasilkan pesan peringatan resmi tentang batasan keamanan Wayland.
pub fn wayland_warning_message() -> &'static str {
    "Sesi Wayland terdeteksi. Protokol keamanan Wayland membatasi global keyboard hook dan deteksi jendela fokus lintas-aplikasi. KeyFlow beroperasi dalam mode fail-open (tombol diteruskan ke sistem tanpa modifikasi)."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warning_message_not_empty() {
        assert!(!wayland_warning_message().is_empty());
    }
}

//! Konteks file manager Linux via X11 protocol dan fallback clipboard (T4.1 & T4.2).
//!
//! Mendukung pembacaan window fokus via `_NET_ACTIVE_WINDOW` dan `WM_CLASS`,
//! serta pengambilan seleksi file via clipboard `text/uri-list` standar FreeDesktop.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::{FileManagerContext, PlatformError, Result, WindowInfo};

const X11_KEYCODE_CTRL_L: u8 = 37;
const X11_KEYCODE_C: u8 = 54;
const X11_KEYCODE_DOWN: u8 = 116;

/// Memeriksa apakah nama proses atau class window adalah file manager yang didukung.
pub fn is_file_manager_process(process_name: &str) -> bool {
    let lower = process_name.to_lowercase();
    lower.contains("nautilus")
        || lower.contains("dolphin")
        || lower.contains("thunar")
        || lower.contains("nemo")
        || lower.contains("pcmanfm")
        || lower.contains("caja")
        || lower.contains("files")
        || lower.contains("explorer")
}

/// Implementasi `FileManagerContext` untuk lingkungan Linux X11.
#[derive(Debug, Default, Clone)]
pub struct LinuxFileManagerContext;

impl LinuxFileManagerContext {
    pub fn new() -> Self {
        Self
    }

    /// Membuka koneksi baru ke X11 server.
    fn connect_x11(&self) -> Result<(RustConnection, usize)> {
        x11rb::connect(None)
            .map_err(|e| PlatformError::Os(format!("Gagal terhubung ke X11 server: {e}")))
    }

    /// Membaca properti teks UTF-8 atau Latin-1 dari window X11.
    fn read_window_property(
        &self,
        conn: &RustConnection,
        window: u32,
        atom: Atom,
        target_type: Atom,
    ) -> Option<String> {
        let reply = conn
            .get_property(false, window, atom, target_type, 0, 1024)
            .ok()?
            .reply()
            .ok()?;

        if reply.value.is_empty() {
            return None;
        }

        Some(String::from_utf8_lossy(&reply.value).to_string())
    }

    /// Membaca nama proses/class dari `WM_CLASS` (format: `instance\0class\0`).
    fn read_wm_class(&self, conn: &RustConnection, window: u32) -> String {
        let reply = conn
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
            .ok()
            .and_then(|cookie| cookie.reply().ok());

        let Some(prop) = reply else {
            return String::new();
        };

        if prop.value.is_empty() {
            return String::new();
        }

        // Ambil elemen string pertama non-kosong dari format null-terminated.
        // Normalisasi format reverse-DNS (mis. "org.gnome.Nautilus" -> "Nautilus")
        let parts: Vec<&[u8]> = prop.value.split(|&b| b == 0).collect();
        for part in parts {
            if !part.is_empty() {
                let raw_class = String::from_utf8_lossy(part).trim().to_string();
                if let Some(pos) = raw_class.rfind('.') {
                    return raw_class[pos + 1..].to_string();
                }
                return raw_class;
            }
        }

        String::new()
    }

    /// Membaca clipboard X11 untuk target `text/uri-list` via window dummy sementara.
    fn read_clipboard_uri_list(&self, conn: &RustConnection, root: u32) -> Result<Vec<PathBuf>> {
        let clipboard_atom = conn
            .intern_atom(false, b"CLIPBOARD")
            .map_err(|e| PlatformError::Os(format!("Gagal intern CLIPBOARD atom: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Os(format!("Gagal reply CLIPBOARD atom: {e}")))?
            .atom;

        let uri_list_atom = conn
            .intern_atom(false, b"text/uri-list")
            .map_err(|e| PlatformError::Os(format!("Gagal intern text/uri-list atom: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Os(format!("Gagal reply text/uri-list atom: {e}")))?
            .atom;

        let prop_atom = conn
            .intern_atom(false, b"KEYFLOW_SELECTION_PROP")
            .map_err(|e| PlatformError::Os(format!("Gagal intern selection prop atom: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Os(format!("Gagal reply selection prop atom: {e}")))?
            .atom;

        // Buat dummy window tak kasat mata untuk menerima SelectionNotify
        let dummy_win = conn
            .generate_id()
            .map_err(|e| PlatformError::Os(format!("Gagal membuat ID window X11: {e}")))?;

        conn.create_window(
            0,
            dummy_win,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(|e| PlatformError::Os(format!("Gagal membuat dummy window: {e}")))?;

        // Minta konversi clipboard ke target text/uri-list
        conn.convert_selection(
            dummy_win,
            clipboard_atom,
            uri_list_atom,
            prop_atom,
            x11rb::CURRENT_TIME,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal convert_selection: {e}")))?;

        conn.flush()
            .map_err(|e| PlatformError::Os(format!("Gagal flush X11: {e}")))?;

        // Tunggu event respons selama maksimal 400ms
        let start_wait = Instant::now();
        let mut raw_data = Vec::new();

        while start_wait.elapsed() < Duration::from_millis(400) {
            if let Ok(Some(x11rb::protocol::Event::SelectionNotify(ev))) = conn.poll_for_event() {
                if ev.property == 0 {
                    // Konversi seleksi ditolak atau seleksi kosong: keluar instan
                    break;
                }
                if ev.property == prop_atom {
                    let mut offset = 0u32;
                    loop {
                        let Ok(reply) = conn.get_property(
                            false,
                            dummy_win,
                            prop_atom,
                            AtomEnum::ANY,
                            offset,
                            8192,
                        ) else {
                            break;
                        };
                        let Ok(prop) = reply.reply() else {
                            break;
                        };
                        if prop.value.is_empty() {
                            break;
                        }
                        let chunk_len = (prop.value.len() / 4) as u32;
                        raw_data.extend_from_slice(&prop.value);
                        if prop.bytes_after == 0 {
                            break;
                        }
                        offset += chunk_len;
                    }
                    break;
                }
            }
            thread::sleep(Duration::from_millis(20));
        }

        // Hapus dummy window
        let _ = conn.destroy_window(dummy_win);
        let _ = conn.flush();

        if raw_data.is_empty() {
            return Ok(Vec::new());
        }

        let content = String::from_utf8_lossy(&raw_data);
        let mut items = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some(path) = decode_file_uri(trimmed) {
                if path.exists() {
                    items.push(path);
                }
            }
        }

        Ok(items)
    }

    /// Membaca window fokus menggunakan koneksi X11 dan atom yang sudah ada (efisien untuk polling cache).
    pub fn read_focused_window_fast(
        &self,
        conn: &RustConnection,
        root: u32,
        net_active_window: Atom,
        net_wm_name: Atom,
        utf8_atom: Atom,
    ) -> WindowInfo {
        let Ok(cookie) = conn.get_property(false, root, net_active_window, AtomEnum::WINDOW, 0, 1)
        else {
            return WindowInfo {
                process_name: String::new(),
                title: String::new(),
            };
        };
        let Ok(prop) = cookie.reply() else {
            return WindowInfo {
                process_name: String::new(),
                title: String::new(),
            };
        };
        let Some(active_win) = prop.value32().and_then(|mut it| it.next()) else {
            return WindowInfo {
                process_name: String::new(),
                title: String::new(),
            };
        };
        if active_win == 0 {
            return WindowInfo {
                process_name: String::new(),
                title: String::new(),
            };
        }

        let proc_name = self.read_wm_class(conn, active_win);
        let title = self
            .read_window_property(conn, active_win, net_wm_name, utf8_atom)
            .or_else(|| {
                self.read_window_property(
                    conn,
                    active_win,
                    AtomEnum::WM_NAME.into(),
                    AtomEnum::STRING.into(),
                )
            })
            .unwrap_or_default();

        WindowInfo {
            process_name: proc_name,
            title,
        }
    }
}

/// Mendekode URI berformat `file:///...` atau path absolut ke `PathBuf`.
pub fn decode_file_uri(uri: &str) -> Option<PathBuf> {
    let clean = uri.trim();
    if clean.is_empty() {
        return None;
    }

    let path_str = if let Some(stripped) = clean.strip_prefix("file://") {
        if let Some(after_localhost) = stripped.strip_prefix("localhost") {
            after_localhost
        } else {
            stripped
        }
    } else if clean.starts_with('/') {
        clean
    } else {
        return None;
    };

    // Percent-decoding manual (%20 -> ' ', dll.)
    let bytes = path_str.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h1), Some(h2)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                decoded.push((h1 << 4) | h2);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }

    let decoded_str = String::from_utf8(decoded).ok()?;
    Some(PathBuf::from(decoded_str))
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

impl FileManagerContext for LinuxFileManagerContext {
    fn focused_window(&self) -> Result<WindowInfo> {
        let (conn, screen_num) = self.connect_x11()?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        let net_active_window = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .map_err(|e| PlatformError::Os(format!("Gagal intern _NET_ACTIVE_WINDOW: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Os(format!("Gagal reply _NET_ACTIVE_WINDOW: {e}")))?
            .atom;

        let utf8_atom = conn
            .intern_atom(false, b"UTF8_STRING")
            .map_err(|e| PlatformError::Os(format!("Gagal intern UTF8_STRING: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Os(format!("Gagal reply UTF8_STRING: {e}")))?
            .atom;

        let net_wm_name = conn
            .intern_atom(false, b"_NET_WM_NAME")
            .map_err(|e| PlatformError::Os(format!("Gagal intern _NET_WM_NAME: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Os(format!("Gagal reply _NET_WM_NAME: {e}")))?
            .atom;

        Ok(self.read_focused_window_fast(&conn, root, net_active_window, net_wm_name, utf8_atom))
    }

    fn current_folder(&self) -> Result<Option<PathBuf>> {
        let win = self.focused_window()?;
        if !is_file_manager_process(&win.process_name) {
            return Ok(None);
        }

        // Analisis judul jendela file manager (Nautilus, Dolphin, Thunar)
        let title = win.title.trim();
        if title.starts_with('/') {
            let candidate = PathBuf::from(title);
            if candidate.is_dir() {
                return Ok(Some(candidate));
            }
        }

        // Deteksi sub-string path absolut di dalam judul (mis. "folder - /home/user/dir")
        for word in title.split(['—', '-', ':']) {
            let trimmed = word.trim();
            if trimmed.starts_with('/') {
                let candidate = PathBuf::from(trimmed);
                if candidate.is_dir() {
                    return Ok(Some(candidate));
                }
            }
        }

        // Deteksi nama folder standar di direktori pengguna
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for folder_name in &[
                title,
                title.split('—').next().unwrap_or("").trim(),
                title.split('-').next().unwrap_or("").trim(),
            ] {
                if folder_name.is_empty() {
                    continue;
                }
                let candidate = home.join(folder_name);
                if candidate.is_dir() {
                    return Ok(Some(candidate));
                }
            }
        }

        Ok(None)
    }

    fn selected_items(&self) -> Result<Vec<PathBuf>> {
        let (conn, screen_num) = self.connect_x11()?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        // Pertahanan mendalam data safety: verifikasi jendela fokus masih file manager
        let win = self.focused_window()?;
        if !is_file_manager_process(&win.process_name) {
            tracing::info!(
                proc = win.process_name,
                "Jendela aktif bukan file manager; seleksi dibatalkan demi keamanan data"
            );
            return Ok(Vec::new());
        }

        // 1. Simulasikan Ctrl+C via XTest fake_input untuk mengisi clipboard dari file manager.
        // Tandai event sintetis agar hook X11 tidak mencegat atau menelannya.
        crate::linux::IS_SYNTHETIC_LINUX_EVENT.store(true, Ordering::SeqCst);

        conn.xtest_fake_input(
            x11rb::protocol::xproto::KEY_PRESS_EVENT,
            X11_KEYCODE_CTRL_L,
            0,
            0,
            0,
            0,
            0,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal simulasi Ctrl press: {e}")))?;

        conn.xtest_fake_input(
            x11rb::protocol::xproto::KEY_PRESS_EVENT,
            X11_KEYCODE_C,
            0,
            0,
            0,
            0,
            0,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal simulasi C press: {e}")))?;

        conn.xtest_fake_input(
            x11rb::protocol::xproto::KEY_RELEASE_EVENT,
            X11_KEYCODE_C,
            0,
            0,
            0,
            0,
            0,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal simulasi C release: {e}")))?;

        conn.xtest_fake_input(
            x11rb::protocol::xproto::KEY_RELEASE_EVENT,
            X11_KEYCODE_CTRL_L,
            0,
            0,
            0,
            0,
            0,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal simulasi Ctrl release: {e}")))?;

        conn.flush()
            .map_err(|e| PlatformError::Os(format!("Gagal flush simulasi keyboard: {e}")))?;

        // Jeda waktu singkat untuk memastikan file manager memperbarui selection X11
        thread::sleep(Duration::from_millis(80));
        crate::linux::IS_SYNTHETIC_LINUX_EVENT.store(false, Ordering::SeqCst);

        // 2. Baca data selection URI list
        let mut items = self.read_clipboard_uri_list(&conn, root)?;

        // Data Safety Guard Mutlak:
        // Validasi bahwa item yang dibaca berasal dari folder aktif saat ini.
        // Jika current_folder TIDAK dapat dipastikan (None/Err), KOSONGKAN seleksi demi mencegah
        // eksekusi data clipboard lama dari folder lain (stale clipboard hazard).
        match self.current_folder() {
            Ok(Some(ref cur_dir)) => {
                items.retain(|item| item.parent() == Some(cur_dir.as_path()));
            }
            _ => {
                tracing::warn!(
                    "Folder aktif tidak dapat dipastikan; mengosongkan seleksi clipboard demi keselamatan data"
                );
                return Ok(Vec::new());
            }
        }

        Ok(items)
    }

    fn select_next(&self) -> Result<()> {
        let (conn, _) = self.connect_x11()?;

        // Tandai event sintetis agar hook X11 tidak menganggap DOWN ini sebagai input pengguna
        crate::linux::IS_SYNTHETIC_LINUX_EVENT.store(true, Ordering::SeqCst);

        conn.xtest_fake_input(
            x11rb::protocol::xproto::KEY_PRESS_EVENT,
            X11_KEYCODE_DOWN,
            0,
            0,
            0,
            0,
            0,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal simulasi Down press: {e}")))?;

        conn.xtest_fake_input(
            x11rb::protocol::xproto::KEY_RELEASE_EVENT,
            X11_KEYCODE_DOWN,
            0,
            0,
            0,
            0,
            0,
        )
        .map_err(|e| PlatformError::Os(format!("Gagal simulasi Down release: {e}")))?;

        conn.flush()
            .map_err(|e| PlatformError::Os(format!("Gagal flush fake_input: {e}")))?;

        thread::sleep(Duration::from_millis(25));
        crate::linux::IS_SYNTHETIC_LINUX_EVENT.store(false, Ordering::SeqCst);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_file_uri() {
        let uri = "file:///home/user/My%20Documents/photo.png";
        let path = decode_file_uri(uri).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/My Documents/photo.png"));

        let uri_localhost = "file://localhost/tmp/test.txt";
        let path_localhost = decode_file_uri(uri_localhost).unwrap();
        assert_eq!(path_localhost, PathBuf::from("/tmp/test.txt"));

        let raw_path = "/var/log/syslog";
        let path_raw = decode_file_uri(raw_path).unwrap();
        assert_eq!(path_raw, PathBuf::from("/var/log/syslog"));
    }

    #[test]
    fn test_is_file_manager_process() {
        assert!(is_file_manager_process("nautilus"));
        assert!(is_file_manager_process("org.gnome.Nautilus"));
        assert!(is_file_manager_process("dolphin"));
        assert!(is_file_manager_process("Thunar"));
        assert!(!is_file_manager_process("firefox"));
        assert!(!is_file_manager_process("alacritty"));
    }
}

//! Interaksi konteks macOS Finder via AppleScript & CoreGraphics (T3.3).
//!
//! Mengimplementasikan `FileManagerContext` untuk macOS Finder (`com.apple.finder`):
//! - `focused_window`: deteksi apakah aplikasi aktif adalah Finder.
//! - `current_folder`: folder dari front Finder window (atau Desktop).
//! - `selected_items`: daftar item yang sedang diseleksi di Finder.
//! - `select_next`: mengirimkan panah bawah via CGEvent dengan tag `KEYFLOW_MACOS_USER_DATA`.

use std::ffi::c_void;
use std::path::PathBuf;
use std::process::Command;

use crate::macos::hook::KEYFLOW_MACOS_USER_DATA;
use crate::{FileManagerContext, PlatformError, Result, WindowInfo};

type CGEventRef = *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(source: *const c_void, keycode: u16, keydown: bool)
        -> CGEventRef;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;
const MACOS_DOWN_ARROW_KEYCODE: u16 = 125;

/// Snapshot konteks Finder yang diambil secara atomik dalam satu eksekusi AppleScript.
#[derive(Debug, Clone, Default)]
pub struct FinderSnapshot {
    pub is_finder: bool,
    pub process_name: String,
    pub window_title: String,
    pub current_folder: Option<PathBuf>,
    pub has_selection: bool,
}

/// Implementasi `FileManagerContext` untuk macOS Finder.
#[derive(Debug, Default, Clone)]
pub struct MacosFinderContext;

impl MacosFinderContext {
    pub fn new() -> Self {
        Self
    }

    /// Menjalankan skrip AppleScript via `osascript` dan mengembalikan output stdout.
    fn run_osascript(&self, script: &str) -> Result<String> {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| PlatformError::Os(format!("Gagal mengeksekusi osascript: {e}")))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(PlatformError::Os(format!("AppleScript gagal: {err_msg}")));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Mengambil snapshot konteks Finder (fokus, judul window, folder aktif, status seleksi)
    /// dalam SATU kali pemanggilan AppleScript untuk menghemat CPU cache polling.
    pub fn snapshot(&self) -> Result<FinderSnapshot> {
        let script = r#"with timeout of 1 seconds
            tell application "System Events"
                set frontProc to name of first application process whose frontmost is true
            end tell
            if frontProc is not "Finder" then
                return "0" & tab & frontProc & tab & tab & tab
            end if
            tell application "Finder"
                set winTitle to "Finder"
                set curFolder to ""
                if (count of Finder windows) > 0 then
                    set winTitle to name of front Finder window
                    try
                        set curFolder to POSIX path of (target of front Finder window as alias)
                    end try
                else
                    try
                        set curFolder to POSIX path of (desktop as alias)
                        set winTitle to "Desktop"
                    end try
                end if
                set selCount to count of selection
                set hasSel to "0"
                if selCount > 0 then
                    set hasSel to "1"
                end if
                return "1" & tab & frontProc & tab & winTitle & tab & curFolder & tab & hasSel
            end tell
        end timeout"#;

        let output = self.run_osascript(script)?;
        let parts: Vec<&str> = output.split('\t').collect();
        if parts.len() < 5 {
            return Ok(FinderSnapshot::default());
        }

        let is_finder = parts[0] == "1";
        let process_name = parts[1].to_string();
        let window_title = parts[2].to_string();
        let current_folder = if parts[3].trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(parts[3].trim()))
        };
        let has_selection = parts[4].trim() == "1";

        Ok(FinderSnapshot {
            is_finder,
            process_name,
            window_title,
            current_folder,
            has_selection,
        })
    }
}

impl FileManagerContext for MacosFinderContext {
    fn focused_window(&self) -> Result<WindowInfo> {
        // Ambil nama proses aplikasi yang sedang fokus di depan dengan timeout aman
        let script = r#"with timeout of 2 seconds
            tell application "System Events" to get name of first application process whose frontmost is true
        end timeout"#;
        let proc_name = self.run_osascript(script).unwrap_or_default();

        let is_finder = proc_name.eq_ignore_ascii_case("finder");

        let title = if is_finder {
            let title_script = r#"with timeout of 2 seconds
                tell application "Finder" to if (count of Finder windows) > 0 then return name of front Finder window else return "Desktop"
            end timeout"#;
            self.run_osascript(title_script)
                .unwrap_or_else(|_| "Finder".to_string())
        } else {
            String::new()
        };

        Ok(WindowInfo {
            process_name: proc_name,
            title,
        })
    }

    fn current_folder(&self) -> Result<Option<PathBuf>> {
        let script = r#"with timeout of 2 seconds
            tell application "Finder" to if (count of Finder windows) > 0 then return POSIX path of (target of front Finder window as alias) else return POSIX path of (desktop as alias)
        end timeout"#;
        match self.run_osascript(script) {
            Ok(output) => {
                let trimmed = output.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(PathBuf::from(trimmed)))
                }
            }
            Err(_) => Ok(None),
        }
    }

    fn selected_items(&self) -> Result<Vec<PathBuf>> {
        // Verifikasi ketat: hanya ambil seleksi jika Finder benar-benar jendela terdepan (frontmost).
        // Mencegah data hazard di mana seleksi lama Finder di latar belakang tereksekusi saat pengguna
        // berada di aplikasi lain.
        let script = r#"with timeout of 2 seconds
            tell application "System Events"
                if not (frontmost of application process "Finder") then return ""
            end tell
            tell application "Finder"
                set sel to selection
                set outList to {}
                repeat with anItem in sel
                    set end of outList to POSIX path of (anItem as alias)
                end repeat
                set AppleScript's text item delimiters to linefeed
                return outList as text
            end tell
        end timeout"#;

        let output = self.run_osascript(script)?;
        let mut items = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                items.push(PathBuf::from(trimmed));
            }
        }

        Ok(items)
    }

    fn select_next(&self) -> Result<()> {
        // SAFETY: Membuat dan memancarkan CGEvent panah bawah dengan tag KEYFLOW_MACOS_USER_DATA
        // agar tidak ditelan oleh hook KeyFlow sendiri.
        unsafe {
            let event_down =
                CGEventCreateKeyboardEvent(std::ptr::null(), MACOS_DOWN_ARROW_KEYCODE, true);
            if !event_down.is_null() {
                CGEventSetIntegerValueField(
                    event_down,
                    K_CG_EVENT_SOURCE_USER_DATA,
                    KEYFLOW_MACOS_USER_DATA,
                );
                CGEventPost(K_CG_SESSION_EVENT_TAP, event_down);
                CFRelease(event_down);
            }

            let event_up =
                CGEventCreateKeyboardEvent(std::ptr::null(), MACOS_DOWN_ARROW_KEYCODE, false);
            if !event_up.is_null() {
                CGEventSetIntegerValueField(
                    event_up,
                    K_CG_EVENT_SOURCE_USER_DATA,
                    KEYFLOW_MACOS_USER_DATA,
                );
                CGEventPost(K_CG_SESSION_EVENT_TAP, event_up);
                CFRelease(event_up);
            }
        }

        Ok(())
    }
}

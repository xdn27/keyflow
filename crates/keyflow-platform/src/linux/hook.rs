//! Implementasi `KeyboardHook` untuk Linux berbasis X11 key grab & SYNC event replay (T4.1 & T4.3).
//!
//! Memenuhi spesifikasi:
//! - Menggunakan `xproto::grab_key` dengan mode `GrabMode::SYNC`.
//! - Fail-open mutlak via `Allow::REPLAY_KEYBOARD`: tombol yang tidak ditelan diteruskan otomatis ke window aktif.
//! - Non-blocking callback dengan `try_read()` pada handler.
//! - Deteksi sesi Wayland (T4.3) dan penanganan fail-open saat berjalan di bawah Wayland/XWayland.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{Builder, JoinHandle};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Allow, ConnectionExt, GrabMode, KeyButMask, ModMask};

use crate::linux::wayland::{is_wayland_session, wayland_warning_message};
use crate::{HookDecision, KeyEvent, KeyboardHook, PlatformError, Result};

type HookHandlerFn = Box<dyn Fn(KeyEvent) -> HookDecision + Send + Sync>;

/// Menerjemahkan X11 evdev keycode ke nama tombol standar KeyFlow.
pub fn x11_keycode_to_name(keycode: u8) -> Option<&'static str> {
    match keycode {
        10 => Some("1"),
        11 => Some("2"),
        12 => Some("3"),
        13 => Some("4"),
        14 => Some("5"),
        15 => Some("6"),
        16 => Some("7"),
        17 => Some("8"),
        18 => Some("9"),
        19 => Some("0"),
        24 => Some("Q"),
        25 => Some("W"),
        26 => Some("E"),
        27 => Some("R"),
        28 => Some("T"),
        29 => Some("Y"),
        30 => Some("U"),
        31 => Some("I"),
        32 => Some("O"),
        33 => Some("P"),
        38 => Some("A"),
        39 => Some("S"),
        40 => Some("D"),
        41 => Some("F"),
        42 => Some("G"),
        43 => Some("H"),
        44 => Some("J"),
        45 => Some("K"),
        46 => Some("L"),
        52 => Some("Z"),
        53 => Some("X"),
        54 => Some("C"),
        55 => Some("V"),
        56 => Some("B"),
        57 => Some("N"),
        58 => Some("M"),
        9 => Some("ESC"),
        22 => Some("BACKSPACE"),
        23 => Some("TAB"),
        36 => Some("ENTER"),
        65 => Some("SPACE"),
        111 => Some("UP"),
        113 => Some("LEFT"),
        114 => Some("RIGHT"),
        116 => Some("DOWN"),
        119 => Some("DELETE"),
        67 => Some("F1"),
        68 => Some("F2"),
        69 => Some("F3"),
        70 => Some("F4"),
        71 => Some("F5"),
        72 => Some("F6"),
        73 => Some("F7"),
        74 => Some("F8"),
        75 => Some("F9"),
        76 => Some("F10"),
        95 => Some("F11"),
        96 => Some("F12"),
        _ => None,
    }
}

/// Pengelola keyboard hook Linux berbasis X11 key grab & SYNC replay.
pub struct LinuxX11HookManager {
    is_running: Arc<AtomicBool>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    handler: Arc<RwLock<Option<HookHandlerFn>>>,
}

impl LinuxX11HookManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            thread_handle: Mutex::new(None),
            handler: Arc::new(RwLock::new(None)),
        }
    }
}

impl KeyboardHook for LinuxX11HookManager {
    fn start(&self, handler: HookHandlerFn) -> Result<()> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        // T4.3: Deteksi sesi Wayland dan catat peringatan protokol
        if is_wayland_session() {
            tracing::warn!("{}", wayland_warning_message());
        }

        if let Ok(mut lock) = self.handler.write() {
            *lock = Some(handler);
        }

        let is_running = self.is_running.clone();
        let handler_clone = self.handler.clone();
        let (init_tx, init_rx) = std::sync::mpsc::channel();

        let handle = Builder::new()
            .name("keyflow-linux-x11-hook".into())
            .spawn(move || {
                let (conn, screen_num) = match x11rb::connect(None) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = init_tx.send(Err(PlatformError::Os(format!(
                            "Gagal membuka koneksi X11 untuk hook: {e}"
                        ))));
                        return;
                    }
                };

                let root = conn.setup().roots[screen_num].root;

                // Daftarkan grab_key untuk tombol-tombol yang didukung KeyFlow
                let keycodes_to_grab: Vec<u8> = (9..=19)
                    .chain(22..=33)
                    .chain(36..=36)
                    .chain(38..=46)
                    .chain(52..=58)
                    .chain(65..=76)
                    .chain([95, 96, 111, 113, 114, 116, 119])
                    .collect();

                // Mask modifier standar: None, Ctrl, Alt, Shift, Ctrl+Shift
                let base_modifiers = [
                    ModMask::from(0u16),
                    ModMask::from(u16::from(KeyButMask::CONTROL)),
                    ModMask::from(u16::from(KeyButMask::MOD1)), // Alt
                    ModMask::from(u16::from(KeyButMask::SHIFT)),
                    ModMask::from(u16::from(KeyButMask::CONTROL) | u16::from(KeyButMask::SHIFT)),
                ];

                // Kombinasi dengan LockMask (CapsLock) dan Mod2Mask (NumLock) agar shortcut tetap aktif
                let lock_num_masks = [
                    0u16,
                    u16::from(KeyButMask::LOCK),
                    u16::from(KeyButMask::MOD2),
                    u16::from(KeyButMask::LOCK) | u16::from(KeyButMask::MOD2),
                ];

                for &kc in &keycodes_to_grab {
                    for &base_mod in &base_modifiers {
                        for &ln in &lock_num_masks {
                            let combined_mod = ModMask::from(u16::from(base_mod) | ln);
                            let _ = conn.grab_key(
                                false,
                                root,
                                combined_mod,
                                kc,
                                GrabMode::ASYNC,
                                GrabMode::SYNC,
                            );
                        }
                    }
                }

                if let Err(e) = conn.flush() {
                    let _ = init_tx.send(Err(PlatformError::Os(format!(
                        "Gagal flush X11 grab_key: {e}"
                    ))));
                    return;
                }

                is_running.store(true, Ordering::SeqCst);
                let _ = init_tx.send(Ok(()));

                // Loop pemrosesan event X11
                while is_running.load(Ordering::Relaxed) {
                    let event = match conn.poll_for_event() {
                        Ok(Some(ev)) => ev,
                        Ok(None) => {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }
                        Err(e) => {
                            tracing::error!("Galat saat membaca event X11: {e}");
                            break;
                        }
                    };

                    if let x11rb::protocol::Event::KeyPress(ev) = event {
                        // 1. Abaikan event buatan sendiri (sintetis dari select_next atau Ctrl+C)
                        if crate::linux::IS_SYNTHETIC_LINUX_EVENT.load(Ordering::SeqCst) {
                            let _ = conn.allow_events(Allow::REPLAY_KEYBOARD, ev.time);
                            let _ = conn.flush();
                            continue;
                        }

                        let result = std::panic::catch_unwind(|| {
                            let Some(key_name) = x11_keycode_to_name(ev.detail) else {
                                // Tombol tidak dikenal: fail-open replay
                                let _ = conn.allow_events(Allow::REPLAY_KEYBOARD, ev.time);
                                let _ = conn.flush();
                                return;
                            };

                            let state = u16::from(ev.state);
                            let ctrl = (state & u16::from(KeyButMask::CONTROL)) != 0;
                            let alt = (state & u16::from(KeyButMask::MOD1)) != 0;
                            let shift = (state & u16::from(KeyButMask::SHIFT)) != 0;
                            let meta = (state & u16::from(KeyButMask::MOD4)) != 0;

                            let mut combo = String::new();
                            if ctrl {
                                combo.push_str("Ctrl+");
                            }
                            if alt {
                                combo.push_str("Alt+");
                            }
                            if shift {
                                combo.push_str("Shift+");
                            }
                            if meta {
                                combo.push_str("Meta+");
                            }
                            combo.push_str(key_name);

                            let key_event = KeyEvent {
                                key: combo,
                                pressed: true,
                            };

                            // Evaluasi hook handler dengan non-blocking try_read
                            let decision = {
                                let lock = handler_clone.try_read().ok();
                                if let Some(Some(ref h)) = lock.as_deref() {
                                    h(key_event)
                                } else {
                                    HookDecision::PassThrough
                                }
                            };

                            if decision == HookDecision::Swallow {
                                // Menelan tombol: lanjutkan keyboard tanpa replay ke window fokus
                                let _ = conn.allow_events(Allow::ASYNC_KEYBOARD, ev.time);
                            } else {
                                // Fail-open: ulangi dan teruskan event tombol ke window fokus asli
                                let _ = conn.allow_events(Allow::REPLAY_KEYBOARD, ev.time);
                            }
                            let _ = conn.flush();
                        });

                        if result.is_err() {
                            // Panic safety: selalu replay tombol ke OS jika terjadi kesalahan
                            let _ = conn.allow_events(Allow::REPLAY_KEYBOARD, ev.time);
                            let _ = conn.flush();
                        }
                    }
                }

                // Cleanup: lepaskan seluruh key grab saat shutdown
                let _ = conn.ungrab_key(0, root, ModMask::ANY);
                let _ = conn.flush();
            })
            .map_err(|e| PlatformError::Os(format!("Gagal membuat thread hook X11: {e}")))?;

        if let Ok(mut lock) = self.thread_handle.lock() {
            *lock = Some(handle);
        }

        init_rx.recv().map_err(|e| {
            PlatformError::Os(format!("Gagal menerima status inisialisasi hook: {e}"))
        })?
    }

    fn stop(&self) {
        if !self.is_running.swap(false, Ordering::SeqCst) {
            return;
        }

        if let Ok(mut lock) = self.thread_handle.lock() {
            if let Some(h) = lock.take() {
                let _ = h.join();
            }
        }

        if let Ok(mut lock) = self.handler.write() {
            *lock = None;
        }
    }
}

impl Drop for LinuxX11HookManager {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for LinuxX11HookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x11_keycode_mapping() {
        assert_eq!(x11_keycode_to_name(10), Some("1"));
        assert_eq!(x11_keycode_to_name(19), Some("0"));
        assert_eq!(x11_keycode_to_name(52), Some("Z"));
        assert_eq!(x11_keycode_to_name(36), Some("ENTER"));
        assert_eq!(x11_keycode_to_name(116), Some("DOWN"));
        assert_eq!(x11_keycode_to_name(255), None);
    }
}

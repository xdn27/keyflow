//! Cache konteks jendela aktif dan folder untuk Linux X11 (T4.1).
//!
//! Menjalankan thread background yang memantau aplikasi depan (file manager)
//! dan memperbarui folder aktif secara berkala (200ms) agar callback
//! hook tetap instan (< 10µs) tanpa memblokir pemrosesan event X11.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, Builder, JoinHandle};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;

use crate::linux::context::{is_file_manager_process, LinuxFileManagerContext};
use crate::FileManagerContext;

/// Snapshot konteks jendela yang disimpan di cache untuk Linux.
#[derive(Debug, Clone, Default)]
pub struct CachedContext {
    pub is_file_manager: bool,
    pub process_name: String,
    pub window_title: String,
    pub current_folder: Option<PathBuf>,
    pub has_selection: bool,
    pub last_updated: Option<Instant>,
}

/// Pengelola thread polling dan cache konteks Linux file manager.
pub struct LinuxContextCache {
    cached: Arc<RwLock<CachedContext>>,
    is_running: Arc<AtomicBool>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

impl LinuxContextCache {
    /// Membuat LinuxContextCache baru dan memulai thread polling background.
    pub fn new(context: Arc<LinuxFileManagerContext>) -> Self {
        let cached = Arc::new(RwLock::new(CachedContext::default()));
        let is_running = Arc::new(AtomicBool::new(true));

        let cached_clone = cached.clone();
        let running_clone = is_running.clone();

        let handle = Builder::new()
            .name("keyflow-linux-context-cache".into())
            .spawn(move || {
                let mut conn_opt = x11rb::connect(None).ok();

                while running_clone.load(Ordering::Relaxed) {
                    if conn_opt.is_none() {
                        conn_opt = x11rb::connect(None).ok();
                    }

                    if let Some((ref conn, screen_num)) = conn_opt {
                        let root = conn.setup().roots[screen_num].root;
                        let net_active_window = conn
                            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
                            .ok()
                            .and_then(|c| c.reply().ok())
                            .map(|r| r.atom)
                            .unwrap_or(0);
                        let utf8_atom = conn
                            .intern_atom(false, b"UTF8_STRING")
                            .ok()
                            .and_then(|c| c.reply().ok())
                            .map(|r| r.atom)
                            .unwrap_or(0);
                        let net_wm_name = conn
                            .intern_atom(false, b"_NET_WM_NAME")
                            .ok()
                            .and_then(|c| c.reply().ok())
                            .map(|r| r.atom)
                            .unwrap_or(0);

                        let win = context.read_focused_window_fast(
                            conn,
                            root,
                            net_active_window,
                            net_wm_name,
                            utf8_atom,
                        );

                        let is_fm = is_file_manager_process(&win.process_name);

                        if is_fm {
                            let current_folder = context.current_folder().unwrap_or(None);
                            if let Ok(mut lock) = cached_clone.write() {
                                lock.is_file_manager = true;
                                lock.process_name = win.process_name;
                                lock.window_title = win.title;
                                lock.current_folder = current_folder;
                                lock.has_selection = true;
                                lock.last_updated = Some(Instant::now());
                            }
                        } else if let Ok(mut lock) = cached_clone.write() {
                            lock.is_file_manager = false;
                            lock.process_name = win.process_name;
                            lock.window_title = win.title;
                            lock.current_folder = None;
                            lock.has_selection = false;
                            lock.last_updated = Some(Instant::now());
                        }
                    }

                    thread::sleep(Duration::from_millis(150));
                }
            })
            .ok();

        Self {
            cached,
            is_running,
            thread_handle: Mutex::new(handle),
        }
    }

    /// Mengambil salinan snapshot konteks saat ini secara instan (sub-mikrodetik).
    pub fn get(&self) -> CachedContext {
        self.cached.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// Menghentikan thread polling background.
    pub fn stop(&self) {
        if !self.is_running.swap(false, Ordering::SeqCst) {
            return;
        }

        if let Ok(mut lock) = self.thread_handle.lock() {
            if let Some(h) = lock.take() {
                let _ = h.join();
            }
        }
    }
}

impl Drop for LinuxContextCache {
    fn drop(&mut self) {
        self.stop();
    }
}

//! Cache konteks jendela aktif dan folder Explorer (T2.2).
//!
//! Menjalankan thread background yang memantau perubahan jendela aktif
//! dan memperbarui folder aktif dari Explorer secara berkala (75ms).
//! Callback hook keyboard hanya membaca cache ini dalam hitungan mikrodetik
//! tanpa I/O atau COM, memastikan performa instan dan fail-open jika jendela
//! bukan Explorer.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::windows::hook::is_explorer_window;
use crate::windows::shell::WindowsShellClient;

/// Snapshot konteks jendela yang disimpan di cache.
#[derive(Debug, Clone, Default)]
pub struct CachedContext {
    pub is_explorer: bool,
    pub hwnd: Option<isize>,
    pub process_name: String,
    pub window_title: String,
    pub current_folder: Option<PathBuf>,
    pub has_selection: bool,
    pub last_updated: Option<Instant>,
}

/// Pengelola thread polling dan cache konteks Explorer.
pub struct ContextCache {
    cached: Arc<RwLock<CachedContext>>,
    is_running: Arc<AtomicBool>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ContextCache {
    /// Membuat ContextCache baru dan memulai thread polling background.
    pub fn new(shell_client: Arc<WindowsShellClient>) -> Self {
        let cached = Arc::new(RwLock::new(CachedContext::default()));
        let is_running = Arc::new(AtomicBool::new(true));

        let cached_clone = cached.clone();
        let running_clone = is_running.clone();

        let handle = Builder::new()
            .name("keyflow-context-cache".into())
            .spawn(move || {
                let mut last_hwnd = HWND(std::ptr::null_mut());

                while running_clone.load(Ordering::Relaxed) {
                    // SAFETY: GetForegroundWindow memeriksa handle jendela yang sedang aktif
                    let hwnd = unsafe { GetForegroundWindow() };

                    let is_explorer = is_explorer_window(hwnd);

                    if is_explorer {
                        // Jika Explorer aktif, query detail folder aktif via COM Shell Client
                        let shell_ctx = shell_client.get_active_context().unwrap_or_default();
                        if let Ok(mut lock) = cached_clone.write() {
                            lock.is_explorer = true;
                            lock.hwnd = Some(hwnd.0 as isize);
                            lock.process_name = "explorer.exe".to_string();
                            lock.window_title = shell_ctx.window_title;
                            lock.current_folder = shell_ctx.active_folder;
                            lock.has_selection = !shell_ctx.selected_items.is_empty();
                            lock.last_updated = Some(Instant::now());
                        }
                    } else if hwnd != last_hwnd {
                        // Jendela aktif berganti dan bukan Explorer
                        if let Ok(mut lock) = cached_clone.write() {
                            lock.is_explorer = false;
                            lock.hwnd = None;
                            lock.process_name = String::new();
                            lock.window_title = String::new();
                            lock.current_folder = None;
                            lock.has_selection = false;
                            lock.last_updated = Some(Instant::now());
                        }
                    }

                    last_hwnd = hwnd;
                    std::thread::sleep(Duration::from_millis(75));
                }
            })
            .ok();

        Self {
            cached,
            is_running,
            thread_handle: Mutex::new(handle),
        }
    }

    /// Mengambil salinan snapshot konteks saat ini (sangat cepat, sub-mikrodetik).
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

impl Drop for ContextCache {
    fn drop(&mut self) {
        self.stop();
    }
}

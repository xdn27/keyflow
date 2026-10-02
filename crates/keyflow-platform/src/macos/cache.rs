//! Cache konteks jendela aktif dan folder Finder untuk macOS (T3.3).
//!
//! Menjalankan thread background yang memantau aplikasi depan (Finder)
//! dan memperbarui folder aktif secara berkala (150ms) agar callback
//! hook tetap instan (< 10µs) tanpa memblokir thread event tap.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

use crate::macos::finder::MacosFinderContext;

/// Snapshot konteks jendela yang disimpan di cache untuk macOS.
#[derive(Debug, Clone, Default)]
pub struct CachedContext {
    pub is_finder: bool,
    pub process_name: String,
    pub window_title: String,
    pub current_folder: Option<PathBuf>,
    pub has_selection: bool,
    pub last_updated: Option<Instant>,
}

/// Pengelola thread polling dan cache konteks Finder.
pub struct ContextCache {
    cached: Arc<RwLock<CachedContext>>,
    is_running: Arc<AtomicBool>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ContextCache {
    /// Membuat ContextCache baru dan memulai thread polling background.
    pub fn new(finder_ctx: Arc<MacosFinderContext>) -> Self {
        let cached = Arc::new(RwLock::new(CachedContext::default()));
        let is_running = Arc::new(AtomicBool::new(true));

        let cached_clone = cached.clone();
        let running_clone = is_running.clone();

        let handle = Builder::new()
            .name("keyflow-macos-context-cache".into())
            .spawn(move || {
                while running_clone.load(Ordering::Relaxed) {
                    if let Ok(snap) = finder_ctx.snapshot() {
                        if let Ok(mut lock) = cached_clone.write() {
                            lock.is_finder = snap.is_finder;
                            lock.process_name = snap.process_name;
                            lock.window_title = snap.window_title;
                            lock.current_folder = snap.current_folder;
                            lock.has_selection = snap.has_selection;
                            lock.last_updated = Some(Instant::now());
                        }
                    }

                    std::thread::sleep(Duration::from_millis(200));
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

impl Drop for ContextCache {
    fn drop(&mut self) {
        self.stop();
    }
}

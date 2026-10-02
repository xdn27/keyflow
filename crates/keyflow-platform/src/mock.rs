//! Mock implementasi trait platform untuk pengujian end-to-end tanpa OS (T1.8).
//!
//! Lihat `docs/agents/testing.md`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use crate::{FileManagerContext, HookDecision, KeyEvent, KeyboardHook, Result, WindowInfo};

type HandlerFn = Box<dyn Fn(KeyEvent) -> HookDecision + Send + Sync>;

/// Mock untuk KeyboardHook.
pub struct MockKeyboardHook {
    handler: Arc<RwLock<Option<HandlerFn>>>,
    dispatched_events: Arc<RwLock<Vec<(KeyEvent, HookDecision)>>>,
}

impl Default for MockKeyboardHook {
    fn default() -> Self {
        Self::new()
    }
}

impl MockKeyboardHook {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(RwLock::new(None)),
            dispatched_events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Mensimulasikan satu penekanan tombol dan mengembalikan keputusan hook.
    pub fn simulate_key(&self, event: KeyEvent) -> HookDecision {
        let guard = self
            .handler
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let decision = if let Some(ref h) = *guard {
            h(event.clone())
        } else {
            HookDecision::PassThrough
        };

        let mut hist = self
            .dispatched_events
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        hist.push((event, decision));

        decision
    }

    /// Mengambil riwayat event tombol dan keputusan yang dihasilkan.
    pub fn history(&self) -> Vec<(KeyEvent, HookDecision)> {
        self.dispatched_events
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl KeyboardHook for MockKeyboardHook {
    fn start(&self, handler: HandlerFn) -> Result<()> {
        let mut guard = self
            .handler
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(handler);
        Ok(())
    }

    fn stop(&self) {
        let mut guard = self
            .handler
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }
}

/// Mock untuk FileManagerContext.
pub struct MockFileManagerContext {
    window: RwLock<WindowInfo>,
    folder: RwLock<Option<PathBuf>>,
    items: RwLock<Vec<PathBuf>>,
    select_next_calls: AtomicUsize,
}

impl Default for MockFileManagerContext {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFileManagerContext {
    pub fn new() -> Self {
        Self {
            window: RwLock::new(WindowInfo {
                process_name: "explorer.exe".to_string(),
                title: "File Explorer".to_string(),
            }),
            folder: RwLock::new(None),
            items: RwLock::new(Vec::new()),
            select_next_calls: AtomicUsize::new(0),
        }
    }

    pub fn set_focused_window(&self, process_name: &str, title: &str) {
        let mut guard = self
            .window
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.process_name = process_name.to_string();
        guard.title = title.to_string();
    }

    pub fn set_current_folder(&self, folder: Option<PathBuf>) {
        let mut guard = self
            .folder
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = folder;
    }

    pub fn set_selected_items(&self, items: Vec<PathBuf>) {
        let mut guard = self
            .items
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = items;
    }

    pub fn select_next_count(&self) -> usize {
        self.select_next_calls.load(Ordering::SeqCst)
    }
}

impl FileManagerContext for MockFileManagerContext {
    fn focused_window(&self) -> Result<WindowInfo> {
        let guard = self
            .window
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(guard.clone())
    }

    fn current_folder(&self) -> Result<Option<PathBuf>> {
        let guard = self
            .folder
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(guard.clone())
    }

    fn selected_items(&self) -> Result<Vec<PathBuf>> {
        let guard = self
            .items
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(guard.clone())
    }

    fn select_next(&self) -> Result<()> {
        self.select_next_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

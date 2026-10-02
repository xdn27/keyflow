//! System Tray icon dan menu kontekstual Windows (T2.4).

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::worker::WorkerTask;

/// Aksi yang dihasilkan dari event menu System Tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    None,
    ReloadConfig,
    Quit,
}

/// Pengelola icon dan menu System Tray.
pub struct TrayManager {
    _tray_icon: TrayIcon,
    is_paused_item: CheckMenuItem,
    undo_item: MenuItem,
    reload_item: MenuItem,
    settings_item: MenuItem,
    open_config_item: MenuItem,
    open_log_item: MenuItem,
    quit_item: MenuItem,
    config_dir: PathBuf,
    log_path: PathBuf,
    worker_tx: Sender<WorkerTask>,
    is_enabled: Arc<AtomicBool>,
    /// Proses jendela pengaturan yang sedang berjalan, agar tidak dibuka ganda.
    settings_child: Mutex<Option<Child>>,
}

impl TrayManager {
    /// Inisialisasi System Tray icon dan menu.
    pub fn new(
        config_dir: PathBuf,
        log_path: PathBuf,
        worker_tx: Sender<WorkerTask>,
        is_enabled: Arc<AtomicBool>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();

        let is_paused_item = CheckMenuItem::new("Jeda KeyFlow", true, false, None);
        let undo_item = MenuItem::new("Undo Terakhir (Ctrl+Shift+Z)", true, None);
        let separator1 = PredefinedMenuItem::separator();
        let reload_item = MenuItem::new("Reload Konfigurasi", true, None);
        let settings_item = MenuItem::new("Pengaturan...", true, None);
        let open_config_item = MenuItem::new("Buka Folder Konfigurasi", true, None);
        let open_log_item = MenuItem::new("Buka File Log Undo", true, None);
        let separator2 = PredefinedMenuItem::separator();
        let quit_item = MenuItem::new("Keluar dari KeyFlow", true, None);

        menu.append(&is_paused_item)?;
        menu.append(&undo_item)?;
        menu.append(&separator1)?;
        menu.append(&settings_item)?;
        menu.append(&reload_item)?;
        menu.append(&open_config_item)?;
        menu.append(&open_log_item)?;
        menu.append(&separator2)?;
        menu.append(&quit_item)?;

        let icon = create_app_icon()?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("KeyFlow - Shortcut File Manager")
            .with_icon(icon)
            .build()?;

        Ok(Self {
            _tray_icon: tray_icon,
            is_paused_item,
            undo_item,
            reload_item,
            settings_item,
            open_config_item,
            open_log_item,
            quit_item,
            config_dir,
            log_path,
            worker_tx,
            is_enabled,
            settings_child: Mutex::new(None),
        })
    }

    /// Memproses event klik menu tray jika ada.
    pub fn handle_menu_events(&self) -> TrayAction {
        let mut result_action = TrayAction::None;

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.is_paused_item.id() {
                let is_checked = self.is_paused_item.is_checked();
                // Jika "Jeda" dicentang -> enabled = false
                let new_enabled = !is_checked;
                self.is_enabled.store(new_enabled, Ordering::SeqCst);
                let _ = self.worker_tx.send(WorkerTask::TogglePause(is_checked));
                tracing::info!(
                    enabled = new_enabled,
                    "Status global KeyFlow diubah via tray"
                );
            } else if event.id == self.undo_item.id() {
                let _ = self.worker_tx.send(WorkerTask::ExecuteUndo);
            } else if event.id == self.reload_item.id() {
                result_action = TrayAction::ReloadConfig;
            } else if event.id == self.settings_item.id() {
                self.open_settings_window();
            } else if event.id == self.open_config_item.id() {
                open_in_file_manager(&self.config_dir);
            } else if event.id == self.open_log_item.id() {
                open_file_with_editor(&self.log_path);
            } else if event.id == self.quit_item.id() {
                tracing::info!("Pengguna memilih 'Keluar' dari system tray");
                return TrayAction::Quit;
            }
        }

        result_action
    }

    /// Menjalankan `keyflow settings` sebagai proses terpisah (GUI tidak berbagi
    /// event loop dengan tray/hook). Bila jendela masih terbuka, tidak membuka lagi.
    fn open_settings_window(&self) {
        let Ok(mut child_slot) = self.settings_child.lock() else {
            return;
        };
        if let Some(child) = child_slot.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                tracing::info!("Jendela pengaturan sudah terbuka");
                return;
            }
        }
        *child_slot = None;

        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                tracing::warn!(error = %e, "Tidak dapat menentukan lokasi keyflow.exe");
                return;
            }
        };
        match std::process::Command::new(exe).arg("settings").spawn() {
            Ok(child) => *child_slot = Some(child),
            Err(e) => tracing::warn!(error = %e, "Gagal membuka jendela pengaturan"),
        }
    }
}

/// Menghasilkan ikon RGBA 32x32 dalam memori untuk icon tray.
fn create_app_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            // Bingkai biru gelap dengan aksen cyan di tengah
            let is_border = x == 0 || x == width - 1 || y == 0 || y == height - 1;
            let is_center = (8..24).contains(&x) && (8..24).contains(&y);

            if is_border {
                rgba.extend_from_slice(&[30, 41, 59, 255]); // Slate 800
            } else if is_center {
                rgba.extend_from_slice(&[14, 165, 233, 255]); // Sky 500 (Cyan)
            } else {
                rgba.extend_from_slice(&[15, 23, 42, 255]); // Slate 900
            }
        }
    }

    Icon::from_rgba(rgba, width, height).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

/// Membuka folder di Windows Explorer.
fn open_in_file_manager(path: &Path) {
    let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
}

/// Membuka file di Notepad Windows.
fn open_file_with_editor(path: &Path) {
    let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
}

//! Wiring aplikasi KeyFlow untuk Windows (T2.1–T2.8).
//!
//! Mengoordinasikan:
//! - Direktori konfigurasi (%APPDATA%/KeyFlow) & log undo (%LOCALAPPDATA%/KeyFlow)
//! - ConfigManager dengan hot-reload otomatis
//! - Keyboard Hook (WH_KEYBOARD_LL) dengan evaluasi kilat via ContextCache
//! - Worker thread asinkron untuk eksekusi aksi file aman & undo
//! - System Tray icon & menu (Jeda, Undo, Reload, Buka Folder, Keluar)
//! - Windows Toast Notification

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossbeam_channel::unbounded;
use directories::ProjectDirs;
use keyflow_core::config::{ConfigManager, KeyCombo};
use keyflow_core::matcher::{ContextMatcher, RuntimeContext};
use keyflow_core::undo::UndoManager;
use keyflow_platform::windows::{
    get_foreground_window_hwnd, process_pending_windows_messages, show_notification, ContextCache,
    WindowsFileManagerContext, WindowsHookManager, WindowsShellClient,
};
use keyflow_platform::{HookDecision, KeyboardHook};

use crate::tray::{TrayAction, TrayManager};
use crate::worker::{run_worker_loop, WorkerTask};

/// Titik masuk aplikasi Windows KeyFlow.
pub fn run() -> anyhow::Result<()> {
    tracing::info!("Memulai KeyFlow untuk Windows...");

    // 1. Direktori konfigurasi dan data
    let (config_dir, data_dir) = ensure_app_directories()?;
    let config_path = config_dir.join("config.yaml");
    ensure_default_config(&config_path)?;
    let undo_log_path = data_dir.join("undo_log.jsonl");

    tracing::info!(
        config_path = %config_path.display(),
        undo_log_path = %undo_log_path.display(),
        "Direktori aplikasi terverifikasi"
    );

    // 2. Inisialisasi ConfigManager dan Matcher
    let mut config_manager = ConfigManager::new(config_path.clone())
        .map_err(|e| anyhow::anyhow!("Gagal memuat konfigurasi: {e}"))?;
    let initial_config = config_manager.get_config();

    let matcher = Arc::new(RwLock::new(ContextMatcher::compile(&initial_config)));

    // 3. Pasang hot-reload konfigurasi otomatis
    let matcher_reload = matcher.clone();
    config_manager
        .start_hot_reload(move |res| match res {
            Ok(new_cfg) => {
                if let Ok(mut lock) = matcher_reload.write() {
                    *lock = ContextMatcher::compile(&new_cfg);
                }
                tracing::info!("Konfigurasi berhasil dimuat ulang secara otomatis");
                show_notification("KeyFlow", "Konfigurasi berhasil diperbarui");
            }
            Err(err) => {
                tracing::error!("Konfigurasi baru tidak valid: {err}");
                show_notification(
                    "KeyFlow Galat Konfigurasi",
                    &format!("Konfigurasi gagal diperbarui: {err}"),
                );
            }
        })
        .map_err(|e| anyhow::anyhow!("Gagal memulai watcher konfigurasi: {e}"))?;

    // 4. Inisialisasi UndoManager dengan log persisten
    let undo_manager = UndoManager::with_log_file(
        undo_log_path.clone(),
        initial_config.settings.undo_history_limit,
    )
    .map_err(|e| anyhow::anyhow!("Gagal membuka file log undo: {e}"))?;

    // 5. Inisialisasi komponen Windows Platform (Shell, Cache, Context berbagi satu STA client)
    let shell_client = Arc::new(
        WindowsShellClient::new()
            .map_err(|e| anyhow::anyhow!("Gagal inisialisasi COM Shell Client: {e}"))?,
    );
    let context_cache = Arc::new(ContextCache::new(shell_client.clone()));
    let fm_context = Arc::new(WindowsFileManagerContext::with_shell_client(shell_client));

    // 6. Inisialisasi worker thread untuk eksekusi aksi file asinkron
    let (worker_tx, worker_rx) = unbounded::<WorkerTask>();
    let worker_fm = fm_context.clone();
    let worker_handle = std::thread::Builder::new()
        .name("keyflow-worker".into())
        .spawn(move || {
            run_worker_loop(
                worker_rx,
                worker_fm,
                undo_manager,
                Box::new(|title, msg| {
                    show_notification(title, msg);
                }),
            );
        })
        .map_err(|e| anyhow::anyhow!("Gagal memulai worker thread: {e}"))?;

    // 7. Status aktif global (dapat dijeda lewat menu tray)
    let is_enabled = Arc::new(AtomicBool::new(true));

    // 8. Pasang low-level keyboard hook
    let hook_manager = WindowsHookManager::new();
    let cache_hook = context_cache.clone();
    let matcher_hook = matcher.clone();
    let tx_hook = worker_tx.clone();
    let enabled_hook = is_enabled.clone();

    hook_manager
        .start(Box::new(move |event| {
            // Jika aplikasi sedang dijeda, jangan menelan tombol apapun
            if !enabled_hook.load(Ordering::Relaxed) {
                return HookDecision::PassThrough;
            }

            let Ok(key_combo) = KeyCombo::parse(&event.key) else {
                return HookDecision::PassThrough;
            };

            // Baca snapshot konteks dari cache (< 10µs, tanpa I/O atau COM)
            let cached = cache_hook.get();
            if !cached.is_explorer {
                return HookDecision::PassThrough;
            }

            // Verifikasi cepat (< 50ns) bahwa jendela aktif saat tombol ditekan
            // masih merupakan jendela Explorer yang terverifikasi (mencegah salah telan saat beralih window).
            let current_fg = get_foreground_window_hwnd();
            if Some(current_fg) != cached.hwnd {
                return HookDecision::PassThrough;
            }

            // Dummy slice untuk penanda ada/tidaknya seleksi tanpa alokasi heap besar
            let dummy_selection = [PathBuf::from("selected")];
            let selected_items = if cached.has_selection {
                &dummy_selection[..]
            } else {
                &[][..]
            };

            let rt_ctx = RuntimeContext {
                process_name: &cached.process_name,
                current_folder: cached.current_folder.as_deref(),
                selected_items,
            };

            // Cocokkan rule
            let matched = {
                let lock = matcher_hook.read().ok();
                lock.and_then(|m| m.match_rule(&key_combo, &rt_ctx))
            };

            if let Some(rule) = matched {
                let _ = tx_hook.send(WorkerTask::ExecuteRule { rule });
                HookDecision::Swallow
            } else {
                HookDecision::PassThrough
            }
        }))
        .map_err(|e| anyhow::anyhow!("Gagal memasang keyboard hook: {e}"))?;

    // 9. Inisialisasi System Tray Icon dan Menu
    let tray_manager = TrayManager::new(
        config_dir.clone(),
        undo_log_path.clone(),
        worker_tx.clone(),
        is_enabled.clone(),
    )
    .map_err(|e| anyhow::anyhow!("Gagal menginisialisasi System Tray: {e}"))?;

    // Tampilkan notifikasi pembuka bahwa KeyFlow siap
    show_notification(
        "KeyFlow Siap",
        "KeyFlow aktif di latar belakang (System Tray).",
    );

    tracing::info!("KeyFlow Windows berjalan. Loop utama aktif.");

    // 10. Loop pesan Windows dan event menu System Tray
    while process_pending_windows_messages() {
        match tray_manager.handle_menu_events() {
            TrayAction::Quit => break,
            TrayAction::ReloadConfig => match config_manager.reload() {
                Ok(new_cfg) => {
                    if let Ok(mut lock) = matcher.write() {
                        *lock = ContextMatcher::compile(&new_cfg);
                    }
                    tracing::info!("Konfigurasi berhasil dimuat ulang via menu tray");
                    show_notification("KeyFlow", "Konfigurasi berhasil diperbarui");
                }
                Err(err) => {
                    tracing::error!("Gagal memuat ulang konfigurasi via menu tray: {err}");
                    show_notification(
                        "KeyFlow Galat Konfigurasi",
                        &format!("Gagal memuat konfigurasi: {err}"),
                    );
                }
            },
            TrayAction::None => {}
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // 11. Graceful shutdown bersih tanpa deadlock
    tracing::info!("Memulai proses shutdown KeyFlow...");
    hook_manager.stop();
    context_cache.stop();
    config_manager.stop_hot_reload();
    let _ = worker_tx.send(WorkerTask::Shutdown);
    drop(tray_manager);
    drop(worker_tx);
    let _ = worker_handle.join();

    tracing::info!("KeyFlow berhasil dimatikan secara bersih.");
    Ok(())
}

/// Menyiapkan direktori konfigurasi dan direktori data lokal.
fn ensure_app_directories() -> anyhow::Result<(PathBuf, PathBuf)> {
    if let Some(proj) = ProjectDirs::from("com", "KeyFlow", "KeyFlow") {
        let config_dir = proj.config_dir().to_path_buf();
        let data_dir = proj.data_local_dir().to_path_buf();

        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&data_dir)?;

        Ok((config_dir, data_dir))
    } else {
        // Fallback untuk environment tanpa ProjectDirs
        let base_dir = std::env::current_dir()?.join(".keyflow");
        fs::create_dir_all(&base_dir)?;
        Ok((base_dir.clone(), base_dir))
    }
}

/// Menulis template konfigurasi bawaan jika file belum ada di disk.
fn ensure_default_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let default_content = r#"# KeyFlow - Context-Aware File Manager Shortcuts
# Dokumentasi dan panduan: https://github.com/your-repo/keyflow

version: 1

settings:
  dry_run: false
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 100

profiles:
  - name: "Default Windows Explorer"
    enabled: true
    context:
      app: "explorer.exe"
    rules:
      # Pintasan Undo: Memulihkan aksi file terakhir yang dilakukan KeyFlow
      - key: "Ctrl+Shift+Z"
        action: undo
"#;

    fs::write(path, default_content)?;
    tracing::info!(path = %path.display(), "Template konfigurasi bawaan berhasil dibuat");
    Ok(())
}

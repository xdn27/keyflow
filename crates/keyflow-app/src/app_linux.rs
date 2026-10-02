//! Wiring aplikasi KeyFlow untuk Linux X11 (T4.1–T4.4).
//!
//! Mengoordinasikan:
//! - Direktori konfigurasi (~/.config/keyflow) & log undo persisten (~/.local/share/keyflow)
//! - Deteksi sesi Wayland (T4.3) dengan pesan batasan protokol & fail-open
//! - ConfigManager dengan hot-reload otomatis
//! - Keyboard Hook (X11 sync grab) dengan evaluasi kilat via LinuxContextCache
//! - Worker thread asinkron untuk eksekusi aksi file aman & undo
//! - Linux native notifications (notify-send)
//! - Penanganan sinyal terminasi bersih (ctrlc)

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
use keyflow_platform::linux::{
    is_wayland_session, show_notification, wayland_warning_message, LinuxContextCache,
    LinuxFileManagerContext, LinuxX11HookManager,
};
use keyflow_platform::{HookDecision, KeyboardHook};

use crate::worker::{run_worker_loop, WorkerTask};

/// Titik masuk aplikasi Linux KeyFlow.
pub fn run() -> anyhow::Result<()> {
    tracing::info!("Memulai KeyFlow untuk Linux...");

    // 1. Deteksi sesi Wayland (T4.3)
    if is_wayland_session() {
        tracing::warn!("{}", wayland_warning_message());
        show_notification("KeyFlow [Wayland]", wayland_warning_message());
    }

    // 2. Direktori konfigurasi dan data
    let (config_dir, data_dir) = ensure_app_directories()?;
    let config_path = config_dir.join("config.yaml");
    ensure_default_config(&config_path)?;
    let undo_log_path = data_dir.join("undo_log.jsonl");

    tracing::info!(
        config_path = %config_path.display(),
        undo_log_path = %undo_log_path.display(),
        "Direktori aplikasi Linux terverifikasi"
    );

    // 3. Inisialisasi ConfigManager dan Matcher
    let mut config_manager = ConfigManager::new(config_path.clone())
        .map_err(|e| anyhow::anyhow!("Gagal memuat konfigurasi: {e}"))?;
    let initial_config = config_manager.get_config();

    let matcher = Arc::new(RwLock::new(ContextMatcher::compile(&initial_config)));

    // 4. Pasang hot-reload konfigurasi otomatis
    let matcher_reload = matcher.clone();
    config_manager
        .start_hot_reload(move |res| match res {
            Ok(new_cfg) => {
                if let Ok(mut lock) = matcher_reload.write() {
                    *lock = ContextMatcher::compile(&new_cfg);
                }
                tracing::info!("Konfigurasi Linux berhasil dimuat ulang secara otomatis");
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

    // 5. Inisialisasi UndoManager dengan log persisten
    let undo_manager = UndoManager::with_log_file(
        undo_log_path.clone(),
        initial_config.settings.undo_history_limit,
    )
    .map_err(|e| anyhow::anyhow!("Gagal membuka file log undo: {e}"))?;

    // 6. Inisialisasi komponen Linux Platform (File Manager Context & Cache)
    let linux_context = Arc::new(LinuxFileManagerContext::new());
    let context_cache = Arc::new(LinuxContextCache::new(linux_context.clone()));

    // 7. Inisialisasi worker thread untuk eksekusi aksi file asinkron
    let (worker_tx, worker_rx) = unbounded::<WorkerTask>();
    let worker_fm = linux_context.clone();
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

    let is_enabled = Arc::new(AtomicBool::new(true));

    // 8. Pasang low-level X11 sync key grab hook (T4.1) hanya jika bukan sesi Wayland (T4.3)
    let hook_manager = LinuxX11HookManager::new();
    if is_wayland_session() {
        tracing::info!("Sesi Wayland aktif: keyboard hook tidak dipasang demi fail-open murni.");
    } else {
        let cache_hook = context_cache.clone();
        let matcher_hook = matcher.clone();
        let tx_hook = worker_tx.clone();
        let enabled_hook = is_enabled.clone();

        let hook_started = hook_manager.start(Box::new(move |event| {
            if !enabled_hook.load(Ordering::Relaxed) {
                return HookDecision::PassThrough;
            }

            let Ok(key_combo) = KeyCombo::parse(&event.key) else {
                return HookDecision::PassThrough;
            };

            let cached = cache_hook.get();
            if !cached.is_file_manager {
                return HookDecision::PassThrough;
            }

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

            let matched = {
                let lock = matcher_hook.try_read().ok();
                lock.and_then(|m| m.match_rule(&key_combo, &rt_ctx))
            };

            if let Some(rule) = matched {
                let _ = tx_hook.send(WorkerTask::ExecuteRule { rule });
                HookDecision::Swallow
            } else {
                HookDecision::PassThrough
            }
        }));

        if let Err(e) = hook_started {
            tracing::warn!(
                "Gagal memasang X11 keyboard hook: {e}. Aplikasi tetap berjalan dalam mode passthrough."
            );
        }
    }

    show_notification(
        "KeyFlow Siap",
        "KeyFlow aktif di latar belakang untuk Linux file manager.",
    );

    tracing::info!("KeyFlow Linux berjalan. Menunggu input shortcut...");

    // 9. Loop utama aplikasi Linux dengan penanganan sinyal graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        tracing::info!("Menerima sinyal terminasi (Ctrl+C), menyiapkan shutdown...");
        r.store(false, Ordering::SeqCst);
    }) {
        tracing::warn!("Gagal memasang penangan sinyal Ctrl+C: {e}");
    }

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(200));
    }

    // 10. Graceful shutdown bersih
    tracing::info!("Menutup KeyFlow Linux secara bersih...");
    hook_manager.stop();
    context_cache.stop();
    config_manager.stop_hot_reload();
    let _ = worker_tx.send(WorkerTask::Shutdown);
    drop(worker_tx);
    let _ = worker_handle.join();

    tracing::info!("KeyFlow Linux berhasil dimatikan secara bersih.");
    Ok(())
}

/// Menyiapkan direktori konfigurasi dan data lokal untuk Linux.
fn ensure_app_directories() -> anyhow::Result<(PathBuf, PathBuf)> {
    if let Some(proj) = ProjectDirs::from("com", "KeyFlow", "KeyFlow") {
        let config_dir = proj.config_dir().to_path_buf();
        let data_dir = proj.data_local_dir().to_path_buf();

        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&data_dir)?;

        Ok((config_dir, data_dir))
    } else {
        let base_dir = std::env::current_dir()?.join(".keyflow");
        fs::create_dir_all(&base_dir)?;
        Ok((base_dir.clone(), base_dir))
    }
}

/// Menulis template konfigurasi bawaan jika file belum ada.
fn ensure_default_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let default_content = r#"# KeyFlow - Context-Aware Shortcuts untuk Linux File Manager
# Dokumentasi dan panduan: https://github.com/your-repo/keyflow

version: 1

settings:
  dry_run: false
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 100

profiles:
  - name: "Default Linux File Manager"
    enabled: true
    context:
      app: "Nautilus"
    rules:
      # Pintasan Undo: Memulihkan aksi file terakhir yang dilakukan KeyFlow
      - key: "Ctrl+Shift+Z"
        action: undo
"#;

    fs::write(path, default_content)?;
    tracing::info!(path = %path.display(), "Template konfigurasi Linux bawaan berhasil dibuat");
    Ok(())
}

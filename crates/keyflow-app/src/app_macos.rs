//! Wiring aplikasi KeyFlow untuk macOS (T3.1–T3.4).
//!
//! Mengoordinasikan:
//! - Direktori konfigurasi (~/Library/Application Support/KeyFlow) & log undo
//! - Verifikasi izin Accessibility & Input Monitoring
//! - ConfigManager dengan hot-reload otomatis
//! - Keyboard Hook (CGEventTap) dengan evaluasi kilat via ContextCache
//! - Worker thread asinkron untuk eksekusi aksi file aman & undo
//! - macOS native notifications

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
use keyflow_platform::macos::{
    show_notification, verify_macos_permissions, ContextCache, MacosFinderContext, MacosHookManager,
};
use keyflow_platform::{HookDecision, KeyboardHook};

use crate::worker::{run_worker_loop, WorkerTask};

/// Titik masuk aplikasi macOS KeyFlow.
pub fn run() -> anyhow::Result<()> {
    tracing::info!("Memulai KeyFlow untuk macOS...");

    // 1. Verifikasi izin Accessibility & Input Monitoring macOS (T3.2)
    if let Err(e) = verify_macos_permissions() {
        tracing::error!("Izin macOS belum lengkap: {e}");
        return Err(anyhow::anyhow!("{e}"));
    }

    // 2. Direktori konfigurasi dan data
    let (config_dir, data_dir) = ensure_app_directories()?;
    let config_path = config_dir.join("config.yaml");
    ensure_default_config(&config_path)?;
    let undo_log_path = data_dir.join("undo_log.jsonl");

    tracing::info!(
        config_path = %config_path.display(),
        undo_log_path = %undo_log_path.display(),
        "Direktori aplikasi macOS terverifikasi"
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

    // 5. Inisialisasi UndoManager dengan log persisten
    let undo_manager = UndoManager::with_log_file(
        undo_log_path.clone(),
        initial_config.settings.undo_history_limit,
    )
    .map_err(|e| anyhow::anyhow!("Gagal membuka file log undo: {e}"))?;

    // 6. Inisialisasi komponen macOS Platform (Finder Context & Cache)
    let finder_context = Arc::new(MacosFinderContext::new());
    let context_cache = Arc::new(ContextCache::new(finder_context.clone()));

    // 7. Inisialisasi worker thread untuk eksekusi aksi file asinkron
    let (worker_tx, worker_rx) = unbounded::<WorkerTask>();
    let worker_fm = finder_context.clone();
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

    // 8. Pasang low-level CGEventTap hook (T3.1)
    let hook_manager = MacosHookManager::new();
    let cache_hook = context_cache.clone();
    let matcher_hook = matcher.clone();
    let tx_hook = worker_tx.clone();
    let enabled_hook = is_enabled.clone();

    hook_manager
        .start(Box::new(move |event| {
            if !enabled_hook.load(Ordering::Relaxed) {
                return HookDecision::PassThrough;
            }

            let Ok(key_combo) = KeyCombo::parse(&event.key) else {
                return HookDecision::PassThrough;
            };

            let cached = cache_hook.get();
            if !cached.is_finder {
                return HookDecision::PassThrough;
            }

            // Dummy slice penanda adanya seleksi tanpa alokasi heap besar
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
        }))
        .map_err(|e| anyhow::anyhow!("Gagal memasang CGEventTap: {e}"))?;

    show_notification(
        "KeyFlow Siap",
        "KeyFlow aktif di latar belakang untuk macOS Finder.",
    );

    tracing::info!("KeyFlow macOS berjalan. Menunggu input shortcut...");

    // 9. Loop utama aplikasi macOS dengan penanganan sinyal graceful shutdown
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
    tracing::info!("Menutup KeyFlow macOS secara bersih...");
    hook_manager.stop();
    context_cache.stop();
    config_manager.stop_hot_reload();
    let _ = worker_tx.send(WorkerTask::Shutdown);
    drop(worker_tx);
    let _ = worker_handle.join();

    tracing::info!("KeyFlow macOS berhasil dimatikan secara bersih.");
    Ok(())
}

/// Menyiapkan direktori konfigurasi dan data lokal untuk macOS.
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

    let default_content = r#"# KeyFlow - Context-Aware Shortcuts untuk macOS Finder
# Dokumentasi dan panduan: https://github.com/your-repo/keyflow

version: 1

settings:
  dry_run: false
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 100

profiles:
  - name: "Default macOS Finder"
    enabled: true
    context:
      app: "Finder"
    rules:
      # Pintasan Undo: Memulihkan aksi file terakhir yang dilakukan KeyFlow
      - key: "Ctrl+Shift+Z"
        action: undo
"#;

    fs::write(path, default_content)?;
    tracing::info!(path = %path.display(), "Template konfigurasi macOS bawaan berhasil dibuat");
    Ok(())
}

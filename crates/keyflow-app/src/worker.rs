//! Worker thread untuk eksekusi aksi file asinkron dan manajemen undo (T2.3).

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use crossbeam_channel::Receiver;
use keyflow_core::actions::{
    apply_rename_template, execute_file_action, ActionSummary, ExecutionOptions,
};
use keyflow_core::config::{ActionType, ThenAction};
use keyflow_core::matcher::MatchedRule;
use keyflow_core::undo::{UndoManager, UndoRecord, UndoTransaction};
use keyflow_platform::FileManagerContext;

/// Perintah yang dikirim ke worker thread.
#[derive(Debug, Clone)]
pub enum WorkerTask {
    ExecuteRule { rule: MatchedRule },
    ExecuteUndo,
    TogglePause(bool),
    Shutdown,
}

/// Tipe alias untuk callback notifikasi desktop.
pub type NotificationCallback = Box<dyn Fn(&str, &str) + Send + Sync>;

/// Menjalankan loop worker thread.
pub fn run_worker_loop<C: FileManagerContext + 'static>(
    rx: Receiver<WorkerTask>,
    context: Arc<C>,
    mut undo_manager: UndoManager,
    on_notify: NotificationCallback,
) {
    let mut is_paused = false;

    while let Ok(task) = rx.recv() {
        match task {
            WorkerTask::Shutdown => {
                tracing::info!("Worker menerima sinyal shutdown, mengakhiri loop");
                break;
            }
            WorkerTask::TogglePause(paused) => {
                is_paused = paused;
                tracing::info!(is_paused, "Status jeda worker diperbarui");
            }
            WorkerTask::ExecuteUndo => {
                handle_undo(&mut undo_manager, &*on_notify);
            }
            WorkerTask::ExecuteRule { rule } => {
                if is_paused {
                    tracing::warn!("Worker sedang dijeda, mengabaikan rule");
                    continue;
                }

                if rule.action == ActionType::Undo {
                    handle_undo(&mut undo_manager, &*on_notify);
                    continue;
                }

                handle_file_rule(&rule, &*context, &mut undo_manager, &*on_notify);
            }
        }
    }
}

fn handle_undo(undo_manager: &mut UndoManager, on_notify: &(dyn Fn(&str, &str) + Send + Sync)) {
    match undo_manager.undo_latest() {
        Some(report) => {
            if report.succeeded > 0 {
                let msg = format!(
                    "{} file berhasil dipulihkan ke lokasi asal",
                    report.succeeded
                );
                tracing::info!(report = ?report, "{msg}");
                on_notify("KeyFlow Undo", &msg);
            } else {
                let msg = format!("Gagal memulihkan file: {} galat ditemukan", report.failed);
                tracing::error!(report = ?report, "{msg}");
                on_notify("KeyFlow Undo Gagal", &msg);
            }
        }
        None => {
            tracing::info!("Tidak ada riwayat undo di stack");
            on_notify("KeyFlow Undo", "Tidak ada operasi yang dapat dibatalkan");
        }
    }
}

fn handle_file_rule<C: FileManagerContext>(
    rule: &MatchedRule,
    context: &C,
    undo_manager: &mut UndoManager,
    on_notify: &(dyn Fn(&str, &str) + Send + Sync),
) {
    // 0. Pertahanan mendalam: verifikasi ulang bahwa file manager
    // (Explorer / Finder / Nautilus / Dolphin / Thunar / Nemo / PCManFM / Caja)
    // masih merupakan jendela terdepan sebelum mengeksekusi aksi file.
    if let Ok(win) = context.focused_window() {
        let proc = win.process_name.to_lowercase();
        let is_fm = proc.contains("finder")
            || proc.contains("explorer")
            || proc.contains("nautilus")
            || proc.contains("dolphin")
            || proc.contains("thunar")
            || proc.contains("nemo")
            || proc.contains("pcmanfm")
            || proc.contains("caja")
            || proc.contains("files");
        if !proc.is_empty() && !is_fm {
            tracing::info!(
                proc,
                "File manager tidak lagi aktif saat worker akan mengeksekusi rule; dibatalkan demi keamanan data."
            );
            return;
        }
    }

    // Kueri seleksi segar langsung dari File Manager (menghindari cache staleness)
    let items = match context.selected_items() {
        Ok(it) if !it.is_empty() => it,
        _ => {
            tracing::info!("Tidak ada item yang dipilih saat worker memeriksa File Manager");
            return;
        }
    };

    let tx_id = format!(
        "tx-{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let target_dir = rule.to.as_deref().map(Path::new);
    let template = rule.template.as_deref();

    // 1. Log INTENT ke log persisten sebelum eksekusi dimulai (wajib lolos demi safety)
    // Sesuai Aturan Safety #23: mode dry_run tidak boleh menulis apa pun ke disk selain log tracing.
    if !rule.dry_run {
        for item in &items {
            let dest_hint = if let Some(d) = target_dir {
                Some(d.join(item.file_name().unwrap_or_default()))
            } else if let Some(tmpl) = template {
                let new_name = apply_rename_template(item, tmpl);
                item.parent().map(|p| p.join(new_name))
            } else {
                None
            };

            if let Err(e) =
                undo_manager.log_intent(&tx_id, &rule.action, item, dest_hint.as_deref())
            {
                tracing::error!("Gagal menulis intent log: {e}");
                on_notify(
                    "KeyFlow Galat Keamanan",
                    "Gagal mencatat log niat; operasi dibatalkan demi integritas data",
                );
                return;
            }
        }
    }

    let opts = ExecutionOptions {
        dry_run: rule.dry_run,
        on_conflict: rule.on_conflict,
        create_missing_dirs: rule.create_missing_dirs,
    };

    // 2. Eksekusi aksi file
    let summary: ActionSummary =
        execute_file_action(&rule.action, &items, target_dir, template, &opts);

    // 3. Log COMPLETION dan kumpulkan undo records
    let mut undo_records = Vec::new();

    for res in &summary.results {
        if !rule.dry_run {
            let _ = undo_manager.log_completion(
                &tx_id,
                &rule.action,
                &res.source,
                res.destination.as_deref(),
                res.success,
                res.error_message.clone(),
            );
        }

        if res.success && !res.is_noop && !res.is_skipped && !res.is_dry_run {
            if let Some(ref dest) = res.destination {
                undo_records.push(UndoRecord::new(
                    rule.action.clone(),
                    res.source.clone(),
                    dest.clone(),
                ));
            }
        }
    }

    // 4. Catat transaksi ke Undo Stack jika bukan dry_run dan ada file yang berubah
    if !undo_records.is_empty() && !rule.dry_run {
        undo_manager.push_transaction(UndoTransaction {
            id: tx_id,
            timestamp_epoch_ms: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            records: undo_records,
        });
    }

    // 5. Eksekusi then action (select_next) hanya jika minimal 1 item berhasil diproses
    if summary.succeeded > 0 {
        if let Some(ThenAction::SelectNext) = rule.then {
            if let Err(e) = context.select_next() {
                tracing::warn!("Gagal memajukan seleksi (select_next): {e}");
            }
        }
    }

    // 6. Notifikasi OS dengan pelaporan jujur multi-file
    let action_name = match rule.action {
        ActionType::Move => "dipindahkan",
        ActionType::Copy => "disalin",
        ActionType::Trash => "dipindahkan ke Trash",
        ActionType::Rename => "diubah namanya",
        ActionType::Undo => "dibatalkan",
    };

    let title = if rule.dry_run {
        "KeyFlow [DRY RUN]"
    } else {
        "KeyFlow"
    };

    let msg = if summary.failed > 0 && summary.succeeded > 0 {
        format!(
            "{} item berhasil {action_name}, {} item gagal",
            summary.succeeded, summary.failed
        )
    } else if summary.succeeded > 0 {
        format!("{} item berhasil {action_name}", summary.succeeded)
    } else if summary.noop > 0 {
        "Operasi tidak dilakukan (lokasi tujuan sama)".to_string()
    } else if summary.skipped > 0 {
        "Operasi dilewati karena konflik nama file".to_string()
    } else {
        format!("Gagal memproses {} item", summary.failed)
    };

    on_notify(title, &msg);
}

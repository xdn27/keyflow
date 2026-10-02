//! End-to-End Integration Test alur KeyFlow tanpa OS asli (T1.8, T1.9).
//!
//! Alur yang diuji:
//! Konfigurasi -> Matcher -> Mock Hook -> Mock Context -> Eksekusi File -> Log Persisten -> Undo -> Recovery.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use keyflow_core::actions::{execute_file_action, ExecutionOptions};
use keyflow_core::config::{Config, KeyCombo, ThenAction};
use keyflow_core::matcher::{ContextMatcher, RuntimeContext};
use keyflow_core::undo::{UndoManager, UndoRecord, UndoTransaction};
use keyflow_platform::mock::{MockFileManagerContext, MockKeyboardHook};
use keyflow_platform::{FileManagerContext, HookDecision, KeyEvent, KeyboardHook};

#[test]
fn test_end_to_end_shortcut_workflow() {
    let temp_root = tempfile::tempdir().unwrap();
    let root = temp_root.path();

    let src_dir = root.join("Foto").join("Mentah");
    let dest_dir = root.join("Foto").join("01_Dipakai");
    fs::create_dir_all(&src_dir).unwrap();

    let file_1 = src_dir.join("DSC_0001.JPG");
    let file_2 = src_dir.join("DSC_0002.JPG");
    File::create(&file_1).unwrap().write_all(b"foto 1").unwrap();
    File::create(&file_2).unwrap().write_all(b"foto 2").unwrap();

    // 1. Definisikan Konfigurasi YAML
    let yaml = format!(
        r#"
version: 1
settings:
  dry_run: false
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 50

profiles:
  - name: "Sorting Foto"
    enabled: true
    context:
      app: file_manager
      path: "{}/Foto/Mentah/**"
      selection: image
    rules:
      - key: "1"
        action: move
        to: "{}"
        then: select_next
      - key: "Ctrl+Shift+Z"
        action: undo
"#,
        root.display().to_string().replace('\\', "/"),
        dest_dir.display().to_string().replace('\\', "/")
    );

    let config = Config::from_yaml(&yaml).expect("Config harus valid");
    let matcher = Arc::new(ContextMatcher::compile(&config));

    // 2. Setup Mock Platform Context & Hook
    let mock_context = Arc::new(MockFileManagerContext::new());
    mock_context.set_focused_window("explorer.exe", "Mentah - File Explorer");
    mock_context.set_current_folder(Some(src_dir.clone()));
    mock_context.set_selected_items(vec![file_1.clone()]);

    let mock_hook = MockKeyboardHook::new();

    // 3. Setup UndoManager dengan persistent log
    let log_path = root.join("keyflow_undo.jsonl");
    let mut undo_mgr = UndoManager::with_log_file(log_path.clone(), 50).unwrap();

    // 4. Hubungkan Hook Handler (simulasi loop aplikasi)
    let matcher_clone = matcher.clone();
    let mock_ctx_clone = mock_context.clone();

    mock_hook
        .start(Box::new(move |event| {
            if !event.pressed {
                return HookDecision::PassThrough;
            }

            let Ok(key_combo) = KeyCombo::parse(&event.key) else {
                return HookDecision::PassThrough;
            };

            let win_info = mock_ctx_clone.focused_window().unwrap();
            let folder = mock_ctx_clone.current_folder().unwrap();
            let items = mock_ctx_clone.selected_items().unwrap();

            let rt_ctx = RuntimeContext {
                process_name: &win_info.process_name,
                current_folder: folder.as_deref(),
                selected_items: &items,
            };

            if matcher_clone.match_rule(&key_combo, &rt_ctx).is_some() {
                HookDecision::Swallow
            } else {
                HookDecision::PassThrough
            }
        }))
        .unwrap();

    // 5. Test 1: Tombol '1' di luar Explorer (mis. di Terminal) -> PassThrough
    mock_context.set_focused_window("alacritty", "Terminal");
    let dec_terminal = mock_hook.simulate_key(KeyEvent {
        key: "1".to_string(),
        pressed: true,
    });
    assert_eq!(dec_terminal, HookDecision::PassThrough);

    // 6. Test 2: Tombol '1' di Explorer di folder yang cocok -> Swallow
    mock_context.set_focused_window("explorer.exe", "Mentah - File Explorer");
    let dec_explorer = mock_hook.simulate_key(KeyEvent {
        key: "1".to_string(),
        pressed: true,
    });
    assert_eq!(dec_explorer, HookDecision::Swallow);

    // 7. Eksekusi Aksi Worker
    let key_1 = KeyCombo::parse("1").unwrap();
    let rt_ctx = RuntimeContext {
        process_name: "explorer.exe",
        current_folder: Some(&src_dir),
        selected_items: std::slice::from_ref(&file_1),
    };
    let matched = matcher.match_rule(&key_1, &rt_ctx).unwrap();

    let tx_id = "tx-integration-1";
    let target_path = PathBuf::from(matched.to.as_ref().unwrap());
    let opts = ExecutionOptions {
        dry_run: matched.dry_run,
        on_conflict: matched.on_conflict,
        create_missing_dirs: matched.create_missing_dirs,
    };

    // Log intent sebelum eksekusi
    undo_mgr
        .log_intent(tx_id, &matched.action, &file_1, Some(&target_path))
        .unwrap();

    // Eksekusi pemindahan
    let summary = execute_file_action(
        &matched.action,
        std::slice::from_ref(&file_1),
        Some(&target_path),
        None,
        &opts,
    );
    assert_eq!(summary.succeeded, 1);
    let moved_dest = summary.results[0].destination.clone().unwrap();

    // Log completion
    undo_mgr
        .log_completion(
            tx_id,
            &matched.action,
            &file_1,
            Some(&moved_dest),
            true,
            None,
        )
        .unwrap();

    // Simpan ke Undo Stack
    undo_mgr.push_transaction(UndoTransaction {
        id: tx_id.to_string(),
        timestamp_epoch_ms: 100,
        records: vec![UndoRecord::new(
            matched.action,
            file_1.clone(),
            moved_dest.clone(),
        )],
    });

    // Panggil then action (select_next)
    if let Some(ThenAction::SelectNext) = matched.then {
        mock_context.select_next().unwrap();
    }

    // Verifikasi kondisi disk setelah pemindahan
    assert!(!file_1.exists(), "File sumber harus sudah dipindah");
    assert!(moved_dest.exists(), "File tujuan harus ada");
    assert_eq!(mock_context.select_next_count(), 1);

    // 8. Test 3: Eksekusi Undo (Ctrl+Shift+Z)
    let undo_rep = undo_mgr.undo_latest().expect("Harus ada transaksi undo");
    assert_eq!(undo_rep.succeeded, 1);
    assert_eq!(undo_rep.failed, 0);

    // Verifikasi file kembali ke lokasi semula
    assert!(file_1.exists(), "File harus kembali ke asal");
    assert!(!moved_dest.exists(), "File di tujuan harus sudah tidak ada");

    // 9. Test 4: Verifikasi pemulihan log persisten dari disk
    let recovered_mgr = UndoManager::with_log_file(log_path, 50).unwrap();
    // Karena transaksi tx-integration-1 sudah berstatus 'Undone', stack hasil recovery harus kosong
    assert_eq!(recovered_mgr.len(), 0);
}

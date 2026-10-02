//! Spike Teknis M0 - Bukti Kelayakan Windows
//!
//! Menjalankan T0.1 (Keyboard Hook), T0.2 (Explorer COM Context), dan T0.3 (Safe Move).
//!
//! Cara menjalankan (di Windows):
//! ```bash
//! cargo run --example m0_windows_spike
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(target_os = "windows")]
fn main() {
    use std::io::{self, BufRead};
    use std::path::PathBuf;
    use std::time::Duration;

    println!("=== KeyFlow - Spike Teknis M0 (Windows) ===");
    println!("1. Memulai client COM Shell STA...");
    let shell_client = match keyflow_platform::windows::WindowsShellClient::new() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Gagal menginisialisasi COM Shell client: {e}");
            return;
        }
    };
    println!("   -> COM Shell STA aktif.");

    println!("2. Memasang hook WH_KEYBOARD_LL...");
    let mut hook_manager = keyflow_platform::windows::WindowsHookManager::new();
    if let Err(e) = hook_manager.start() {
        eprintln!("Gagal memasang hook keyboard: {e}");
        return;
    }
    println!(
        "   -> Hook aktif! Tekan '1' saat jendela Explorer fokus untuk menguji penelanan tombol."
    );

    println!("\nPetunjuk Pengujian:");
    println!("- Buka File Explorer (CabinetWClass), sorot satu atau beberapa file.");
    println!("- Tekan tombol '1': tombol akan ditelan dan file dipindahkan ke folder target uji.");
    println!(
        "- Tekan '1' di aplikasi lain (Notepad, Terminal, Browser): tombol akan lolos normal."
    );
    println!("- Tekan [Enter] di konsol ini untuk membaca konteks saat ini atau ketik 'q' lalu Enter untuk keluar.");

    let target_dir = PathBuf::from("./keyflow_test_target");
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        if let Some(Ok(line)) = lines.next() {
            if line.trim().eq_ignore_ascii_case("q") {
                break;
            }
        }

        // Ambil konteks jendela Explorer aktif saat ini
        match shell_client.get_active_context() {
            Ok(ctx) => {
                println!("\n--- Snapshot Konteks Explorer ---");
                println!("Judul Jendela : {}", ctx.window_title);
                println!(
                    "Folder Aktif  : {}",
                    ctx.active_folder
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(bukan folder filesystem / folder virtual)".to_string())
                );
                println!("Jumlah Seleksi: {}", ctx.selected_items.len());
                for (idx, item) in ctx.selected_items.iter().enumerate() {
                    println!("  [{}] {}", idx + 1, item.display());
                }
                println!("Latensi Query : {} µs", ctx.query_duration_us);

                if !ctx.selected_items.is_empty() {
                    println!("\nMenguji pemindahan file terpilih (T0.3)...");
                    let summary = keyflow_platform::windows::move_selected_files(
                        &ctx.selected_items,
                        &target_dir,
                    );
                    println!(
                        "Hasil: Total: {}, Sukses: {}, Gagal: {}, No-op: {}",
                        summary.total, summary.succeeded, summary.failed, summary.noop
                    );
                    for res in summary.results {
                        if res.success {
                            println!(
                                "  [OK] {} -> {}",
                                res.source.display(),
                                res.destination.display()
                            );
                        } else {
                            println!(
                                "  [GAGAL] {} ({:?})",
                                res.source.display(),
                                res.error_message
                            );
                        }
                    }
                }
            }
            Err(e) => {
                println!("Explorer tidak terdeteksi atau error: {e}");
            }
        }

        // Tampilkan statistik latensi hook
        let stats = keyflow_platform::windows::get_latency_stats();
        if stats.count > 0 {
            println!(
                "Statistik Latensi Hook: Count={}, Min={} µs, Max={} µs, Rata-rata={:.2} µs, Terakhir={} µs",
                stats.count,
                stats.min_us,
                stats.max_us,
                stats.average_us(),
                stats.last_us
            );
        }

        std::thread::sleep(Duration::from_millis(500));
    }

    println!("Menghentikan hook dan membersihkan resources...");
    hook_manager.stop();
    println!("Selesai.");
}

#[cfg(not(target_os = "windows"))]
fn main() {
    println!("=== KeyFlow - Spike Teknis M0 (Windows) ===");
    println!("Program contoh ini dirancang khusus untuk platform Windows.");
    println!("Pada host non-Windows, verifikasi dilakukan melalui:");
    println!("1. `cargo check --target x86_64-pc-windows-gnu --all-targets`");
    println!("2. `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings`");
    println!(
        "3. Pemeriksaan tipe Win32/COM Shell (`IShellWindows`, `IFolderView2`, `WH_KEYBOARD_LL`)."
    );
}

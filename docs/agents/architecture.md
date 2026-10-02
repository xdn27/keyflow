# Arsitektur

Kode OS-spesifik dipisahkan di balik trait agar inti aplikasi sama di semua platform.

## Struktur workspace

```
keyflow/
├── Cargo.toml            # workspace
├── crates/
│   ├── keyflow-core/     # config, matcher, actions, undo (TANPA kode OS)
│   ├── keyflow-platform/ # trait + implementasi windows/macos/linux (cfg-gated)
│   └── keyflow-app/      # main, tray, notifikasi, wiring
├── examples/config.yaml
├── docs/                 # PERMISSIONS.md, PLATFORM_SUPPORT.md
└── README.md
```

Gunakan `#[cfg(target_os = "...")]` untuk memisahkan kode platform. `cargo build` di tiap OS tidak boleh mengompilasi kode OS lain (dependensi OS-spesifik juga diberi `[target.'cfg(windows)'.dependencies]`).

## Trait platform

```rust
trait KeyboardHook {
    // Callback mengembalikan Swallow atau PassThrough secara SINKRON
    // dan harus sangat cepat.
    fn start(&self, handler: Box<dyn Fn(KeyEvent) -> HookDecision + Send + Sync>) -> Result<()>;
    fn stop(&self);
}

trait FileManagerContext {
    fn focused_window(&self) -> Result<WindowInfo>;      // nama proses, judul
    fn current_folder(&self) -> Result<Option<PathBuf>>;
    fn selected_items(&self) -> Result<Vec<PathBuf>>;
    fn select_next(&self) -> Result<()>;                 // best-effort
}
```

Trait harus bisa di-mock (lihat `testing.md`). Bentuk akhir trait boleh disesuaikan (mis. error type, `async`) selama kontrak sinkron untuk `KeyboardHook` terjaga.

## Modul inti (platform-agnostic, di `keyflow-core`)

| Modul | Tanggung jawab |
|---|---|
| `config` | parsing, validasi, hot-reload |
| `matcher` | pencocokan konteks, resolusi spesifisitas |
| `actions` | pelaksana aksi file (move/copy/trash/rename) + penanganan konflik nama |
| `undo` | stack undo + log persisten |

Di `keyflow-app`: `notify` (notifikasi OS), `tray` (ikon dan menu), dan wiring antar komponen.

## Aturan performa hook (kritis)

Callback hook harus selesai dalam **hitungan milidetik**. Hook low-level yang lambat membuat seluruh input sistem tersendat (di Windows, hook yang melewati timeout bisa di-bypass OS secara diam-diam).

Rancangan yang wajib diikuti:

1. **Cache konteks**: thread terpisah memperbarui `{window fokus, folder aktif}` ke struktur yang dibaca lock-free atau lock singkat (mis. `ArcSwap`/`RwLock` dengan `try_read`).
2. **Keputusan cepat**: callback hanya membaca cache + tabel rule yang sudah dikompilasi (tombol -> kandidat rule), lalu memutuskan `Swallow`/`PassThrough`. Tidak ada I/O, COM, alokasi besar, atau logging berat di sini.
3. **Kerja berat di worker**: ambil seleksi, pindah file, undo, notifikasi, `select_next` dikirim lewat channel (`crossbeam`/`tokio::mpsc`) ke worker thread.
4. **Fail-open**: jika cache belum siap, usang, atau ada error, putuskan `PassThrough`. Tidak pernah menelan tombol karena ragu.
5. **Tidak pernah panic** di dalam callback. Bungkus dengan `catch_unwind` bila perlu dan fail-open.
6. Event yang dihasilkan sendiri (mis. `SendInput` untuk `select_next`) harus ditandai agar hook tidak memprosesnya lagi (hindari loop).

Konsekuensi: keputusan swallow memakai konteks yang bisa sedikit usang. Rule yang menelan tombol harus tetap mengonfirmasi ulang konteks di worker sebelum bertindak. Bila ternyata tidak cocok, jangan lakukan aksi file dan beri notifikasi.

## Threading COM (Windows)

COM harus diinisialisasi (STA) di thread yang memakainya. Rancang satu thread khusus "shell context" yang memiliki semua objek COM; thread lain berkomunikasi lewat channel. Jangan melewatkan objek COM antar thread.

## Crate yang disarankan (verifikasi versi terbaru sebelum dipakai)

| Keperluan | Crate |
|---|---|
| Config | `serde`, `serde_yaml` (cek status pemeliharaan, pertimbangkan alternatif seperti `serde_yml`/`serde_norway`), `notify`, `directories` |
| Pola path | `globset` |
| File ops | `std::fs` + logika sendiri, atau `fs_extra`; `trash` |
| Hook/input | `windows` (Win32/COM), `rdev` atau `core-graphics` (macOS), `enigo` |
| Window aktif | `active-win-pos-rs` |
| Tray | `tray-icon` |
| Notifikasi | `notify-rust` |
| Logging | `tracing`, `tracing-subscriber` |
| Error | `thiserror` (library), `anyhow` (aplikasi) |
| Async/threading | `tokio` atau thread + `crossbeam` channel |
| Test | `tempfile`, `assert_fs` |

Jika sebuah crate tidak lagi terawat atau tidak cocok, pilih alternatif dan jelaskan alasannya singkat.

## Titik perluasan untuk pasca-MVP

- `Action` sebagai enum yang dapat diperluas (atau trait `ActionProvider`) agar aksi skrip/sistem dapat ditambah tanpa mengubah matcher.
- `FileManagerContext` memiliki satu implementasi per file manager; pemilihan implementasi berdasarkan proses window fokus.
- Config bersifat berversi. GUI pengaturan (`keyflow-app/src/settings_ui`) membaca dan menulis berkas yang sama lewat `keyflow-core::config_edit` (`patch_settings` menambal teks YAML tanpa mengubah komentar; `render_full` hanya jalur cadangan dengan konfirmasi). GUI berjalan sebagai **proses terpisah** (`keyflow settings`) sehingga event loop egui tidak bercampur dengan hook/tray; perubahan diterapkan lewat hot-reload yang sudah ada. Tahap 2 (editor profil/rule) cukup menambah fungsi patch untuk blok `profiles:`.

# Desain: GUI Pengaturan KeyFlow (Tahap 1)

Tanggal: 2026-10-03
Status: menunggu review pengguna

## Tujuan

Pengguna dapat mengubah pengaturan global KeyFlow lewat jendela GUI tanpa mengedit `config.yaml` secara manual. `config.yaml` tetap menjadi satu-satunya sumber kebenaran; GUI hanyalah editor atas berkas yang sama.

## Cakupan

### Tahap 1 (spec ini)
Hanya blok `settings:` global:

| Field | Kontrol | Catatan |
|---|---|---|
| `dry_run` | checkbox | Keterangan: berkas fisik tidak disentuh |
| `notifications` | checkbox | |
| `on_conflict` | pilihan `rename` / `skip` / `overwrite` / `ask` | `rename` ditandai "disarankan" |
| `create_missing_dirs` | checkbox | |
| `undo_history_limit` | input angka | Batas minimum dan maksimum ditetapkan saat implementasi, mengikuti validasi `keyflow-core` |

### Tahap 2 (spec terpisah, di luar cakupan)
Editor profil, konteks (app, path glob, selection), dan rule (tombol, aksi, tujuan). Desain tahap 1 tidak boleh menghalanginya (lihat "Perluasan tahap 2").

## Batasan dan asumsi

Mengikuti `AGENTS.md`/`CLAUDE.md`:
- Tidak ada akses jaringan dan tidak ada telemetri.
- Config rusak tidak boleh membuat aplikasi crash, dan GUI tidak boleh pernah menyimpan config yang tidak valid.
- Tidak pernah menimpa file pengguna secara diam-diam. Default `on_conflict` tetap `rename`.
- Tidak ada `unwrap()`/`expect()` di jalur produksi. Logging dengan `tracing`.
- `keyflow-core` tidak bergantung pada kode OS maupun toolkit GUI.
- Callback hook tidak boleh tersentuh oleh perubahan ini.

Catatan platform: tray saat ini hanya ada di Windows. Di macOS dan Linux, GUI dibuka lewat `keyflow settings` (terminal atau pintasan desktop) sampai tray untuk platform tersebut tersedia. Tidak ada klaim dukungan tray di platform yang belum memilikinya.

## Arsitektur

Pendekatan terpilih: **egui/eframe, proses terpisah**.

Alasan: event loop tray Windows dan event loop toolkit GUI bentrok bila satu proses, dan GUI yang crash tidak boleh mematikan hook (Aturan #5). Alternatif yang ditolak: Tauri (toolchain Node, `webkit2gtk`, CI dan biner lebih berat, berlebihan untuk lima field) dan egui satu proses (bentrok event loop, risiko ke jalur hook).

### Komponen

1. **`keyflow-core/src/config_edit.rs` (baru, tanpa OS).**
   - `patch_settings(yaml: &str, new: &Settings) -> PatchResult`: mengganti isi blok `settings:` dengan menyentuh baris-baris blok itu saja, sehingga komentar dan profil di luar blok tidak berubah. `PatchResult` adalah `Patched(String)` atau `Unpatchable(PatchReason)`.
   - `render_full(config: &Config) -> Result<String, ConfigError>`: tulis ulang penuh lewat serde (jalur fallback; komentar hilang).
   - Error memakai `thiserror`.
2. **`keyflow-app/src/settings_ui/` (baru).**
   - `state.rs`: struct state form beserta fungsi murni (perubahan field, deteksi "dirty", validasi awal) yang dapat diuji tanpa egui.
   - `view.rs`: rendering egui; tidak berisi logika bisnis.
   - `save.rs`: orkestrasi alur simpan (hash, patch, validasi, tulis atomik).
3. **`keyflow-app/src/main.rs`:** subperintah `settings` membuka jendela. Tanpa subperintah, perilaku sekarang tidak berubah.
4. **`keyflow-app/src/tray.rs` (Windows):** item menu baru "Pengaturan…" yang menjalankan `keyflow settings` sebagai proses anak.

Proses utama tidak mendapat jalur komunikasi baru. Hot-reload yang sudah ada (`ConfigManager::start_hot_reload`) mengambil berkas yang disimpan dan menukar `Arc<Config>` secara atomik.

## Alur simpan

1. GUI memuat `config.yaml`, mengingat hash isinya, dan mengisi form.
2. Saat "Simpan", hash berkas di disk dibandingkan dengan hash saat dimuat. Bila berbeda (berkas diubah di tempat lain), GUI menampilkan peringatan dan menawarkan muat ulang; tidak ada penimpaan.
3. Panggil `patch_settings`. Bila `Patched`, lanjut ke langkah 5.
4. Bila `Unpatchable`, tampilkan dialog berisi alasan dengan dua pilihan: "Tulis ulang penuh (komentar hilang; cadangan `config.yaml.bak` dibuat)" atau "Batal". Jalur penuh hanya berjalan setelah konfirmasi eksplisit.
5. Validasi teks hasil dengan `Config::from_yaml` sebelum menyentuh disk. Bila gagal, **tidak ada yang ditulis**; error ditampilkan lengkap dengan nomor baris.
6. Tulis atomik: berkas temp di direktori yang sama, lalu `rename`. Hot-reload aplikasi yang berjalan menerapkan hasilnya.

## Penanganan error dan keselamatan data

- **Berkas config tidak ada:** GUI menampilkan default aman (`Config::default_safe`) dan membuat berkas baru saat simpan.
- **Config yang ada tidak valid:** GUI tidak menimpa. Ia menampilkan error validasi dan meminta pengguna memperbaikinya lebih dulu.
- **`on_conflict: overwrite`:** memilihnya memunculkan dialog konfirmasi eksplisit yang menjelaskan bahwa berkas lama di tujuan akan ditimpa (Aturan #3). Pembatalan mengembalikan nilai sebelumnya.
- **Gagal tulis (izin, disk penuh):** tampilkan pesan jelas; berkas asli tidak berubah karena penulisan atomik.
- Cadangan `.bak` hanya dibuat di jalur tulis-ulang-penuh.

## Dependensi

- `eframe`/`egui` ditambahkan hanya ke `keyflow-app`, sebagai dependensi umum (bukan hanya Windows) karena GUI berjalan di ketiga OS. Versi terbaru diverifikasi dengan `cargo add` sebelum dipakai, sesuai konvensi proyek; alasan pemilihan dicatat di commit.
- Linux: CI dan dokumentasi build perlu paket pengembangan X11/Wayland tambahan. Ini diperbarui di workflow rilis dan `docs/PLATFORM_SUPPORT`.
- Tidak ada dependensi jaringan yang masuk lewat `eframe` (fitur diperiksa dan dikunci seminimal mungkin).

## Pengujian

- **`config_edit` (unit test, `tempfile`):**
  - Komentar dan profil di luar `settings:` tetap utuh setelah patch.
  - Struktur tak terduga (mis. `settings:` bergaya flow `{...}`, duplikat, atau tidak ditemukan) menghasilkan `Unpatchable`, bukan tebakan.
  - Setiap hasil `Patched` lolos `Config::from_yaml` dan bernilai sama dengan input.
  - Round-trip `render_full` menghasilkan `Config` yang setara.
- **`settings_ui::state` dan `save`:** unit test untuk deteksi dirty, penolakan saat hash berubah, tidak menulis ketika validasi gagal, dan penulisan atomik (berkas asli utuh saat gagal).
- **Rendering egui:** tidak diuji otomatis; ditambahkan ke checklist uji manual di `docs/agents/testing.md` (buka dari tray, ubah tiap field, simpan, lihat hot-reload aktif, uji konfirmasi `overwrite`, uji alur `Unpatchable`).
- Seluruh perintah di `CLAUDE.md` (`build`, `test`, `clippy -D warnings`, `fmt --check`) harus lolos. Kode GUI tidak boleh membuat kode `windows` terkompilasi di Linux/macOS.

## Perluasan tahap 2

`config_edit` terpisah dari GUI dan beroperasi pada model `Config`, sehingga tahap 2 menambah fungsi patch untuk blok `profiles:` tanpa mengubah kontrak tahap 1. Karena logika disimpan di `keyflow-core`, frontend dapat diganti (mis. Tauri) bila editor rule terbukti terlalu berat di egui.

## Pembaruan dokumen

- `docs/agents/product.md`: pindahkan "UI pengaturan" dari pasca-MVP ke fitur yang dikerjakan (tahap 1).
- `docs/agents/architecture.md`: tambahkan `config_edit` dan `settings_ui`, serta catatan pemisahan proses.
- `docs/agents/testing.md`: tambahkan checklist uji manual GUI.
- `AGENTS.md` (sumber `CLAUDE.md`): perbarui bagian Status.

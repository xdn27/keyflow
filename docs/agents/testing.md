# Pengujian

## Prinsip

- Logika inti harus teruji **tanpa OS asli**. Semua akses OS lewat trait yang bisa di-mock.
- Test file memakai direktori sementara (`tempfile`, `assert_fs`). Jangan pernah menyentuh folder pengguna.
- Uji skenario gagal sama seriusnya dengan skenario sukses, terutama di `actions` dan `undo`.

## Unit test

- **Parser config**: contoh valid, field tidak dikenal, versi salah, YAML rusak (cek nomor baris di pesan error).
- **Validasi**: tombol tidak valid, aksi tidak dikenal, duplikasi tombol dalam profil, tujuan berbahaya, glob rusak.
- **Matcher**: kecocokan tiap lapis konteks; **spesifisitas** (lokasi tanpa wildcard > wildcard > tanpa lokasi); profil disabled; tombol sama di profil berbeda; tidak ada rule -> `PassThrough`.
- **Penamaan konflik**: `foto.jpg` -> `foto (1).jpg` -> `foto (2).jpg`; file tanpa ekstensi; nama dengan titik ganda.
- **Undo**: stack berlapis, urutan terbalik, batas `undo_history_limit`, path asal sudah ditempati, file sudah dipindah/hilang.
- **Keputusan hook**: cache kosong/usang -> fail-open (`PassThrough`).

## Integration test aksi file

Pakai direktori sementara untuk: move, copy, trash (Trash di-mock/abstraksi), rename dengan template, konflik semua mode `on_conflict`, banyak file dengan sebagian gagal, dry run (tidak ada perubahan di disk), pindah lintas-drive (simulasikan lewat abstraksi "rename gagal -> copy+verify").

## Mock platform

Sediakan `MockKeyboardHook` dan `MockFileManagerContext` di `keyflow-platform` (modul `mock`, tanpa dependensi OS) agar alur lengkap tombol -> keputusan -> aksi -> undo dapat diuji end-to-end tanpa OS.

## Properti yang layak diuji (opsional, `proptest`)

- Penamaan konflik selalu menghasilkan nama yang belum ada.
- `aksi` lalu `undo` mengembalikan keadaan awal untuk move/rename.
- Tidak ada operasi yang pernah menimpa file yang sudah ada saat `on_conflict != overwrite`.

## Uji manual per OS (checklist di `docs/`)

Checklist lengkap ada di `docs/MANUAL_TESTING.md` (perbarui bila fitur berubah). Ringkasan:

- [ ] Di folder cocok, tombol `1` memindahkan file terpilih; seleksi maju.
- [ ] Di folder lain dan di aplikasi lain, tombol `1` bekerja normal (mis. mengetik angka di Notepad).
- [ ] Banyak file terpilih sekaligus.
- [ ] Tidak ada file terpilih -> notifikasi, tombol tertelan.
- [ ] `Ctrl+Shift+Z` berulang kali memulihkan aksi berurutan.
- [ ] Konflik nama di tujuan (tidak ada yang tertimpa).
- [ ] Pindah ke drive lain (USB/partisi lain).
- [ ] Edit config valid -> aktif tanpa restart; config rusak -> notifikasi, perilaku lama tetap.
- [ ] `dry_run` tidak mengubah file.
- [ ] Menu tray: enable/disable, reload, buka config, buka log, keluar.
- [ ] CPU idle mendekati 0%; tidak ada lag saat mengetik cepat.
- [ ] (macOS) alur izin Accessibility/Input Monitoring.
- [ ] (Linux) sesi Wayland menampilkan peringatan dan tidak menelan tombol.

## CI

Matriks `windows-latest`, `macos-latest`, `ubuntu-latest`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`. Test yang butuh OS asli (hook, COM) ditandai `#[ignore]` dan dijalankan manual.

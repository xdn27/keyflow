# Milestone dan cara kerja

Kerjakan bertahap. **Berhenti di akhir setiap milestone** untuk memastikan semuanya berjalan sebelum lanjut, lalu beri ringkasan: apa yang selesai, apa yang diuji, apa yang belum.

## M0 - Spike teknis (Windows)

Buktikan tiga hal sebelum lanjut:

- (a) Low-level hook menelan tombol `1` **hanya** saat Explorer fokus.
- (b) Membaca folder aktif dan file terpilih via COM.
- (c) Memindahkan file terpilih ke satu folder.

Kode spike boleh kasar tetapi diberi label sementara. Jangan lanjut ke M1 sebelum ketiganya terbukti. Catat temuan (latensi hook, kendala COM, kasus aneh) di `docs/` agar M2 memakainya.

## M1 - Inti

Config + validasi + hot-reload, matcher, aksi move/copy/trash, undo + log. Semua dengan unit test dan tanpa dependensi OS. Aturan di `safety.md` berlaku penuh di sini.

## M2 - Aplikasi Windows lengkap

Tray, notifikasi, profil, `select_next`, dry run. Wiring hook -> cache konteks -> worker sesuai `architecture.md`.

## M3 - macOS

Implementasi trait platform + penanganan izin (Accessibility, Input Monitoring) dengan pesan yang jelas.

## M4 - Linux (X11)

Implementasi trait platform + deteksi Wayland dengan peringatan, tanpa menelan tombol di Wayland.

## M5 - Pengemasan

CI (GitHub Actions) build untuk 3 OS, binary release, dokumentasi (`README.md`, `docs/PERMISSIONS.md`, `docs/PLATFORM_SUPPORT.md`, `docs/MANUAL_TESTING.md`), `examples/config.yaml`.

## Kriteria selesai MVP (Windows)

1. Di `D:\Foto\Mentah` dalam Explorer, menekan `1` memindahkan foto terpilih ke `01_Dipakai` dan seleksi maju.
2. Di luar folder itu atau di aplikasi lain, tombol `1` berfungsi normal.
3. `Ctrl+Shift+Z` mengembalikan beberapa aksi terakhir secara berurutan.
4. Mengubah config otomatis aktif tanpa restart; config rusak tidak membuat aplikasi crash.
5. Tidak ada file yang hilang atau tertimpa pada skenario konflik nama dan pindah antar-drive.
6. CPU idle mendekati 0% dan latensi tombol terasa instan.

## Cara kerja yang diharapkan

- Awali milestone dengan rencana singkat, lalu kerjakan.
- Keputusan desain ambigu: pilih opsi paling aman, jelaskan singkat, lanjut. Jika menyangkut keamanan data atau perilaku yang menelan tombol, konfirmasi dulu.
- Setiap perubahan lolos `cargo fmt`, `cargo clippy -D warnings`, dan `cargo test`.
- Kode idiomatik; jelaskan keterbatasan platform dengan jujur.
- Gunakan subagent: `explorer` untuk riset kode, `test-writer` untuk test, `data-safety-auditor` sebelum merge perubahan di `actions`/`undo`, `code-reviewer` di akhir tiap milestone.

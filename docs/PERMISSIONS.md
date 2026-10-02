# Izin yang dibutuhkan

KeyFlow memantau keyboard secara global, sehingga OS meminta izin tertentu. KeyFlow tidak mengirim data ke mana pun (tanpa jaringan dan telemetri).

## Windows

- Tidak butuh izin khusus untuk hook keyboard tingkat rendah.
- Aplikasi yang berjalan sebagai Administrator tidak menerima hook/`SendInput` dari proses non-elevated. Untuk shortcut di jendela elevated, KeyFlow juga harus dijalankan sebagai Administrator.
- Antivirus dapat menandai hook keyboard global; binary rilis sebaiknya ditandatangani.

## macOS

Butuh dua izin di **System Settings -> Privacy & Security**:

1. **Accessibility**
2. **Input Monitoring**

KeyFlow mendeteksi izin yang belum diberikan dan menampilkan petunjuk. Setelah memberi izin, KeyFlow mungkin perlu dijalankan ulang. Kontrol Finder lewat AppleScript dapat memicu dialog **Automation** ("KeyFlow ingin mengontrol Finder"); izinkan agar folder aktif dan seleksi terbaca.

## Linux

- X11: tidak butuh izin khusus.
- Wayland: lihat `PLATFORM_SUPPORT.md`; global key grab tidak tersedia secara standar.

> TODO(M3/M5): sesuaikan teks dengan perilaku sebenarnya setelah diuji di tiap OS.

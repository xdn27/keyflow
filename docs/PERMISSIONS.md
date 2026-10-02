# Izin yang dibutuhkan

KeyFlow memantau keyboard secara global, sehingga OS meminta izin tertentu. KeyFlow tidak mengirim data ke mana pun (tanpa jaringan dan telemetri).

## Windows

- Tidak butuh izin khusus untuk hook keyboard tingkat rendah.
- Aplikasi yang berjalan sebagai Administrator tidak menerima hook/`SendInput` dari proses non-elevated. Untuk shortcut di jendela elevated, KeyFlow juga harus dijalankan sebagai Administrator.
- Antivirus dapat menandai hook keyboard global; binary rilis sebaiknya ditandatangani.

## macOS

Butuh dua izin utama di **System Settings -> Privacy & Security**:

1. **Accessibility**: KeyFlow memeriksa izin ini saat startup via `AXIsProcessTrusted()`. Jika belum aktif, KeyFlow memanggil `AXIsProcessTrustedWithOptions` untuk memicu dialog prompt izin macOS secara otomatis.
2. **Input Monitoring**: Dibutuhkan untuk menerima dan memproses event keyboard tingkat rendah via `CGEventTap`.
3. **Automation (Finder)**: Saat pertama kali berinteraksi dengan Finder lewat AppleScript (`osascript`), macOS akan menampilkan dialog *"KeyFlow ingin mengontrol Finder"*. Klik **OK/Allow** agar folder aktif dan item seleksi dapat terbaca.

Jika izin belum diberikan, KeyFlow mengembalikan pesan kesalahan yang jelas dan menolak berjalan (fail-safe) tanpa menelan tombol apa pun.

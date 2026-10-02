# Checklist uji manual

Jalankan di tiap OS sebelum rilis. Gunakan folder uji sendiri berisi salinan file, **jangan** data asli. Tandai hasil per OS (W = Windows, M = macOS, L = Linux X11).

| # | Skenario | W | M | L |
|---|---|---|---|---|
| 1 | Di folder cocok, tombol `1` memindahkan file terpilih; seleksi maju | [ ] | [ ] | [ ] |
| 2 | Di folder lain dan di aplikasi lain (mis. editor teks), `1` mengetik angka normal | [ ] | [ ] | [ ] |
| 3 | Banyak file terpilih sekaligus | [ ] | [ ] | [ ] |
| 4 | Tidak ada file terpilih: notifikasi muncul, tombol tertelan | [ ] | [ ] | [ ] |
| 5 | `Ctrl+Shift+Z` berulang memulihkan aksi berurutan | [ ] | [ ] | [ ] |
| 6 | Konflik nama di tujuan: tidak ada yang tertimpa (`foto (1).jpg`) | [ ] | [ ] | [ ] |
| 7 | Pindah ke drive/volume lain; sumber hanya hilang setelah verifikasi | [ ] | [ ] | [ ] |
| 8 | Edit config valid: aktif tanpa restart | [ ] | [ ] | [ ] |
| 9 | Config rusak: notifikasi + perilaku lama tetap, tidak crash | [ ] | [ ] | [ ] |
| 10 | `dry_run` global dan per profil tidak mengubah file | [ ] | [ ] | [ ] |
| 11 | Menu tray: enable/disable, reload, buka config, buka log, keluar | [ ] | [ ] | [ ] |
| 12 | CPU idle mendekati 0%; tidak ada lag saat mengetik cepat | [ ] | [ ] | [ ] |
| 13 | `trash` masuk Recycle Bin/Trash dan bisa dipulihkan dari sana | [ ] | [ ] | [ ] |
| 14 | Profil disabled tidak menelan tombol | [ ] | [ ] | [ ] |
| 15 | Dua profil cocok: rule yang lebih spesifik menang | [ ] | [ ] | [ ] |
| 16 | Log aksi berisi asal, tujuan, waktu, status | [ ] | [ ] | [ ] |
| 17 | (macOS) alur izin Accessibility / Input Monitoring jelas | | [ ] | |
| 18 | (Linux) sesi Wayland: peringatan tampil, tidak ada tombol tertelan | | | [ ] |
| 19 | (Windows) Explorer dengan tab baru dan banyak jendela Explorer | [ ] | | |
| 20 | Kill proses saat aksi berjalan: tidak ada file hilang; log cukup untuk pemulihan | [ ] | [ ] | [ ] |

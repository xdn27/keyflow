# Keamanan data (WAJIB)

Pengguna mempercayakan ribuan file pribadi pada aplikasi ini. Kehilangan atau penimpaan file adalah kegagalan terburuk. Ketika ragu, pilih opsi yang tidak merusak: lewati aksi dan laporkan.

## Aturan

1. **Tidak ada hapus permanen.** `trash` selalu ke Recycle Bin/Trash (crate `trash`). Tidak ada `fs::remove_*` pada file pengguna kecuali: (a) menghapus sumber setelah copy lintas-drive terverifikasi, (b) membersihkan file sementara milik KeyFlow sendiri.
2. **Log sebelum dan sesudah** setiap `move`/`copy`/`rename`: asal, tujuan, waktu, status. Entri "niat" ditulis dan di-flush sebelum eksekusi, sehingga crash di tengah jalan tetap bisa dipulihkan.
3. **Undo aman**: mengembalikan file ke lokasi dan nama asal. Jika path asal sudah ditempati, **gagal dengan aman** (jangan menimpa; laporkan). Undo dari aksi `copy` menghapus salinan hanya lewat Trash, dan hanya jika salinan itu masih identik dengan yang dibuat.
4. **Konflik nama** mengikuti `on_conflict`. Default `rename` (sufiks angka, mis. `foto (1).jpg`). **Tidak pernah menimpa** tanpa konfirmasi eksplisit (`overwrite` hanya bila pengguna menyetelnya; `ask` meminta konfirmasi).
5. **Pindah antar-drive**: copy, verifikasi ukuran (dan hash untuk file yang dianggap penting/di bawah ambang yang ditentukan), baru hapus sumber (via Trash bila memungkinkan). Jika verifikasi gagal, sumber dibiarkan utuh dan salinan parsial dibersihkan.
6. **Atomik per file** pada aksi banyak file: satu kegagalan tidak membatalkan file yang sudah berhasil, tetapi harus dilaporkan (jumlah berhasil/gagal di notifikasi dan log).
7. **Tolak path berbahaya**: root drive, direktori OS (`C:\Windows`, `/`, `/usr`, `/System`, dsb.), dan folder profil pengguna tingkat atas sebagai tujuan/asal, kecuali dikonfigurasi eksplisit. Validasi di config dan lagi saat eksekusi (setelah normalisasi/resolusi symlink).
8. **Tidak ada jaringan, tidak ada telemetri.** Jangan menambah dependensi yang melakukan panggilan jaringan.

## Detail yang sering terlewat

- Normalisasi path (`..`, symlink, huruf drive, kapitalisasi) sebelum membandingkan atau memeriksa path berbahaya.
- Tujuan di dalam sumber (memindahkan folder ke dalam dirinya sendiri) harus ditolak.
- Asal == tujuan: no-op, bukan error dan bukan rename.
- Nama file dengan karakter khusus/Unicode/sangat panjang; nama yang dicadangkan Windows (`CON`, `NUL`, dst.).
- File sedang dipakai/terkunci: laporkan, jangan retry tanpa batas.
- Mode `dry_run` tidak boleh menulis apa pun ke disk selain log "dry run".
- Hindari TOCTOU: gunakan operasi yang gagal bila tujuan sudah ada (mis. `create_new`/`rename` tanpa overwrite) alih-alih "cek lalu tulis".
- Format log persisten harus tahan crash (append-only, satu entri per baris, mis. JSON Lines).

## Checklist review sebelum merge perubahan di `actions`/`undo`

- [ ] Adakah jalur yang bisa menimpa file? Apakah diuji?
- [ ] Adakah jalur yang menghapus permanen?
- [ ] Log "sebelum" ditulis sebelum operasi?
- [ ] Undo menangani path asal yang sudah terisi?
- [ ] Skenario lintas-drive dan konflik nama punya test?
- [ ] Kegagalan sebagian dilaporkan, bukan ditelan?

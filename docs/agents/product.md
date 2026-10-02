# Produk

## Tujuan

Aplikasi latar belakang (system tray) yang menjalankan shortcut keyboard sadar konteks khusus file manager. Kasus utama: mensortir ribuan foto di File Explorer. Pengguna menyorot foto, menekan `1`, file pindah ke `01_Dipakai`. `2` ke `02_Cadangan`, `3` menyalin ke `03_Posting`, `4` ke `04_Reject`, lalu seleksi maju ke file berikutnya.

KeyFlow memakai file manager bawaan OS. Tidak ada GUI browsing sendiri.

## Alur eksekusi setiap tombol

1. Hook keyboard menerima key event.
2. Cek window fokus. Bukan file manager yang didukung: teruskan tombol, selesai.
3. Ambil folder aktif. Tidak ada profil yang cocok: teruskan tombol, selesai.
4. Cari rule yang cocok dengan tombol + konteks. Tidak ada: teruskan tombol.
5. Ambil daftar file terpilih. Kosong: tampilkan notifikasi, **telan** tombol.
6. Jalankan aksi, catat ke undo log, kirim notifikasi singkat.
7. Opsional: majukan seleksi ke item berikutnya (`then: select_next`).

**Tombol hanya DITELAN jika konteks cocok dan rule dijalankan. Jangan pernah menelan tombol di luar konteks.**

Catatan implementasi: langkah 2-4 harus diputuskan dari cache konteks (lihat `architecture.md`), bukan dengan memanggil OS di dalam callback hook. Langkah 5-7 berjalan di worker thread setelah keputusan swallow dibuat.

## Sistem konteks (3 lapis)

1. **Aplikasi** (wajib): window fokus adalah file manager yang didukung.
2. **Lokasi** (opsional): folder aktif cocok dengan pola glob, mis. `D:/Foto/Mentah/**`.
3. **Seleksi** (opsional): jenis item terpilih: hanya gambar, hanya video, minimal 1 file, atau ekstensi tertentu.

Jika beberapa rule cocok untuk tombol yang sama, **rule dengan konteks paling spesifik menang**. Urutan spesifisitas:

1. pola lokasi tanpa wildcard
2. pola lokasi dengan wildcard
3. tanpa lokasi

## Fitur MVP

- Hotkey keyboard yang bisa ditelan kondisional (tanpa modifier diperbolehkan, mis. tombol `1`).
- Deteksi window fokus, folder aktif, dan file terpilih.
- Aksi: `move`, `copy`, `trash` (ke Recycle Bin/Trash), `rename` (pola template).
- Opsi `then: select_next` setelah aksi.
- Profil: kumpulan rule dengan konteks bersama, bisa di-enable/disable.
- Undo berlapis (stack). Hotkey undo dapat dikonfigurasi, default `Ctrl+Shift+Z`.
- Log aksi persisten (asal, tujuan, waktu, status).
- Notifikasi OS singkat setelah aksi ("Dipindahkan ke 01_Dipakai").
- System tray: enable/disable global, reload config, buka folder config, buka log, keluar.
- Mode `dry_run` global dan per profil: hanya menampilkan apa yang akan terjadi.
- Hot-reload konfigurasi saat file berubah.

## Pasca-MVP (JANGAN dikerjakan, tapi desain tidak boleh menghalanginya)

- UI pengaturan (kandidat: Tauri atau egui).
- Preset "Sorting Foto" satu klik.
- Aksi: jalankan skrip dengan path file sebagai argumen, buka dengan aplikasi tertentu, buat subfolder otomatis berdasarkan tanggal/ekstensi.
- Aksi sistem (toggle Wi-Fi, volume, dsb.) sebagai action provider terpisah.
- File manager pihak ketiga (Total Commander, Directory Opus, Dolphin, Thunar, dll.).

Implikasi desain: `Action` sebaiknya berupa enum/trait yang mudah ditambah; deteksi file manager lewat trait `FileManagerContext` yang bisa punya banyak implementasi per OS; model config punya `version` agar bisa dimigrasi.

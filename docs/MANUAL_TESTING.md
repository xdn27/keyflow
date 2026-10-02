# Panduan & Checklist Pengujian Manual

Dokumen ini adalah panduan langkah-demi-langkah untuk melakukan pengujian manual sebelum rilis (*acceptance testing*) pada setiap platform yang didukung.

> [!CAUTION]
> **PENTING**: Selalu gunakan folder uji khusus (berisi berkas tiruan/dummy), **JANGAN PERNAH** menguji pada berkas data asli Anda!

---

## 📋 Checklist Skenario Uji

Tandai hasil pengujian per sistem operasi:
- **W**: Windows 10 / 11
- **M**: macOS (Monterey / Ventura / Sonoma / Sequoia)
- **L**: Linux X11 (Ubuntu / Fedora / Arch / Mint)

| # | Skenario Pengujian | W | M | L |
|---|---|:---:|:---:|:---:|
| 1 | **Eksekusi Tombol Cocok**: Di folder target dengan file terpilih, menekan `1` memindahkan file dan seleksi maju otomatis (`select_next`). | [ ] | [ ] | [ ] |
| 2 | **Fail-Open di Luar Konteks**: Di folder yang tidak cocok atau di aplikasi lain (mis. Text Editor / Browser), menekan `1` mengetik angka `1` secara normal tanpa delay atau hilang. | [ ] | [ ] | [ ] |
| 3 | **Multi-Seleksi**: Memilih 5-10 file sekaligus lalu menekan shortcut; seluruh file terpilih dipindahkan bersamaan. | [ ] | [ ] | [ ] |
| 4 | **Seleksi Kosong**: Fokus di file manager tanpa memilih file apa pun, menekan shortcut tidak melakukan aksi apa pun dan tidak merusak sistem. | [ ] | [ ] | [ ] |
| 5 | **Fitur Undo Multi-Langkah**: Menekan `Ctrl+Shift+Z` berulang kali mengembalikan file ke lokasi asalnya satu per satu secara berurutan. | [ ] | [ ] | [ ] |
| 6 | **Konflik Nama File**: Memindahkan file yang namanya sudah ada di folder tujuan; file baru diberi penomoran otomatis (mis. `foto (1).jpg`), tidak ada file lama yang tertimpa. | [ ] | [ ] | [ ] |
| 7 | **Pemindahan Lintas-Drive/Volume**: Memindahkan file ke drive/mount point lain (mis. dari C: ke D:, atau root ke USB disk); file diverifikasi integritasnya sebelum sumber dihapus. | [ ] | [ ] | [ ] |
| 8 | **Hot-Reload Konfigurasi**: Mengubah shortcut di `config.yaml` dari `1` menjadi `x` saat aplikasi aktif; tombol `x` langsung berfungsi tanpa perlu restart KeyFlow. | [ ] | [ ] | [ ] |
| 9 | **Penanganan Konfigurasi Rusak**: Merusak sintaks `config.yaml` (mis. salah indentasi YAML); KeyFlow menampilkan notifikasi peringatan, tidak crash, dan konfigurasi valid sebelumnya tetap aktif. | [ ] | [ ] | [ ] |
| 10 | **Mode Dry Run**: Mengaktifkan `dry_run: true`; menekan shortcut memunculkan notifikasi tanpa memindahkan atau mengubah berkas di disk sama sekali. | [ ] | [ ] | [ ] |
| 11 | **Menu System Tray**: Klik kanan ikon tray dan uji menu: Pause/Resume (jeda global), Reload Config, Open Config Folder, Open Logs, dan Exit. | [ ] | [ ] | [ ] |
| 12 | **Penggunaan Sumber Daya (Idle CPU)**: Biarkan KeyFlow berjalan 5 menit saat idle; periksa Task Manager / Activity Monitor / htop. Penggunaan CPU harus mendekati 0%. | [ ] | [ ] | [ ] |
| 13 | **Aksi Trash**: Shortcut dengan `action: trash` memindahkan file ke Recycle Bin / Trash OS dan berkas dapat dipulihkan (*Put Back* / *Restore*). | [ ] | [ ] | [ ] |
| 14 | **Profil Dinonaktifkan**: Profil dengan `enabled: false` tidak pernah menelan tombol atau memproses file. | [ ] | [ ] | [ ] |
| 15 | **Prioritas Spesifisitas Profil**: Jika ada dua profil yang sama-sama cocok, profil dengan path lebih spesifik diprioritaskan. | [ ] | [ ] | [ ] |
| 16 | **Integritas Intent Log**: Periksa berkas `audit.log.jsonl`; setiap entri memiliki timestamp, ID operasi, path sumber, path tujuan, status intent, dan completion. | [ ] | [ ] | [ ] |
| 17 | **(macOS) Verifikasi Izin**: Buka KeyFlow tanpa izin Accessibility; pastikan dialog izin muncul dengan instruksi jelas dan KeyFlow menolak hook tanpa menelan tombol. | - | [ ] | - |
| 18 | **(Linux) Deteksi Sesi Wayland**: Jalankan KeyFlow di sesi Wayland (`echo $XDG_SESSION_TYPE`); pastikan muncul log peringatan dan tombol 100% fail-open. | - | - | [ ] |
| 19 | **(Windows) Banyak Tab & Jendela Explorer**: Buka beberapa jendela dan tab Explorer; KeyFlow secara tepat membaca folder dan seleksi dari tab yang sedang aktif. | [ ] | - | - |
| 20 | **GUI Pengaturan membuka & memuat**: Jalankan `keyflow settings` (atau klik "Pengaturan..." di tray Windows); jendela menampilkan nilai dari `config.yaml`. Klik tray dua kali: tidak muncul jendela kedua. | [ ] | [ ] | [ ] |
| 21 | **Simpan menjaga komentar**: Ubah `dry_run`, klik Simpan; `config.yaml` berubah hanya pada nilai itu, komentar dan profil tetap utuh. | [ ] | [ ] | [ ] |
| 22 | **Hot-reload dari GUI**: Dengan KeyFlow berjalan, simpan dari GUI; perubahan langsung berlaku (mis. `dry_run: true` membuat shortcut hanya notifikasi) tanpa restart. | [ ] | [ ] | [ ] |
| 23 | **Konfirmasi `overwrite`**: Memilih `overwrite` memunculkan dialog peringatan; "Batal" mengembalikan pilihan sebelumnya. | [ ] | [ ] | [ ] |
| 24 | **Config diubah di luar GUI**: Biarkan GUI terbuka, edit `config.yaml` di editor lain, lalu klik Simpan di GUI; muncul peringatan, isi editor tidak tertimpa. Config yang sedang rusak: GUI menampilkan error dan menolak menyimpan. | [ ] | [ ] | [ ] |
| 25 | **Penghentian Paksa (Crash Resiliency)**: Matikan proses paksa (`kill -9` / End Task) saat aksi berlangsung; pastikan tidak ada file yang hilang dan intent log mencukupi untuk audit pemulihan. | [ ] | [ ] | [ ] |

---

## 🛠️ Langkah Menjalankan Pengujian

1. **Siapkan Lingkungan**:
   Buat folder uji di komputer Anda:
   ```bash
   mkdir -p ~/keyflow_test/mentah
   mkdir -p ~/keyflow_test/01_dipakai
   # Buat beberapa file tiruan
   touch ~/keyflow_test/mentah/test_01.jpg
   touch ~/keyflow_test/mentah/test_02.jpg
   touch ~/keyflow_test/mentah/test_03.jpg
   ```
2. **Siapkan Konfigurasi Uji**:
   Arahkan profil pada `config.yaml` ke folder `~/keyflow_test/mentah/**` dengan tujuan `~/keyflow_test/01_dipakai`.
3. **Jalankan Aplikasi**:
   ```bash
   cargo run --bin keyflow
   ```
4. **Verifikasi Checklist**:
   Buka file manager Anda pada folder `~/keyflow_test/mentah` dan lakukan pengujian sesuai urutan skenario tabel di atas.

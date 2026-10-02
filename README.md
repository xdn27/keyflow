# KeyFlow

**KeyFlow** adalah aplikasi desktop lintas-platform (Windows, macOS, Linux) berbasis Rust yang berjalan di latar belakang (system tray) untuk mengeksekusi **shortcut keyboard sadar konteks** pada file manager bawaan OS.

Contoh kasus penggunaan: saat Anda menyortir ratusan foto di File Explorer / Finder / Nautilus, cukup sorot foto lalu tekan tombol `1` untuk langsung memindahkannya ke folder `01_Dipakai` dan secara otomatis memajukan seleksi ke file berikutnya (`then: select_next`). Ketika Anda berpindah ke editor teks, browser, atau folder lain di luar konteks yang ditentukan, tombol `1` akan bekerja normal tanpa ditelan atau terganggu.

KeyFlow **bukan** file manager tersendiri dan tidak memiliki jendela penjelajah file grafis; KeyFlow memberdayakan file manager bawaan yang sudah Anda gunakan sehari-hari.

---

## ⚡ Fitur Utama

- **Sadar Konteks Berlapis (3-Layer Context Matching)**:
  - **Aplikasi**: Hanya aktif di file manager target (Windows Explorer, macOS Finder, Linux Nautilus/Dolphin/Thunar/Nemo/Caja/PCManFM).
  - **Path Folder**: Mendukung pencocokan path eksplisit maupun pola glob (misalnya `D:/Foto/Mentah/**` atau `~/Pictures/Camera/**`).
  - **Kriteria Seleksi**: Saring berdasarkan jenis file (`image`, `video`, `audio`, `document`, atau ekstensi spesifik `ext:[jpg,png,raw]`).
- **Prinsip Keamanan Data Mutlak (Zero Data Loss)**:
  - **Tidak ada hapus permanen**: Aksi `trash` selalu memindahkan berkas ke Recycle Bin / Trash OS.
  - **Proteksi Penimpaan**: Kebijakan `on_conflict` bawaan adalah `rename` otomatis (mis. `foto (1).jpg`), bukan menimpa. Opsi `skip`, `ask`, dan `overwrite` (hanya jika eksplisit) juga didukung.
  - **Audit Intent Log**: Niat setiap transaksi file dicatat dan di-flush ke disk (`audit.log.jsonl`) *sebelum* berkas fisik dimodifikasi, dan dicatat kembali setelah selesai.
  - **Undo Berlapis**: Riwayat aksi tersimpan secara persisten dan dapat dibatalkan kapan saja dengan shortcut undo (`Ctrl+Shift+Z`).
  - **Pencegahan Recursive Loops**: Mencegah pemindahan folder ke dalam subdirektori dirinya sendiri.
  - **Penanganan Lintas-Drive/Volume**: Pada pemindahan lintas-drive, berkas disalin terlebih dahulu, ukuran diverifikasi, lalu sumber dihapus secara aman.
- **Fail-Open Guarantee**:
  - Tombol keyboard **hanya ditelan** jika jendela aktif adalah file manager yang valid, seleksi berkas tidak kosong, dan aturan profil cocok.
  - Dalam kondisi ragu, jendela non-aktif, atau seleksi kosong, tombol selalu diteruskan langsung ke sistem operasi.
- **Hot-Reload Konfigurasi**:
  - Setiap perubahan pada berkas konfigurasi YAML langsung diterapkan secara otomatis tanpa perlu restart aplikasi.
  - Jika sintaks konfigurasi baru salah atau rusak, KeyFlow tetap menggunakan konfigurasi valid terakhir dan memunculkan notifikasi peringatan.
- **System Tray Ringan & Non-Intrusif**:
  - Penggunaan CPU idle mendekati 0%.
  - Menu tray untuk mengaktifkan/menonaktifkan shortcut secara global, me-reload konfigurasi secara manual, membuka folder konfigurasi, membuka berkas log, dan keluar dengan bersih.
- **Dukungan Multi-Platform Nyata**:
  - **Windows**: Menggunakan low-level keyboard hook (`WH_KEYBOARD_LL`) dan integrasi COM Shell (`IShellWindows`/`IFolderView2`).
  - **macOS**: Menggunakan `CGEventTap` dengan pemulihan otomatis timeout, integrasi AppleScript Finder, dan verifikasi izin Accessibility & Input Monitoring.
  - **Linux (X11)**: Menggunakan synchronous X11 key grab dengan auto-replay (`Allow::REPLAY_KEYBOARD`), ekstraksi metadata window `_NET_ACTIVE_WINDOW` & `WM_CLASS`, fallback clipboard `text/uri-list`, dan deteksi aman sesi Wayland.

---

## 🖥️ Matriks Dukungan Platform

| Kemampuan | Windows | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| **Hook Keyboard Global** | `WH_KEYBOARD_LL` (Win32) | `CGEventTap` (CoreGraphics) | `XGrabKey` (Sync, x11rb) | Dinonaktifkan (Fail-Open) |
| **Integrasi File Manager** | File Explorer (COM) | Finder (AppleScript) | Nautilus, Dolphin, Thunar, Nemo, Caja, PCManFM | - |
| **Navigasi Otomatis** | `SendInput` Down Arrow | `CGEvent` Down Arrow | XTest Fake Input Down Arrow | - |
| **Status Izin** | Tanpa izin khusus (Admin opsional) | Butuh Accessibility & Input Monitoring | Display X11 standar | Peringatan visual |

> [!NOTE]
> Untuk detail arsitektur per platform dan batasan teknis, baca [docs/PLATFORM_SUPPORT.md](docs/PLATFORM_SUPPORT.md) dan panduan izin di [docs/PERMISSIONS.md](docs/PERMISSIONS.md).

---

## 📦 Pemasangan & Kompilasi

### Prasyarat

- **Rust Toolchain**: Rust versi stable (1.80+ direkomendasikan).
- **Linux**: Membutuhkan pustaka pengembangan X11 dan XTest:
  ```bash
  # Debian / Ubuntu / Pop!_OS
  sudo apt-get install -y libx11-dev libxtst-dev libxext-dev
  
  # Fedora / RHEL
  sudo dnf install -y libX11-devel libXtst-devel libXext-devel
  
  # Arch Linux
  sudo pacman -S --needed libx11 libxtst libxext
  ```

### Kompilasi dari Sumber

Clone repositori dan kompilasi versi rilis:

```bash
git clone https://github.com/d4n/shortcut-file-manager.git
cd shortcut-file-manager

# Jalankan test suite
cargo test --workspace

# Kompilasi binary rilis
cargo build --release --bin keyflow
```

Executable biner akan tersedia di:
- Linux / macOS: `target/release/keyflow`
- Windows: `target/release/keyflow.exe`

---

## ⚙️ Konfigurasi

KeyFlow membaca berkas konfigurasi `config.yaml` pada direktori konfigurasi standar pengguna:
- **Windows**: `%APPDATA%\keyflow\config.yaml`
- **macOS**: `~/Library/Application Support/keyflow/config.yaml`
- **Linux**: `~/.config/keyflow/config.yaml`

Contoh konfigurasi lengkap:

```yaml
version: 1

settings:
  dry_run: false            # true = uji coba tanpa menyentuh berkas fisik
  notifications: true       # tampilkan notifikasi desktop setiap aksi
  on_conflict: rename       # rename | skip | overwrite | ask (bawaan: rename aman)
  create_missing_dirs: true # buat direktori tujuan otomatis jika belum ada
  undo_history_limit: 200   # batas jumlah aksi undo yang dicatat

profiles:
  - name: "Sorting Foto Kamera"
    enabled: true
    context:
      app: file_manager
      path: "D:/Foto/Mentah/**"  # di Linux/macOS sesuaikan path, mis: "~/Pictures/Raw/**"
      selection: image           # any | image | video | audio | document | ext:[jpg,png,cr3]
    rules:
      - key: "1"
        action: move
        to: "D:/Foto/01_Dipakai"
        then: select_next
      - key: "2"
        action: move
        to: "D:/Foto/02_Cadangan"
        then: select_next
      - key: "3"
        action: copy
        to: "D:/Foto/03_Posting"
      - key: "4"
        action: trash
        then: select_next
      - key: "Ctrl+Shift+Z"
        action: undo
```

> [!TIP]
> Lihat dokumentasi lengkap format dan skema konfigurasi di [docs/agents/config.md](docs/agents/config.md) atau contoh di [examples/config.yaml](examples/config.yaml).

---

## 🧪 Pengujian & Kontribusi

Jalankan rangkaian linter dan pengujian:

```bash
# Verifikasi pemformatan kode
cargo fmt --all -- --check

# Linter statis ketat
cargo clippy --workspace --all-targets -- -D warnings

# Jalankan seluruh unit dan integration test
cargo test --workspace
```

Bagi pengembang atau kontributor AI, silakan pelajari peta dokumentasi arsitektur di [AGENTS.md](AGENTS.md) dan panduan uji manual di [docs/MANUAL_TESTING.md](docs/MANUAL_TESTING.md).

---

## 📄 Lisensi

Proyek ini dilisensikan di bawah ketentuan lisensi MIT / Apache-2.0.

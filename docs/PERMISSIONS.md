# Izin & Akses Sistem

KeyFlow memantau event keyboard secara global dan berinteraksi dengan file manager sistem operasi. Dokumen ini menjelaskan izin dan hak akses yang dibutuhkan pada setiap platform.

KeyFlow mematuhi prinsip privasi ketat: **tidak ada koneksi jaringan, tidak ada pengumpulan analitik, dan tidak ada telemetri**.

---

## 🪟 Windows

- **Pengguna Standar (Non-Elevated)**:
  - KeyFlow tidak memerlukan izin khusus atau hak Administrator untuk berjalan secara normal.
  - Low-level keyboard hook (`WH_KEYBOARD_LL`) dipasang pada sesi pengguna saat ini.
- **Interaksi dengan Jendela Administrator (Elevated)**:
  - Berdasarkan kebijakan keamanan UIPI (User Interface Privilege Isolation) Windows, proses non-elevated tidak dapat menyadap atau menginjeksi input (`SendInput`) ke aplikasi yang berjalan sebagai Administrator.
  - Jika Anda membuka File Explorer sebagai Administrator dan ingin shortcut KeyFlow berfungsi di jendela tersebut, KeyFlow juga harus dijalankan dengan hak Administrator (*Run as Administrator*).
- **Antivirus & SmartScreen**:
  - Pemasangan global keyboard hook kadang memicu deteksi heuristik antivirus pihak ketiga. Menandatangani biner rilis dengan sertifikat digital atau menambahkan pengecualian lokal dapat mengatasi hal ini.

---

## 🍎 macOS

macOS memberlakukan sistem perizinan keamanan yang ketat. KeyFlow memerlukan izin berikut yang dapat diatur di **System Settings -> Privacy & Security**:

1. **Accessibility (Aksesibilitas)**:
   - Dibutuhkan untuk memantau status aplikasi, fokus jendela, serta mengirimkan event sintetis navigasi (`select_next` via `CGEvent`).
   - KeyFlow memeriksa status izin ini saat awal dijalankan menggunakan `AXIsProcessTrusted()`. Jika izin belum aktif, KeyFlow memanggil `AXIsProcessTrustedWithOptions` untuk memicu dialog prompt permohonan izin macOS secara otomatis.
2. **Input Monitoring (Pemantauan Input)**:
   - Dibutuhkan untuk menangkap dan menyaring ketukan keyboard tingkat rendah secara global melalui `CGEventTap`.
   - Pastikan toggle untuk KeyFlow (atau Terminal tempat Anda menjalankannya) berada dalam posisi **Aktif (Enabled)**.
3. **Automation (Finder)**:
   - Saat pertama kali KeyFlow membaca folder aktif atau daftar berkas yang disorot melalui AppleScript (`osascript`), macOS akan menampilkan kotak dialog sistem: *"KeyFlow would like to control Finder"*.
   - Pilih **OK / Allow** agar KeyFlow diizinkan membaca jalur folder dan seleksi Finder.

> [!IMPORTANT]
> Jika izin Accessibility atau Input Monitoring ditolak atau dicabut, KeyFlow menolak aktif dan **tidak akan menelan tombol apa pun** (fail-safe penuh) demi mencegah gangguan pada pengetikan pengguna.

---

## 🐧 Linux

### Sesi X11 (X.Org)

- **Izin Standar**:
  - KeyFlow berjalan sebagai proses pengguna biasa dan **tidak memerlukan akses root/sudo**.
  - KeyFlow berkomunikasi langsung dengan X Server lokal melalui soket `$DISPLAY` (`/tmp/.X11-unix/X0`).
- **XTest Extension**:
  - Fitur navigasi otomatis (`then: select_next`) dan fallback seleksi clipboard menggunakan ekstensi X11 XTest (`libXtst`).
  - Pada hampir seluruh distribusi Linux desktop modern (Ubuntu, Fedora, Debian, Arch, Manjaro, Linux Mint), ekstensi XTest telah diaktifkan secara bawaan di X Server.
- **Batasan Akses**:
  - Jika X Server dijalankan dengan isolasi khusus (misalnya container Docker atau sandbox Flatpak tanpa izin `--share=ipc` dan `--socket=x11`), KeyFlow tidak dapat membaca window tree atau memasang key grab.

### Sesi Wayland

- **Kebijakan Keamanan Wayland**:
  - Desain keamanan Wayland sengaja mengisolasi antar-klien: aplikasi biasa dilarang memantau ketukan tombol aplikasi lain atau membaca konten jendela lain secara global tanpa protokol compositor khusus berizin istimewa.
- **Perilaku KeyFlow di Wayland**:
  - KeyFlow secara aktif memeriksa variabel lingkungan `XDG_SESSION_TYPE=wayland` dan `WAYLAND_DISPLAY`.
  - Jika sesi Wayland terdeteksi, KeyFlow **tidak memasang keyboard grab sama sekali**.
  - Aplikasi memunculkan notifikasi peringatan dan mencatat log bahwa sesi Wayland terdeteksi, serta beralih ke mode **fail-open murni** (0% tombol ditelan).

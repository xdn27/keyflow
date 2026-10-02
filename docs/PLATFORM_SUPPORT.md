# Matriks Dukungan Platform

Dokumen ini menjelaskan status kemampuan, batasan teknis, dan file manager yang didukung oleh KeyFlow pada setiap sistem operasi.

---

## 📊 Matriks Kemampuan OS

| Fitur / Kemampuan | Windows | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| **Global Key Hook** | `WH_KEYBOARD_LL` (Win32 API) | `CGEventTap` (CoreGraphics) | `XGrabKey` (Sync, x11rb) | Dinonaktifkan (Fail-open) |
| **Penelanan Tombol (Swallow)** | Sangat Cepat (< 1ms) | Sangat Cepat (< 1ms) | Sinkron dengan Auto-Replay | Tidak (0 tombol ditelan) |
| **Deteksi Jendela Fokus** | `GetForegroundWindow` | AppleScript Finder frontmost | `_NET_ACTIVE_WINDOW` & `WM_CLASS` | Terbatas |
| **Deteksi Folder Aktif** | COM Shell (`IShellWindows`) | AppleScript (`target of Finder window`) | Ekstraksi path dari WM Title / Fallback | Tidak didukung |
| **Deteksi Berkas Terpilih** | COM (`IFolderView2`) | AppleScript (`selection of Finder window`) | Fallback Clipboard (`text/uri-list`) | Tidak didukung |
| **Navigasi Berkas (`select_next`)** | `SendInput` (Down Arrow) | `CGEvent` (Down Arrow) | XTest Fake Input (Keycode 116) | Tidak didukung |
| **Proteksi Event Sintetis** | `KEYFLOW_EXTRA_INFO` flag | `KEYFLOW_MACOS_USER_DATA` tag | Tagging / Timing Separation | - |
| **Notifikasi Desktop** | Windows Toast (`notify-rust`) | AppleScript `display notification` | FreeDesktop `notify-send` | FreeDesktop `notify-send` |
| **System Tray** | Tray Icon Win32 | Status Bar Menu | AppIndicator / FreeDesktop Tray | AppIndicator / FreeDesktop Tray |

---

## 📁 File Manager yang Didukung

### 1. Windows
- **File Explorer bawaan**:
  - Windows 10 & Windows 11 (termasuk Explorer modern berbasis tab).
  - Menggunakan API COM resmi `IShellWindows`, `IShellBrowser`, dan `IFolderView2` yang diakses dari background thread STA (Single-Threaded Apartment) khusus.

### 2. macOS
- **Finder bawaan Apple**:
  - macOS Monterey (12), Ventura (13), Sonoma (14), dan Sequoia (15).
  - Menggunakan skrip AppleScript berkecepatan tinggi yang dieksekusi melalui `osascript` dengan mekanisme snapshot cache untuk meminimalkan beban CPU.

### 3. Linux (X11)
- **GNOME Files / Nautilus** (`nautilus`, class `org.gnome.Nautilus`)
- **KDE Dolphin** (`dolphin`, class `org.kde.dolphin`)
- **XFCE Thunar** (`thunar`, class `Thunar`)
- **Cinnamon Nemo** (`nemo`, class `Nemo`)
- **PCManFM / PCManFM-Qt** (`pcmanfm`)
- **MATE Caja** (`caja`, class `Caja`)

---

## ⚠️ Kebijakan Lingkungan Wayland

Wayland dirancang dengan model isolasi keamanan yang memisahkan jendela setiap klien. Klien standar tidak diperbolehkan secara global menyadap keyboard atau memantau jendela aplikasi lain tanpa hak istimewa protokol compositor khusus.

**Sikap KeyFlow**:
1. KeyFlow secara jujur **tidak mengklaim mendukung Wayland secara penuh**.
2. Saat aplikasi berjalan, KeyFlow memeriksa apakah sesi desktop adalah Wayland (melalui variabel `XDG_SESSION_TYPE=wayland` atau `WAYLAND_DISPLAY`).
3. Jika Wayland terdeteksi, KeyFlow sengaja **tidak memasang hook keyboard sama sekali**.
4. Log peringatan dicatat dan notifikasi sistem ditampilkan untuk menginformasikan bahwa shortcut dinonaktifkan.
5. Mode ini menjamin **100% fail-open**: tidak ada ketikan tombol pengguna yang ditelan atau diinterupsi saat mengetik di aplikasi apa pun.

---

## 🔮 Rencana Pasca-MVP

Setelah fase MVP stabil, integrasi berikut direncanakan:
- Dukungan file manager pihak ketiga: Total Commander, Double Commander, Directory Opus, One Commander.
- Integrasi Wayland portal (via `xdg-desktop-portal` GlobalShortcuts bila adopsi compositor mencukupi).
- GUI konfigurasi visual dan wizard pengaturan shortcut.

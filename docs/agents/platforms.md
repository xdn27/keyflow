# Detail per platform

Urutan pengerjaan: Windows (M0-M2) -> macOS (M3) -> Linux X11 (M4). Jelaskan keterbatasan platform dengan jujur dan jangan disembunyikan.

## Windows (target utama)

- **Hook**: low-level keyboard hook `WH_KEYBOARD_LL` lewat crate `windows`. Return nilai non-zero untuk menelan tombol, `CallNextHookEx` untuk meneruskan. Hook butuh message loop di threadnya.
- **Window fokus**: `GetForegroundWindow` + nama proses (atau `active-win-pos-rs`). Kenali Explorer: `explorer.exe` dengan class window `CabinetWClass`.
- **Folder aktif dan seleksi**: COM `IShellWindows` -> cocokkan HWND -> `IShellBrowser`/`IFolderView2` -> `GetCurrentFolder` / `Items(SVGIO_SELECTION)`. Inisialisasi COM (STA) di thread yang memakainya.
- **Fallback bila COM gagal**: simulasikan `Ctrl+C`, baca daftar file dari clipboard (`CF_HDROP`), lalu **pulihkan isi clipboard sebelumnya**. Hati-hati: ini mengubah clipboard sesaat dan bisa bentrok dengan tombol yang masih ditekan pengguna.
- **`select_next`**: kirim tombol Down lewat `SendInput` (atau `enigo`). Opsional, karena Explorer sering sudah memajukan seleksi sendiri setelah file pindah. Tandai event buatan sendiri agar hook mengabaikannya.
- Catatan: aplikasi yang berjalan sebagai Administrator tidak menerima input dari proses non-elevated dan sebaliknya; dokumentasikan.

## macOS

- **Hook**: `CGEventTap` (crate `core-graphics`, atau `rdev` dengan fitur grab). Butuh izin **Accessibility** dan **Input Monitoring**. Deteksi izin yang belum diberikan dan arahkan pengguna ke System Settings dengan pesan jelas (dokumentasikan di `docs/PERMISSIONS.md`). Tap bisa dinonaktifkan sistem jika callback lambat; tangani event `tapDisabledByTimeout` dengan mengaktifkan ulang.
- **Window fokus**: `NSWorkspace.frontmostApplication` (`objc2` / `active-win-pos-rs`). Kenali `com.apple.finder`.
- **Folder aktif dan seleksi**: AppleScript lewat `osascript` (`tell application "Finder" to get selection`, `target of front window`). Lambat: cache hasil dan batasi frekuensi pemanggilan; jangan panggil dari callback hook.
- **Trash**: crate `trash`.

## Linux

- **Prioritas X11**. Hook lewat `rdev` (grab) atau X11 key grab. Window fokus lewat `_NET_ACTIVE_WINDOW`.
- **Wayland**: pembatasan keamanan membuat global key grab dan deteksi window fokus tidak tersedia secara standar. **Jangan berpura-pura mendukung.** Deteksi sesi Wayland (`XDG_SESSION_TYPE`, `WAYLAND_DISPLAY`), tampilkan peringatan jelas tentang keterbatasan, jangan menelan tombol apa pun, dan catat sebagai dukungan terbatas di `docs/PLATFORM_SUPPORT.md`.
- **Folder aktif dan seleksi**: tidak ada API universal. Strategi bertahap:
  1. Fallback clipboard (`Ctrl+C` + baca `text/uri-list`) dan judul window/path.
  2. Integrasi spesifik per file manager (Nautilus, Dolphin, Thunar) di fase berikutnya.

## Matriks dukungan (target akhir MVP)

| Kemampuan | Windows | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| Hook + telan tombol | Ya | Ya (izin) | Ya | Tidak (standar) |
| Window fokus | Ya | Ya | Ya | Terbatas |
| Folder aktif | COM | AppleScript | Clipboard/judul | Tidak |
| Seleksi | COM / clipboard | AppleScript | Clipboard | Tidak |
| `select_next` | Best-effort | Best-effort | Best-effort | Tidak |

Perbarui matriks ini bila implementasi berubah.

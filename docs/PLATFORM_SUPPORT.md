# Dukungan platform

| Kemampuan | Windows | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| Hook + telan tombol | Ya | Ya (butuh izin) | Ya | Tidak (standar) |
| Window fokus | Ya | Ya | Ya | Terbatas |
| Folder aktif | COM Shell | AppleScript | Clipboard/judul window | Tidak |
| Seleksi | COM / clipboard | AppleScript | Clipboard | Tidak |
| `select_next` | Best-effort | Best-effort | Best-effort | Tidak |

Status per milestone: Windows (Selesai M0-M2), macOS (Selesai M3: CGEventTap, Finder AppleScript, verifikasi izin), Linux X11 (M4). Matriks di atas mencerminkan fitur yang telah diimplementasikan di codebase.

## Wayland

Wayland sengaja membatasi global key grab dan deteksi window fokus demi keamanan. KeyFlow tidak berpura-pura mendukungnya: pada sesi Wayland, KeyFlow menampilkan peringatan, **tidak menelan tombol apa pun**, dan fitur dinonaktifkan. Gunakan sesi X11 atau XWayland-only bila memungkinkan.

## File manager yang didukung (MVP)

- Windows: File Explorer (`explorer.exe`, class `CabinetWClass`)
- macOS: Finder (`com.apple.finder`)
- Linux: terbatas (strategi clipboard); integrasi Nautilus/Dolphin/Thunar menyusul

File manager pihak ketiga (Total Commander, Directory Opus, dll.) direncanakan setelah MVP.

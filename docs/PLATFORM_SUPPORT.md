# Dukungan platform

| Kemampuan | Windows | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| Hook + telan tombol | Ya | Ya (butuh izin) | Ya | Tidak (standar) |
| Window fokus | Ya | Ya | Ya | Terbatas |
| Folder aktif | COM Shell | AppleScript | Clipboard/judul window | Tidak |
| Seleksi | COM / clipboard | AppleScript | Clipboard | Tidak |
| `select_next` | Best-effort | Best-effort | Best-effort | Tidak |

Status per milestone: Windows (Selesai M0-M2), macOS (Selesai M3: CGEventTap, Finder AppleScript, verifikasi izin), Linux X11 (Selesai M4: X11 sync grab dengan fail-open replay, deteksi `_NET_ACTIVE_WINDOW` & `WM_CLASS`, fallback clipboard `text/uri-list`, XTest fake_input, dan deteksi Wayland).

## Wayland

Wayland sengaja membatasi global key grab dan deteksi window fokus lintas-aplikasi demi keamanan. KeyFlow tidak berpura-pura mendukungnya: pada sesi Wayland, KeyFlow mendeteksi `XDG_SESSION_TYPE=wayland` atau `WAYLAND_DISPLAY`, menampilkan peringatan visual/log yang jelas, dan beroperasi dalam mode **fail-open** (tombol diteruskan ke sistem tanpa dimodifikasi atau ditelan).

## File manager yang didukung (MVP)

- **Windows**: File Explorer (`explorer.exe`, class `CabinetWClass`)
- **macOS**: Finder (`com.apple.finder`)
- **Linux X11**:
  - GNOME Files (Nautilus, `org.gnome.Nautilus` / `nautilus`)
  - KDE Dolphin (`org.kde.dolphin` / `dolphin`)
  - XFCE Thunar (`Thunar` / `thunar`)
  - Cinnamon Nemo (`nemo`)
  - PCManFM (`pcmanfm`) & MATE Caja (`caja`)

File manager pihak ketiga (Total Commander, Directory Opus, dll.) direncanakan setelah MVP.

# Temuan spike M0 (Windows)

> Diisi oleh agent/pengembang yang mengerjakan M0. Jangan lanjut ke M1 sebelum tiga bukti di bawah terisi dengan hasil nyata.

## (a) Hook menelan tombol `1` hanya saat Explorer fokus

- Status: belum dikerjakan
- Cara pembuktian:
- Latensi callback terukur (min/median/maks):
- Catatan (timeout hook, message loop, event buatan sendiri):

## (b) Folder aktif dan file terpilih via COM

- Status: belum dikerjakan
- Pendekatan (IShellWindows -> HWND -> IShellBrowser/IFolderView2):
- Kasus yang diuji (tab, banyak jendela, folder khusus/Search/This PC, tanpa seleksi):
- Latensi pembacaan:
- Perlu fallback clipboard? Mengapa:

## (c) Memindahkan file terpilih ke satu folder

- Status: belum dikerjakan
- Hasil (termasuk file terkunci, lintas-drive):

## Keputusan desain yang muncul

-

## Risiko / hal yang mengubah rencana M1-M2

-

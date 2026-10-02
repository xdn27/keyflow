# Konfigurasi

Format **YAML**. Lokasi: direktori config standar OS lewat crate `directories`, mis. `keyflow/config.yaml`. Contoh lengkap nanti ada di `examples/config.yaml`.

```yaml
version: 1
settings:
  dry_run: false
  notifications: true
  on_conflict: rename   # rename | skip | overwrite | ask
  create_missing_dirs: true
  undo_history_limit: 200

profiles:
  - name: Sorting Foto
    enabled: true
    context:
      app: file_manager          # file_manager | nama proses spesifik
      path: "D:/Foto/Mentah/**"  # glob, opsional
      selection: image           # any | image | video | audio | document | ext:[jpg,png]
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
        action: move
        to: "D:/Foto/04_Reject"
        then: select_next
      - key: "Ctrl+Shift+Z"
        action: undo
```

## Validasi saat load

- Tombol valid (parser kombinasi tombol, mis. `Ctrl+Shift+Z`).
- Folder tujuan valid (dan bukan path sistem berbahaya, lihat `safety.md`).
- Aksi dikenal; field wajib per aksi ada (`move`/`copy` butuh `to`).
- Tidak ada duplikasi tombol dalam satu profil.
- Pola glob valid.

Error harus berupa **pesan jelas dengan nomor baris (dan kolom bila ada)**, mis. `config.yaml:14: aksi "mov" tidak dikenal (maksud Anda "move"?)`. Kumpulkan semua error sekaligus, jangan berhenti di yang pertama.

## Config rusak tidak boleh crash

- Saat startup tanpa config: buat default aman (tanpa rule yang menelan tombol) dan beri tahu pengguna.
- Saat hot-reload gagal: **tetap pakai config valid terakhir**, kirim notifikasi berisi ringkasan error, tulis detail ke log.
- Hot-reload memakai crate `notify` dengan debounce (editor sering menulis beberapa kali). Ganti config secara atomik (swap `Arc`), jangan memodifikasi in-place.

## Catatan desain

- Field `version` wajib; tolak versi yang tidak dikenal dengan pesan jelas.
- Gunakan `#[serde(deny_unknown_fields)]` agar salah ketik pada nama field terdeteksi.
- Path di config boleh memakai `/` di semua OS; normalisasi di `keyflow-core`.
- Parsing, validasi, dan model config hidup di `keyflow-core` dan tidak boleh bergantung pada OS.

# KeyFlow

Aplikasi desktop Rust (system tray) yang menjalankan **shortcut keyboard sadar konteks** untuk file manager bawaan OS. Contoh: di Explorer, menyorot foto lalu menekan `1` memindahkan file ke `01_Dipakai`. Di luar konteks yang cocok, tombol berfungsi normal. KeyFlow **bukan** file manager dan tidak punya GUI browsing sendiri.

> Nama "KeyFlow" bersifat sementara. File ini dibaca semua agent (Claude Code, Qwen, Gemini CLI, Codex, OpenCode, Amp). `CLAUDE.md`, `GEMINI.md`, dan `QWEN.md` adalah symlink ke file ini. Edit hanya `AGENTS.md`.

## Status
 
Seluruh Milestone MVP: **M0** (Spike Windows), **M1** (Inti `keyflow-core`), **M2** (Aplikasi Windows lengkap), **M3** (Dukungan macOS), **M4** (Dukungan Linux X11 & Wayland fail-open), dan **M5** (Pengemasan CI/CD multi-platform, workflow rilis GitHub Actions, dokumentasi final, serta audit adversarial keselamatan data) telah **SELESAI 100%**. Seluruh test (24 unit/integration test) lolos, linter clippy 0 warning, dan kompilasi multi-target (Windows, macOS Intel/Silicon, Linux) terverifikasi hijau. Proyek siap untuk rilis versi v0.1.0.

Pasca-MVP: **GUI Pengaturan tahap 1** (blok `settings:` global lewat `keyflow settings`, egui, proses terpisah) selesai. Implementasi dan test otomatis lolos; uji manual per OS (baris 20-24 di `docs/MANUAL_TESTING.md`) belum dijalankan. Tahap 2 (editor profil/rule) belum dikerjakan.

## Perintah

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Sebuah perubahan dianggap selesai hanya jika keempat perintah di atas lolos. Pada Linux/macOS, kode `windows` tidak boleh ikut terkompilasi (gunakan `#[cfg(target_os = "...")]`).

## Peta dokumen (baca sesuai kebutuhan)

| Dokumen | Isi |
|---|---|
| `docs/agents/product.md` | Tujuan, alur eksekusi tombol, sistem konteks, fitur MVP vs pasca-MVP |
| `docs/agents/config.md` | Format YAML, validasi, perilaku saat config rusak |
| `docs/agents/architecture.md` | Workspace, trait, modul, aturan performa hook, daftar crate |
| `docs/agents/platforms.md` | Detail Windows, macOS, Linux (X11/Wayland) dan keterbatasannya |
| `docs/agents/safety.md` | Aturan keamanan data (WAJIB dibaca sebelum menyentuh kode file/undo) |
| `docs/agents/testing.md` | Strategi uji, mock platform, checklist uji manual |
| `docs/agents/milestones.md` | M0-M5, kriteria selesai MVP, cara kerja per milestone |
| `docs/agents/plan.md` | **Mulai di sini**: daftar tugas bernomor per milestone + prompt awal |

## Aturan yang tidak boleh dilanggar

1. **Tombol hanya boleh ditelan** jika konteks cocok dan rule benar-benar dijalankan. Di luar itu, selalu teruskan.
2. **Tidak ada hapus permanen.** Hapus selalu lewat Recycle Bin/Trash.
3. **Tidak pernah menimpa file** tanpa konfirmasi eksplisit. Default `on_conflict: rename`.
4. Log ditulis **sebelum dan sesudah** setiap `move`/`copy`/`rename`.
5. Callback hook keyboard **sinkron dan cepat** (milidetik): tanpa I/O, tanpa COM, tanpa lock panjang. Kerja berat di worker thread.
6. Config rusak **tidak boleh** membuat aplikasi crash: pakai config valid terakhir dan kirim notifikasi.
7. Tidak ada akses jaringan dan tidak ada telemetri.
8. Jangan berpura-pura mendukung sesuatu yang tidak bisa (Wayland, izin macOS). Jelaskan keterbatasannya.

## Konvensi kode

- Rust edition 2021+, idiomatik. Panduan: skill `rust-best-practices`.
- `thiserror` untuk error di library crate, `anyhow` hanya di `keyflow-app`.
- Tidak ada `unwrap()`/`expect()` di jalur produksi (boleh di test). Tidak ada `panic!` di dalam hook callback.
- Kode `unsafe` (Win32/COM/FFI) diisolasi di `keyflow-platform`, dibungkus API aman, dan setiap blok `unsafe` diberi komentar `// SAFETY:`.
- Logging dengan `tracing`, bukan `println!`.
- Logika inti (`keyflow-core`) tidak boleh bergantung pada kode OS. Semua akses OS lewat trait.
- Verifikasi versi crate terbaru sebelum menambah dependensi. Jika memilih alternatif dari daftar yang disarankan, catat alasannya singkat di PR/commit.
- Komentar dan dokumentasi ditulis dalam Bahasa Indonesia; identifier kode dalam bahasa Inggris.

## Cara bekerja

- Kerjakan satu milestone sekali. Berhenti di akhir milestone dan beri ringkasan: apa yang selesai, apa yang diuji, apa yang belum.
- Pada keputusan desain yang ambigu: pilih opsi paling aman, jelaskan singkat, lanjut. Jika berdampak pada keamanan data, tanya dulu.
- Jangan mengerjakan fitur pasca-MVP, tetapi jangan membuat desain yang menghalanginya.
- Gunakan subagent di `.agents/agents/` untuk review, penulisan test, eksplorasi, dan audit keamanan data.

## Struktur direktori

```
AGENTS.md                 # sumber kebenaran (CLAUDE/GEMINI/QWEN.md = symlink)
.agents/skills/           # skill (sumber); .claude/.qwen/.gemini/skills = symlink
.agents/agents/           # definisi subagent (sumber); .claude/.qwen/.gemini/agents = symlink
docs/agents/              # dokumentasi detail untuk agent
crates/                   # keyflow-core, keyflow-platform, keyflow-app (stub)
examples/config.yaml      # contoh config
docs/                     # PERMISSIONS, PLATFORM_SUPPORT, MANUAL_TESTING, spikes/
```

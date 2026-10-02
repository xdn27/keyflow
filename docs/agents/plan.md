# Rencana eksekusi (handoff untuk agent pelaksana)

Dokumen ini adalah titik awal untuk agent yang mengerjakan KeyFlow. Baca berurutan: `AGENTS.md` -> dokumen ini -> dokumen `docs/agents/*` sesuai tugas.

## Kondisi awal repo

Sudah ada: workspace Cargo (3 crate, kosong/stub), trait platform di `keyflow-platform/src/lib.rs`, konfigurasi lint (`Cargo.toml`, `clippy.toml`, `rustfmt.toml`), CI (`.github/workflows/ci.yml`), `examples/config.yaml`, dokumen, subagent.

**Belum ada dan belum pernah dikompilasi**: skeleton ditulis tanpa toolchain Rust. Langkah pertama WAJIB memverifikasinya (lihat T0.0). Belum ada dependensi di `Cargo.toml` mana pun; tambahkan dengan `cargo add` (versi terbaru) saat dibutuhkan.

## Aturan eksekusi

- Satu milestone sekali; berhenti di akhir dan beri ringkasan (selesai / diuji / belum).
- Setiap tugas selesai hanya jika `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, dan `cargo test --workspace` lolos. Pengembangan Windows dikerjakan di mesin Windows; jangan klaim "berjalan" untuk hal yang belum dijalankan.
- Keputusan ambigu: opsi paling aman + catatan singkat. Menyangkut keamanan data atau penelanan tombol: tanya pengguna dulu.
- Fitur pasca-MVP jangan dikerjakan.
- Sebelum merge perubahan di `actions`/`undo`/log/logika swallow: jalankan subagent `data-safety-auditor`. Di akhir tiap milestone: `code-reviewer`.
- Perbarui `AGENTS.md` (bagian Status) dan matriks di `platforms.md`/`PLATFORM_SUPPORT.md` bila kenyataan berubah.

## M0 - Spike teknis (Windows)

Tujuan: membuktikan kelayakan. Boleh kasar, tetapi simpan di `crates/keyflow-platform/src/windows/` atau `examples/` dan beri label sementara. Hasil dicatat di `docs/spikes/m0-findings.md`.

- [x] **T0.0** Pasang toolchain Rust, jalankan `cargo build --workspace`, `clippy`, `fmt`, `test`. Perbaiki skeleton bila ada galat. Commit sebagai baseline.
- [x] **T0.1** Hook `WH_KEYBOARD_LL` (crate `windows`) dengan message loop. Telan tombol `1` **hanya** saat jendela fokus adalah Explorer (`explorer.exe`, class `CabinetWClass`); di aplikasi lain tombol lolos. Ukur latensi callback.
- [x] **T0.2** Baca folder aktif dan file terpilih dari Explorer via COM (`IShellWindows` -> cocokkan HWND -> `IShellBrowser`/`IFolderView2`), di thread STA khusus. Uji: banyak jendela/tab, tanpa seleksi, folder khusus.
- [x] **T0.3** Pindahkan file terpilih ke satu folder tujuan (boleh `std::fs::rename`; versi aman penuh dikerjakan di M1).
- [x] **T0.4** Lengkapi `docs/spikes/m0-findings.md`: latensi, kendala, kebutuhan fallback clipboard, keputusan desain.

**Selesai M0 bila**: ketiga bukti (T0.1-T0.3) berjalan nyata di Windows. Berhenti dan laporkan; tunggu persetujuan sebelum M1.

## M1 - Inti (`keyflow-core`, tanpa OS)

Dependensi kandidat: `serde`, `serde_yaml` (periksa status; alternatif `serde_yml`/`serde_norway`), `globset`, `thiserror`, `directories`, `notify`, `trash`, `tracing`; dev: `tempfile`, `assert_fs`.

- [x] **T1.1** Model config + parser YAML dengan `deny_unknown_fields`, `version`, pesan error dengan nomor baris. (`config.md`)
- [x] **T1.2** Validasi: tombol, aksi + field wajib, tujuan, duplikasi tombol per profil, glob, path berbahaya. Kumpulkan semua error.
- [x] **T1.3** Parser kombinasi tombol (`Ctrl+Shift+Z`, `1`).
- [x] **T1.4** Matcher: konteks 3 lapis, tabel rule terkompilasi (tombol -> kandidat), spesifisitas (lokasi tanpa wildcard > wildcard > tanpa lokasi), profil disabled.
- [x] **T1.5** Aksi file `move`/`copy`/`trash`/`rename` (template) + `on_conflict` (rename/skip/overwrite/ask), `create_missing_dirs`, lintas-drive (copy + verifikasi + hapus sumber), laporan hasil per file, `dry_run`. (`safety.md`)
- [x] **T1.6** Undo stack berlapis + log persisten JSON Lines (niat dicatat dan di-flush sebelum eksekusi, hasil sesudahnya), batas `undo_history_limit`, pemulihan aman.
- [x] **T1.7** Hot-reload (`notify`, debounce, swap `Arc` atomik), config valid terakhir dipertahankan saat gagal.
- [x] **T1.8** Mock platform (`MockKeyboardHook`, `MockFileManagerContext`) di `keyflow-platform` dan test alur end-to-end tanpa OS.
- [x] **T1.9** Test sesuai `testing.md`. Jalankan `data-safety-auditor` dan `code-reviewer`.

**Selesai M1 bila**: semua test lolos tanpa OS asli, tidak ada jalur menimpa/menghapus permanen, audit keamanan data bersih.

## M2 - Aplikasi Windows lengkap

Dependensi kandidat: `windows`, `tray-icon`, `notify-rust`, `tracing-subscriber`, `anyhow`, `crossbeam-channel` atau `tokio`, `enigo` (atau `SendInput` langsung).

- [x] **T2.1** Implementasi `KeyboardHook` dan `FileManagerContext` Windows dari hasil M0 (isolasi `unsafe`, `// SAFETY:`). Catatan: modul `windows` di `keyflow-platform` bentrok namanya dengan crate `windows`; rujuk crate dengan `::windows::...` atau ganti nama modul (mis. `win`).
- [x] **T2.2** Cache konteks (window fokus + folder aktif) di thread terpisah; callback hook hanya membaca cache + tabel rule, **fail-open**. (`architecture.md`)
- [x] **T2.3** Worker: ambil seleksi, konfirmasi ulang konteks, jalankan aksi, catat undo, kirim notifikasi, `select_next` (tandai event buatan sendiri).
- [x] **T2.4** Tray: enable/disable global, reload config, buka folder config, buka log, keluar.
- [x] **T2.5** Notifikasi OS ("Dipindahkan ke 01_Dipakai"), notifikasi config rusak.
- [x] **T2.6** Hotkey undo (default `Ctrl+Shift+Z`) lewat rule `action: undo`.
- [x] **T2.7** `dry_run` global + per profil.
- [x] **T2.8** Uji manual Windows memakai `docs/MANUAL_TESTING.md`; ukur CPU idle dan latensi.

**Selesai M2 bila**: seluruh Kriteria Selesai MVP (`milestones.md`) terpenuhi di Windows.

## M3 - macOS

- [x] **T3.1** `CGEventTap` + penanganan `tapDisabledByTimeout`.
- [x] **T3.2** Deteksi izin Accessibility/Input Monitoring, pesan arahan ke System Settings.
- [x] **T3.3** Window fokus (`com.apple.finder`), folder aktif + seleksi via `osascript` dengan cache dan pembatasan frekuensi.
- [x] **T3.4** Uji manual macOS; perbarui `PERMISSIONS.md`, `PLATFORM_SUPPORT.md`.

## M4 - Linux (X11)

- [ ] **T4.1** Hook X11 (`rdev` grab atau key grab) + window fokus (`_NET_ACTIVE_WINDOW`).
- [ ] **T4.2** Folder aktif/seleksi via fallback clipboard (`text/uri-list`) dan judul window.
- [ ] **T4.3** Deteksi Wayland: peringatan jelas, tidak menelan tombol apa pun.
- [ ] **T4.4** Uji manual Linux; perbarui dokumentasi dan paket sistem di CI.

## M5 - Pengemasan

- [ ] **T5.1** CI hijau di 3 OS (lengkapi paket sistem Linux).
- [ ] **T5.2** Workflow rilis: binary per OS (artifact GitHub Releases).
- [ ] **T5.3** Finalisasi `README.md`, `PERMISSIONS.md`, `PLATFORM_SUPPORT.md`, `MANUAL_TESTING.md`, `examples/config.yaml`.
- [ ] **T5.4** Tinjauan akhir: `code-reviewer` + `data-safety-auditor` pada seluruh codebase.

## Prompt awal untuk agent pelaksana

> Baca `AGENTS.md`, lalu `docs/agents/plan.md`. Kerjakan **M0** sesuai tugas T0.0-T0.4 (mulai dari verifikasi skeleton). Tampilkan rencana singkat lalu langsung kerjakan. Patuhi aturan di `AGENTS.md` dan `docs/agents/safety.md`. Berhenti di akhir M0 dengan ringkasan: apa yang selesai, apa yang diuji, apa yang belum; jangan lanjut ke M1 tanpa persetujuan.

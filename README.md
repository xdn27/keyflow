# KeyFlow

> Nama sementara. Status: **perencanaan / skeleton**. Belum ada fungsi yang berjalan.

KeyFlow adalah aplikasi system tray (Rust) yang menjalankan shortcut keyboard **sadar konteks** untuk file manager bawaan OS. Contoh: di Explorer, sorot foto, tekan `1`, file langsung pindah ke `01_Dipakai`. Di luar konteks yang cocok, tombol `1` bekerja seperti biasa.

- Tidak punya GUI browsing sendiri; memakai file manager OS.
- Aman terhadap data: tidak ada hapus permanen, tidak pernah menimpa tanpa konfirmasi, undo berlapis, log persisten.
- Tanpa jaringan dan tanpa telemetri.

## Dukungan platform

Windows (utama) -> macOS -> Linux X11. Wayland: dukungan sangat terbatas. Lihat [docs/PLATFORM_SUPPORT.md](docs/PLATFORM_SUPPORT.md) dan [docs/PERMISSIONS.md](docs/PERMISSIONS.md).

## Konfigurasi

Lihat [examples/config.yaml](examples/config.yaml).

## Pengembangan

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Struktur: `crates/keyflow-core` (inti, tanpa OS), `crates/keyflow-platform` (trait + implementasi per OS), `crates/keyflow-app` (tray, notifikasi, wiring).

Untuk kontributor dan agent AI: mulai dari [AGENTS.md](AGENTS.md) dan [docs/agents/plan.md](docs/agents/plan.md).

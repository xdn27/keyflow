# GUI Pengaturan KeyFlow (Tahap 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pengguna dapat mengubah lima pengaturan global (`settings:`) KeyFlow lewat jendela GUI tanpa mengedit `config.yaml` manual.

**Architecture:** GUI egui/eframe berjalan sebagai proses terpisah (`keyflow settings`), sehingga tidak berbagi event loop dengan tray atau hook. Logika penyuntingan teks YAML (`config_edit`) hidup di `keyflow-core` tanpa kode OS; modul `settings_ui` di `keyflow-app` hanya mengorkestrasi muat/simpan dan merender form. Hasil simpan berupa `config.yaml` valid yang diambil aplikasi utama lewat hot-reload yang sudah ada.

**Tech Stack:** Rust 2021, `eframe`/`egui` 0.36 (fitur `glow`, `x11`, `wayland`, `default_fonts`), `serde_norway`, `thiserror`, `tempfile` (test).

**Spec:** `docs/superpowers/specs/2026-10-03-settings-gui-design.md`

**Lingkungan:** `cargo` ada di `~/.cargo/bin` tetapi belum masuk `PATH` shell ini. Awali setiap sesi dengan `export PATH="$HOME/.cargo/bin:$PATH"`.

## Global Constraints

- Tidak ada akses jaringan dan tidak ada telemetri (Aturan #7). Setelah menambah `eframe`, `cargo tree` tidak boleh memuat `ureq|reqwest|hyper|curl`.
- Config rusak tidak boleh membuat aplikasi crash; GUI tidak pernah menyimpan config yang tidak valid dan tidak pernah menimpa config yang sudah rusak (Aturan #6).
- Tidak pernah menimpa file pengguna tanpa konfirmasi eksplisit; default `on_conflict: rename` (Aturan #3). Memilih `overwrite` di GUI wajib lewat dialog konfirmasi.
- Callback hook tidak tersentuh. Tray hanya menjalankan proses anak (Aturan #5).
- `keyflow-core` tanpa kode OS dan tanpa dependensi GUI (`#![forbid(unsafe_code)]` tetap berlaku).
- Tidak ada `unwrap()`/`expect()`/`panic!` di jalur produksi (boleh di test); logging dengan `tracing`, bukan `println!`.
- `thiserror` untuk error di library crate; `anyhow` hanya di `keyflow-app`.
- Komentar dan dokumentasi dalam Bahasa Indonesia; identifier dalam bahasa Inggris.
- Kode `windows` tidak boleh terkompilasi di Linux/macOS (`#[cfg(target_os = "windows")]`).
- Sebuah perubahan selesai hanya jika keempat perintah ini lolos: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`.

## Review Focus

Input atau kondisi yang tersirat di spec tetapi mudah terlewat; masing-masing sudah punya test di task pemiliknya.

1. **Config berakhir baris CRLF (Windows)** harus tetap CRLF setelah simpan, tanpa campuran `\n` telanjang. → Task 1, `akhir_baris_crlf_dipertahankan`.
2. **`config.yaml` diedit di editor lain saat jendela terbuka**: simpan ditolak dan isi editor tidak ditimpa. → Task 3, `simpan_ditolak_bila_berkas_berubah_di_tempat_lain`.
3. **Bentuk YAML yang tidak dikenali** (`settings: {…}` inline, nilai bertanda kutip, blok hilang): ditolak (`Unpatchable`), tidak ditebak. → Task 1, `settings_gaya_inline_ditolak`, `nilai_bertanda_kutip_ditolak_bukan_ditebak`, `tanpa_blok_settings_ditolak`; Task 3, `bentuk_tak_dikenal_meminta_konfirmasi_dan_tidak_menulis`.
4. **Batas undo berisi sampah** (`""`, `abc`, `-5`, `0`, `2.5`, `10001`): tombol Simpan nonaktif, tidak ada yang ditulis. → Task 4, `batas_undo_tidak_valid_menolak_simpan`.
5. **Rename atomik harus benar-benar memicu hot-reload** (watcher hanya bereaksi pada event yang path-nya persis path config). → Task 3, `rename_atomik_memicu_hot_reload_yang_sudah_ada`.

## Deviasi dari spec (sengaja, sudah diverifikasi)

- **Deteksi perubahan di disk** memakai perbandingan isi teks penuh, bukan hash. Hasilnya setara dan lebih ketat, tanpa dependensi tambahan.
- **Rentang `undo_history_limit`**: `Config::validate()` tidak membatasi nilai ini, jadi spec ("mengikuti validasi core") tidak punya acuan. Task 1 mendefinisikan `UNDO_HISTORY_LIMIT_RANGE = 1..=10_000` yang **hanya dipakai GUI**; `validate()` sengaja tidak diubah agar config lama milik pengguna tidak tiba-tiba ditolak.
- **API eframe 0.36**: `App::ui(&mut self, ui, frame)` (bukan `update`) dan `CentralPanel::show` (bukan `show_inside`/`show(ctx)`). Kode di bawah sudah dikompilasi terhadap versi ini.
- **Satu jendela saja**: item tray tidak membuka jendela kedua selama proses pengaturan masih berjalan.
- **CI Linux**: build lokal tidak membutuhkan paket apt baru (X11/Wayland dimuat dinamis). Bila CI gagal tertaut, tambahkan `libxkbcommon-dev libwayland-dev` di `ci.yml` dan `release.yml`.

---

## File Structure

| Berkas | Aksi | Tanggung jawab |
|---|---|---|
| `crates/keyflow-core/src/config_edit.rs` | Create | `patch_settings`, `render_full`, `UNDO_HISTORY_LIMIT_RANGE` (murni, tanpa I/O) |
| `crates/keyflow-core/src/lib.rs` | Modify | `pub mod config_edit;` |
| `crates/keyflow-app/src/settings_ui/mod.rs` | Create | Deklarasi modul, `default_config_path()`, re-export `run` |
| `crates/keyflow-app/src/settings_ui/save.rs` | Create | `load`, `save`, penulisan atomik, cadangan `.bak` |
| `crates/keyflow-app/src/settings_ui/state.rs` | Create | `FormState`: logika form murni, dapat diuji tanpa egui |
| `crates/keyflow-app/src/settings_ui/view.rs` | Create | Rendering egui + dialog; tanpa logika bisnis |
| `crates/keyflow-app/src/main.rs` | Modify | `pub mod settings_ui;` dan subperintah `settings` |
| `crates/keyflow-app/src/tray.rs` | Modify | Item menu "Pengaturan..." (Windows) |
| `crates/keyflow-app/Cargo.toml` | Modify | `thiserror`, `eframe`, dev-dep `tempfile` |
| `docs/agents/*.md`, `docs/MANUAL_TESTING.md`, `AGENTS.md` | Modify | Dokumentasi |

---

### Task 1: `config_edit::patch_settings` (keyflow-core)

**Files:**
- Create: `crates/keyflow-core/src/config_edit.rs`
- Modify: `crates/keyflow-core/src/lib.rs`

**Interfaces:**
- Consumes: `keyflow_core::config::{Config, OnConflict, Settings}`, `Config::from_yaml`.
- Produces:
  - `pub const UNDO_HISTORY_LIMIT_RANGE: RangeInclusive<usize>` (`1..=10_000`)
  - `pub enum PatchResult { Patched(String), Unpatchable(PatchReason) }`
  - `pub enum PatchReason { InvalidOriginal(String), MissingBlock, DuplicateBlock, InlineBlock, UnsupportedLine { line: usize }, VerificationFailed(String) }` (implements `Display` lewat `thiserror`)
  - `pub fn patch_settings(yaml: &str, new: &Settings) -> PatchResult`

- [ ] **Step 0: Siapkan branch kerja**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
git checkout -b feat/settings-gui
cargo test --workspace 2>&1 | grep "test result"
```
Expected: semua `test result: ok` (baseline sebelum perubahan).

- [ ] **Step 1: Tulis test yang gagal**

Daftarkan modul di `crates/keyflow-core/src/lib.rs`, tepat setelah `pub mod config;`:

```rust
pub mod config_edit;
```

Buat `crates/keyflow-core/src/config_edit.rs` berisi baris `use` sementara dan modul test berikut:

```rust
use crate::config::{Config, OnConflict, Settings};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# KeyFlow config
version: 1

settings:
  # simulasi
  dry_run: false  # ubah ke true untuk uji coba
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 200

# Profil
profiles:
  - name: \"Foto\"
    enabled: true
    context:
      app: file_manager
    rules:
      - key: \"1\"
        action: move
        to: \"D:/Foto/01\"
";

    fn patched(yaml: &str, new: &Settings) -> String {
        match patch_settings(yaml, new) {
            PatchResult::Patched(text) => text,
            PatchResult::Unpatchable(reason) => panic!("seharusnya Patched: {reason}"),
        }
    }

    #[test]
    fn mengubah_nilai_dan_menjaga_komentar_serta_profil() {
        let new = Settings {
            dry_run: true,
            on_conflict: OnConflict::Skip,
            ..Settings::default()
        };
        let out = patched(SAMPLE, &new);

        assert!(out.contains("  dry_run: true  # ubah ke true untuk uji coba\n"));
        assert!(out.contains("  on_conflict: skip\n"));
        assert!(out.contains("  # simulasi\n"));
        assert!(out.starts_with("# KeyFlow config\n"));
        // Semua teks mulai dari komentar "# Profil" harus identik byte demi byte.
        assert_eq!(
            out.split_once("# Profil").unwrap().1,
            SAMPLE.split_once("# Profil").unwrap().1
        );
        assert_eq!(Config::from_yaml(&out).unwrap().settings, new);
    }

    #[test]
    fn pengaturan_yang_sama_menghasilkan_teks_identik() {
        let current = Config::from_yaml(SAMPLE).unwrap().settings;
        assert_eq!(patched(SAMPLE, &current), SAMPLE);
    }

    #[test]
    fn kunci_yang_hilang_disisipkan_dengan_indentasi_yang_sama() {
        let yaml = "version: 1\nsettings:\n    dry_run: true\nprofiles: []\n";
        let new = Settings {
            dry_run: true,
            undo_history_limit: 50,
            ..Settings::default()
        };
        let out = patched(yaml, &new);
        assert!(out.contains("    dry_run: true\n"));
        assert!(out.contains("    undo_history_limit: 50\n"));
        assert!(out.ends_with("profiles: []\n"));
        assert_eq!(Config::from_yaml(&out).unwrap().settings, new);
    }

    #[test]
    fn akhir_baris_crlf_dipertahankan() {
        let crlf = SAMPLE.replace('\n', "\r\n");
        let new = Settings {
            notifications: false,
            ..Settings::default()
        };
        let out = patched(&crlf, &new);
        assert!(out.contains("  notifications: false\r\n"));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn blok_settings_di_akhir_berkas_tanpa_newline() {
        let yaml = "version: 1\nsettings:\n  dry_run: false";
        let new = Settings {
            dry_run: true,
            ..Settings::default()
        };
        let out = patched(yaml, &new);
        assert!(out.contains("  dry_run: true\n"));
        assert!(out.ends_with("undo_history_limit: 200"));
        assert_eq!(Config::from_yaml(&out).unwrap().settings, new);
    }

    #[test]
    fn settings_gaya_inline_ditolak() {
        let yaml = "version: 1\nsettings: {dry_run: true}\n";
        assert_eq!(
            patch_settings(yaml, &Settings::default()),
            PatchResult::Unpatchable(PatchReason::InlineBlock)
        );
    }

    #[test]
    fn tanpa_blok_settings_ditolak() {
        let yaml = "version: 1\nprofiles: []\n";
        assert_eq!(
            patch_settings(yaml, &Settings::default()),
            PatchResult::Unpatchable(PatchReason::MissingBlock)
        );
    }

    #[test]
    fn nilai_bertanda_kutip_ditolak_bukan_ditebak() {
        let yaml = "version: 1\nsettings:\n  on_conflict: \"rename\"\n";
        assert!(matches!(
            patch_settings(yaml, &Settings::default()),
            PatchResult::Unpatchable(PatchReason::UnsupportedLine { line: 3 })
        ));
    }

    #[test]
    fn config_asli_tidak_valid_ditolak() {
        let yaml = "version: 1\nsettings:\n  dry_run: [salah\n";
        assert!(matches!(
            patch_settings(yaml, &Settings::default()),
            PatchResult::Unpatchable(PatchReason::InvalidOriginal(_))
        ));
    }

    #[test]
    fn blok_settings_ganda_ditolak_saat_asli_valid_secara_sintaks() {
        // YAML dengan kunci ganda ditolak parser (InvalidOriginal); pastikan tidak panic.
        let yaml = "version: 1\nsettings:\n  dry_run: true\nsettings:\n  dry_run: false\n";
        assert!(matches!(
            patch_settings(yaml, &Settings::default()),
            PatchResult::Unpatchable(_)
        ));
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p keyflow-core config_edit 2>&1 | tail -15`
Expected: gagal kompilasi, `cannot find function \`patch_settings\`` dan `cannot find type \`PatchResult\``.

- [ ] **Step 3: Tulis implementasi**

Ganti baris `use` sementara di bagian atas `config_edit.rs` dengan kode berikut (modul `tests` tetap di bawahnya, tidak diubah):

```rust
//! Penyuntingan teks `config.yaml` untuk GUI pengaturan (tahap 1).
//!
//! `patch_settings` hanya mengganti nilai di blok `settings:` sehingga komentar,
//! urutan, dan seluruh isi `profiles:` milik pengguna tidak berubah. Bila bentuk
//! berkas tidak dikenali, fungsi menolak (`Unpatchable`) dan tidak menebak. Setiap
//! hasil diverifikasi ulang lewat `Config::from_yaml` sebelum dikembalikan.
//!
//! Modul ini tidak melakukan I/O dan tidak bergantung pada OS.

use std::ops::RangeInclusive;

use thiserror::Error;

use crate::config::{Config, OnConflict, Settings};

/// Rentang `undo_history_limit` yang diizinkan GUI. Validasi `Config` sendiri tidak
/// membatasi nilai ini, jadi batas ini hanya berlaku untuk input dari GUI.
pub const UNDO_HISTORY_LIMIT_RANGE: RangeInclusive<usize> = 1..=10_000;

/// Hasil upaya menambal blok `settings:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchResult {
    /// Teks YAML baru yang sudah diverifikasi valid.
    Patched(String),
    /// Berkas tidak bisa ditambal dengan aman; alasannya disertakan.
    Unpatchable(PatchReason),
}

/// Alasan sebuah berkas tidak bisa ditambal otomatis.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatchReason {
    #[error("config asli tidak valid: {0}")]
    InvalidOriginal(String),
    #[error("blok 'settings:' tidak ditemukan")]
    MissingBlock,
    #[error("blok 'settings:' muncul lebih dari sekali")]
    DuplicateBlock,
    #[error("blok 'settings:' memakai gaya inline/flow")]
    InlineBlock,
    #[error("baris {line} di blok 'settings:' tidak dapat diedit otomatis")]
    UnsupportedLine { line: usize },
    #[error("hasil penambalan gagal diverifikasi: {0}")]
    VerificationFailed(String),
}

/// Satu baris teks beserta akhir barisnya (`\n`, `\r\n`, atau kosong di baris terakhir).
struct Line {
    text: String,
    ending: String,
}

/// Satu baris `kunci: nilai  # komentar` di dalam blok `settings:`.
struct Entry<'a> {
    indent: usize,
    key: &'a str,
    gap: &'a str,
    suffix: &'a str,
}

/// Mengganti nilai kelima field `settings` di teks YAML tanpa menyentuh bagian lain.
pub fn patch_settings(yaml: &str, new: &Settings) -> PatchResult {
    match try_patch(yaml, new) {
        Ok(text) => PatchResult::Patched(text),
        Err(reason) => PatchResult::Unpatchable(reason),
    }
}

fn try_patch(yaml: &str, new: &Settings) -> Result<String, PatchReason> {
    let original =
        Config::from_yaml(yaml).map_err(|e| PatchReason::InvalidOriginal(e.to_string()))?;

    let mut lines = split_lines(yaml);
    let eol = lines
        .iter()
        .map(|l| l.ending.as_str())
        .find(|e| !e.is_empty())
        .unwrap_or("\n")
        .to_string();
    let header = find_header(&lines)?;
    let end = block_end(&lines, header);

    let mut pending: Vec<(&'static str, String)> = vec![
        ("dry_run", new.dry_run.to_string()),
        ("notifications", new.notifications.to_string()),
        (
            "on_conflict",
            on_conflict_value(new.on_conflict).to_string(),
        ),
        ("create_missing_dirs", new.create_missing_dirs.to_string()),
        ("undo_history_limit", new.undo_history_limit.to_string()),
    ];
    let mut last_entry = header;
    let mut entry_indent: Option<usize> = None;

    for (idx, line) in lines.iter_mut().enumerate().take(end).skip(header + 1) {
        let text = line.text.clone();
        if is_blank_or_comment(&text) {
            continue;
        }
        let unsupported = PatchReason::UnsupportedLine { line: idx + 1 };
        let entry = parse_entry(&text).ok_or_else(|| unsupported.clone())?;
        let slot = pending
            .iter()
            .position(|(key, _)| *key == entry.key)
            .ok_or(unsupported)?;
        let (_, value) = pending.remove(slot);
        line.text = format!(
            "{}{}:{}{}{}",
            " ".repeat(entry.indent),
            entry.key,
            entry.gap,
            value,
            entry.suffix
        );
        entry_indent.get_or_insert(entry.indent);
        last_entry = idx;
    }

    if !pending.is_empty() {
        let indent = " ".repeat(entry_indent.unwrap_or(2));
        let anchor_was_last_line = lines[last_entry].ending.is_empty();
        if anchor_was_last_line {
            lines[last_entry].ending = eol.clone();
        }
        let count = pending.len();
        let inserted: Vec<Line> = pending
            .into_iter()
            .enumerate()
            .map(|(i, (key, value))| Line {
                text: format!("{indent}{key}: {value}"),
                ending: if anchor_was_last_line && i + 1 == count {
                    String::new()
                } else {
                    eol.clone()
                },
            })
            .collect();
        lines.splice(last_entry + 1..last_entry + 1, inserted);
    }

    let mut out = String::with_capacity(yaml.len() + 64);
    for line in &lines {
        out.push_str(&line.text);
        out.push_str(&line.ending);
    }

    let patched =
        Config::from_yaml(&out).map_err(|e| PatchReason::VerificationFailed(e.to_string()))?;
    let expected = Config {
        settings: new.clone(),
        ..original
    };
    if patched != expected {
        return Err(PatchReason::VerificationFailed(
            "isi hasil penambalan tidak sama dengan yang diharapkan".to_string(),
        ));
    }
    Ok(out)
}

fn split_lines(src: &str) -> Vec<Line> {
    src.split_inclusive('\n')
        .map(|raw| {
            if let Some(text) = raw.strip_suffix("\r\n") {
                Line {
                    text: text.to_string(),
                    ending: "\r\n".to_string(),
                }
            } else if let Some(text) = raw.strip_suffix('\n') {
                Line {
                    text: text.to_string(),
                    ending: "\n".to_string(),
                }
            } else {
                Line {
                    text: raw.to_string(),
                    ending: String::new(),
                }
            }
        })
        .collect()
}

/// Mencari indeks baris `settings:` di kolom 0; menolak bentuk inline atau ganda.
fn find_header(lines: &[Line]) -> Result<usize, PatchReason> {
    let mut found = None;
    for (idx, line) in lines.iter().enumerate() {
        let Some(rest) = line.text.strip_prefix("settings:") else {
            continue;
        };
        let rest = rest.trim();
        if !(rest.is_empty() || rest.starts_with('#')) {
            return Err(PatchReason::InlineBlock);
        }
        if found.replace(idx).is_some() {
            return Err(PatchReason::DuplicateBlock);
        }
    }
    found.ok_or(PatchReason::MissingBlock)
}

/// Blok berakhir di baris tak kosong pertama yang berada di kolom 0.
fn block_end(lines: &[Line], header: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(header + 1)
        .find(|(_, l)| !l.text.trim().is_empty() && !l.text.starts_with(' '))
        .map_or(lines.len(), |(i, _)| i)
}

fn is_blank_or_comment(text: &str) -> bool {
    let t = text.trim();
    t.is_empty() || t.starts_with('#')
}

/// Mengurai `kunci: nilai  # komentar` dengan nilai skalar polos; selain itu `None`.
fn parse_entry(text: &str) -> Option<Entry<'_>> {
    let body = text.trim_start_matches(' ');
    let indent = text.len() - body.len();
    let colon = body.find(':')?;
    let key = &body[..colon];
    if key.is_empty() || key.contains(char::is_whitespace) || key.starts_with(['"', '\'', '-', '#'])
    {
        return None;
    }
    let after = &body[colon + 1..];
    let value_part = after.trim_start_matches(' ');
    let gap = &after[..after.len() - value_part.len()];
    if gap.is_empty() {
        return None;
    }
    let value = match value_part.find(" #") {
        Some(pos) => value_part[..pos].trim_end(),
        None => value_part.trim_end(),
    };
    if value.is_empty()
        || value.starts_with(['"', '\'', '|', '>', '&', '*', '[', '{', '!', '%', '@', '`'])
    {
        return None;
    }
    Some(Entry {
        indent,
        key,
        gap,
        suffix: &value_part[value.len()..],
    })
}

fn on_conflict_value(value: OnConflict) -> &'static str {
    match value {
        OnConflict::Rename => "rename",
        OnConflict::Skip => "skip",
        OnConflict::Overwrite => "overwrite",
        OnConflict::Ask => "ask",
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lolos**

Run: `cargo test -p keyflow-core config_edit 2>&1 | grep -E "test |test result"`
Expected: 10 test `config_edit::tests::*` `ok`, `test result: ok. 10 passed`.

- [ ] **Step 5: Lint dan format**

Run: `cargo fmt --all && cargo clippy -p keyflow-core --all-targets -- -D warnings 2>&1 | tail -5`
Expected: tanpa error/warning.

- [ ] **Step 6: Commit**

```bash
git add crates/keyflow-core/src/config_edit.rs crates/keyflow-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): patch_settings untuk menambal blok settings tanpa mengubah komentar

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `config_edit::render_full` (jalur cadangan)

**Files:**
- Modify: `crates/keyflow-core/src/config_edit.rs`

**Interfaces:**
- Consumes: `Config` (derive `Serialize`), `serde_norway::to_string`.
- Produces: `pub struct RenderError`; `pub fn render_full(config: &Config) -> Result<String, RenderError>` (diawali satu baris komentar penanda).

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di **akhir** `config_edit.rs`:

```rust
#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn render_full_bisa_dibaca_kembali_dengan_isi_setara() {
        let yaml = "\
version: 1
settings:
  dry_run: true
profiles:
  - name: Foto
    context:
      path: \"D:/Foto/**\"
      selection: image
    rules:
      - key: \"1\"
        action: move
        to: \"D:/Foto/01\"
        then: select_next
";
        let config = Config::from_yaml(yaml).unwrap();
        let rendered = render_full(&config).unwrap();
        assert_eq!(Config::from_yaml(&rendered).unwrap(), config);
    }

    #[test]
    fn render_full_config_awal_aman_valid() {
        let rendered = render_full(&Config::default_safe()).unwrap();
        assert_eq!(
            Config::from_yaml(&rendered).unwrap(),
            Config::default_safe()
        );
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p keyflow-core render_tests 2>&1 | tail -10`
Expected: gagal kompilasi, `cannot find function \`render_full\``.

- [ ] **Step 3: Tulis implementasi**

Sisipkan blok ini tepat **di atas** `#[cfg(test)] mod tests {` (setelah fungsi `on_conflict_value`):

```rust
/// Galat saat menyerialisasi konfigurasi ke YAML.
#[derive(Debug, Error)]
#[error("gagal menyerialisasi konfigurasi: {0}")]
pub struct RenderError(String);

/// Menulis ulang seluruh konfigurasi lewat serde. Komentar dan format asli hilang,
/// jadi ini hanya jalur cadangan yang dipakai setelah konfirmasi pengguna.
pub fn render_full(config: &Config) -> Result<String, RenderError> {
    let body = serde_norway::to_string(config).map_err(|e| RenderError(e.to_string()))?;
    Ok(format!(
        "# Ditulis ulang oleh Pengaturan KeyFlow; komentar sebelumnya tidak dipertahankan.\n{body}"
    ))
}
```

- [ ] **Step 4: Jalankan test, pastikan lolos**

Run: `cargo test -p keyflow-core config_edit 2>&1 | grep "test result"`
Expected: `test result: ok. 12 passed`.

- [ ] **Step 5: Lint, format, commit**

```bash
cargo fmt --all && cargo clippy -p keyflow-core --all-targets -- -D warnings 2>&1 | tail -3
git add crates/keyflow-core/src/config_edit.rs
git commit -m "$(cat <<'EOF'
feat(core): render_full sebagai jalur tulis-ulang cadangan config

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `settings_ui::save` (muat, simpan atomik, cadangan)

**Files:**
- Create: `crates/keyflow-app/src/settings_ui/mod.rs`
- Create: `crates/keyflow-app/src/settings_ui/save.rs`
- Modify: `crates/keyflow-app/src/main.rs`
- Modify: `crates/keyflow-app/Cargo.toml`

**Interfaces:**
- Consumes: `keyflow_core::config::{Config, ConfigManager, Settings}`, `keyflow_core::config_edit::{patch_settings, render_full, PatchReason, PatchResult}`.
- Produces (di `settings_ui::save`):
  - `pub enum LoadOutcome { Missing, Loaded { text: String, settings: Settings }, Invalid { message: String } }`
  - `pub fn load(path: &Path) -> LoadOutcome`
  - `pub enum SaveMode { PatchOnly, AllowFullRewrite }`
  - `pub enum SaveOutcome { Saved, NeedsFullRewrite(PatchReason) }`
  - `pub enum SaveError { ChangedOnDisk, Invalid(String), Io(std::io::Error), Render(String) }`
  - `pub fn save(path: &Path, loaded_text: Option<&str>, new: &Settings, mode: SaveMode) -> Result<SaveOutcome, SaveError>`
  - di `settings_ui`: `pub fn default_config_path() -> anyhow::Result<PathBuf>`

- [ ] **Step 1: Tambah dependensi**

Di `crates/keyflow-app/Cargo.toml`, tambahkan `thiserror` di akhir blok `[dependencies]` (setelah `ctrlc`) dan blok `[dev-dependencies]` di akhir berkas:

```toml
thiserror = { workspace = true }
```

```toml
[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Daftarkan modul**

Di `crates/keyflow-app/src/main.rs`, tambahkan tepat sebelum `pub mod worker;`:

```rust
pub mod settings_ui;
```

Buat `crates/keyflow-app/src/settings_ui/mod.rs`:

```rust
//! GUI pengaturan KeyFlow (tahap 1: pengaturan global).
//!
//! Berjalan sebagai proses terpisah (`keyflow settings`) agar event loop GUI tidak
//! bercampur dengan hook/tray. Hasil simpan berupa `config.yaml` valid; aplikasi yang
//! sedang berjalan mengambilnya lewat hot-reload yang sudah ada.

// SEMENTARA: dihapus di Task 5 saat modul ini mulai dipakai dari `main`.
#![allow(dead_code)]

use std::path::PathBuf;

use directories::ProjectDirs;

pub mod save;

/// Lokasi `config.yaml` yang sama dengan yang dipakai aplikasi utama.
pub fn default_config_path() -> anyhow::Result<PathBuf> {
    let dir = match ProjectDirs::from("com", "KeyFlow", "KeyFlow") {
        Some(proj) => proj.config_dir().to_path_buf(),
        None => std::env::current_dir()?.join(".keyflow"),
    };
    Ok(dir.join("config.yaml"))
}
```

- [ ] **Step 3: Tulis test yang gagal**

Buat `crates/keyflow-app/src/settings_ui/save.rs` berisi doc comment sementara, blok `use` ini, dan modul test:

```rust
//! Alur muat dan simpan `config.yaml` untuk GUI pengaturan.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use keyflow_core::config::{Config, Settings};
use keyflow_core::config_edit::{patch_settings, render_full, PatchReason, PatchResult};
use thiserror::Error;
#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use keyflow_core::config::{ConfigManager, OnConflict};

    use super::*;

    const SAMPLE: &str = "\
# KeyFlow config
version: 1

settings:
  dry_run: false  # simulasi
  notifications: true
  on_conflict: rename
  create_missing_dirs: true
  undo_history_limit: 200

# Profil
profiles:
  - name: \"Foto\"
    context:
      app: file_manager
    rules:
      - key: \"1\"
        action: move
        to: \"D:/Foto/01\"
";

    fn write_sample(dir: &Path) -> PathBuf {
        let path = dir.join("config.yaml");
        fs::write(&path, SAMPLE).unwrap();
        path
    }

    fn loaded(path: &Path) -> (String, Settings) {
        match load(path) {
            LoadOutcome::Loaded { text, settings } => (text, settings),
            other => panic!("seharusnya Loaded: {other:?}"),
        }
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn load_berkas_tidak_ada_valid_dan_rusak() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        assert_eq!(load(&path), LoadOutcome::Missing);

        fs::write(&path, SAMPLE).unwrap();
        assert!(matches!(load(&path), LoadOutcome::Loaded { .. }));

        fs::write(&path, "version: 1\nsettings:\n  dry_run: [salah\n").unwrap();
        assert!(matches!(load(&path), LoadOutcome::Invalid { .. }));
    }

    #[test]
    fn simpan_menambal_nilai_dan_menjaga_komentar() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            on_conflict: OnConflict::Skip,
            ..settings
        };

        let outcome = save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("  dry_run: true  # simulasi\n"));
        assert!(written.contains("# Profil\n"));
        assert_eq!(Config::from_yaml(&written).unwrap().settings, new);
        assert_eq!(files_in(dir.path()), vec!["config.yaml"]);
    }

    #[test]
    fn simpan_ditolak_bila_berkas_berubah_di_tempat_lain() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);
        let edited_elsewhere = SAMPLE.replace("# KeyFlow config", "# diedit di editor");
        fs::write(&path, &edited_elsewhere).unwrap();

        let result = save(&path, Some(&text), &settings, SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(&path).unwrap(), edited_elsewhere);
    }

    #[test]
    fn simpan_ditolak_bila_berkas_baru_muncul_setelah_dimuat_sebagai_tidak_ada() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, SAMPLE).unwrap();

        let result = save(&path, None, &Settings::default(), SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(&path).unwrap(), SAMPLE);
    }

    #[test]
    fn berkas_belum_ada_dibuat_dengan_default_aman_plus_pengaturan_baru() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.yaml");
        let new = Settings {
            dry_run: true,
            ..Settings::default()
        };

        let outcome = save(&path, None, &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.settings, new);
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn bentuk_tak_dikenal_meminta_konfirmasi_dan_tidak_menulis() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let inline = "version: 1\nsettings: {dry_run: false}\n";
        fs::write(&path, inline).unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        let outcome = save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        assert_eq!(
            outcome,
            SaveOutcome::NeedsFullRewrite(PatchReason::InlineBlock)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), inline);
        assert_eq!(files_in(dir.path()), vec!["config.yaml"]);
    }

    #[test]
    fn tulis_ulang_penuh_membuat_cadangan_dan_menjaga_profil() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let inline = "version: 1\nsettings: {dry_run: false}\nprofiles:\n  - name: Foto\n";
        fs::write(&path, inline).unwrap();
        let (text, settings) = loaded(&path);
        let new = Settings {
            dry_run: true,
            ..settings
        };

        let outcome = save(&path, Some(&text), &new, SaveMode::AllowFullRewrite).unwrap();

        assert_eq!(outcome, SaveOutcome::Saved);
        assert_eq!(
            fs::read_to_string(dir.path().join("config.yaml.bak")).unwrap(),
            inline
        );
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.settings, new);
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(files_in(dir.path()), vec!["config.yaml", "config.yaml.bak"]);
    }

    #[test]
    fn galat_io_dikembalikan_tanpa_panic() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("bukan_folder");
        fs::write(&blocker, "x").unwrap();
        let path = blocker.join("config.yaml");

        let result = save(&path, None, &Settings::default(), SaveMode::PatchOnly);

        assert!(matches!(result, Err(SaveError::Io(_))));
    }

    #[test]
    fn rename_atomik_memicu_hot_reload_yang_sudah_ada() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sample(dir.path());
        let (text, settings) = loaded(&path);

        let mut manager = ConfigManager::new(path.clone()).unwrap();
        let (tx, rx) = mpsc::channel();
        manager
            .start_hot_reload(move |result| {
                let _ = tx.send(result.map(|cfg| cfg.settings.dry_run));
            })
            .unwrap();
        // Beri waktu watcher terpasang sebelum berkas diubah.
        std::thread::sleep(Duration::from_millis(1000));

        let new = Settings {
            dry_run: true,
            ..settings
        };
        save(&path, Some(&text), &new, SaveMode::PatchOnly).unwrap();

        let reloaded = rx.recv_timeout(Duration::from_secs(10));
        manager.stop_hot_reload();
        assert!(
            matches!(reloaded, Ok(Ok(true))),
            "hot-reload tidak menerima config baru: {reloaded:?}"
        );
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan gagal**

Run: `cargo test -p keyflow-app settings_ui 2>&1 | tail -15`
Expected: gagal kompilasi, `cannot find function \`load\`` / `\`save\`` / type `LoadOutcome`.

- [ ] **Step 5: Tulis implementasi**

Ganti isi `save.rs` di atas modul `tests` (doc comment sementara dan blok `use`) dengan kode berikut; modul `tests` tidak diubah:

```rust
//! Alur muat dan simpan `config.yaml` untuk GUI pengaturan.
//!
//! Aturan keselamatan: config yang tidak valid tidak pernah ditimpa, berkas yang
//! berubah di tempat lain tidak pernah ditimpa diam-diam, hasil selalu divalidasi
//! sebelum menyentuh disk, dan penulisan bersifat atomik (temp file lalu rename).

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use keyflow_core::config::{Config, Settings};
use keyflow_core::config_edit::{patch_settings, render_full, PatchReason, PatchResult};
use thiserror::Error;

/// Hasil memuat config dari disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Berkas belum ada; GUI menampilkan default aman.
    Missing,
    /// Berkas valid; `text` disimpan untuk mendeteksi perubahan di tempat lain.
    Loaded { text: String, settings: Settings },
    /// Berkas ada tetapi tidak valid atau tidak terbaca; GUI tidak boleh menimpanya.
    Invalid { message: String },
}

/// Memuat dan memvalidasi config dari `path`.
pub fn load(path: &Path) -> LoadOutcome {
    match fs::read_to_string(path) {
        Ok(text) => match Config::from_yaml(&text) {
            Ok(config) => LoadOutcome::Loaded {
                settings: config.settings,
                text,
            },
            Err(e) => LoadOutcome::Invalid {
                message: e.to_string(),
            },
        },
        Err(e) if e.kind() == ErrorKind::NotFound => LoadOutcome::Missing,
        Err(e) => LoadOutcome::Invalid {
            message: format!("Gagal membaca {}: {e}", path.display()),
        },
    }
}

/// Cara menyimpan bila penambalan teks tidak memungkinkan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// Hanya menambal blok `settings:`; bila tidak bisa, minta konfirmasi.
    PatchOnly,
    /// Boleh menulis ulang seluruh berkas (komentar hilang, cadangan `.bak` dibuat).
    AllowFullRewrite,
}

/// Hasil upaya menyimpan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    /// Penambalan ditolak; GUI harus meminta konfirmasi sebelum `AllowFullRewrite`.
    NeedsFullRewrite(PatchReason),
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("Berkas config diubah di tempat lain sejak dimuat. Muat ulang sebelum menyimpan.")]
    ChangedOnDisk,
    #[error("Config tidak valid, tidak ada yang ditulis: {0}")]
    Invalid(String),
    #[error("Gagal menulis config: {0}")]
    Io(#[from] std::io::Error),
    #[error("Gagal menyiapkan isi config: {0}")]
    Render(String),
}

/// Menyimpan `new` ke `path`. `loaded_text` adalah isi berkas saat dimuat
/// (`None` bila berkas belum ada saat itu).
pub fn save(
    path: &Path,
    loaded_text: Option<&str>,
    new: &Settings,
    mode: SaveMode,
) -> Result<SaveOutcome, SaveError> {
    let on_disk = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if on_disk.as_deref() != loaded_text {
        return Err(SaveError::ChangedOnDisk);
    }

    let (new_text, needs_backup) = match &on_disk {
        None => {
            let mut config = Config::default_safe();
            config.settings = new.clone();
            let text = render_full(&config).map_err(|e| SaveError::Render(e.to_string()))?;
            (text, false)
        }
        Some(text) => match patch_settings(text, new) {
            PatchResult::Patched(patched) => (patched, false),
            PatchResult::Unpatchable(PatchReason::InvalidOriginal(message)) => {
                return Err(SaveError::Invalid(message));
            }
            PatchResult::Unpatchable(reason) => match mode {
                SaveMode::PatchOnly => return Ok(SaveOutcome::NeedsFullRewrite(reason)),
                SaveMode::AllowFullRewrite => {
                    let mut config =
                        Config::from_yaml(text).map_err(|e| SaveError::Invalid(e.to_string()))?;
                    config.settings = new.clone();
                    let rendered =
                        render_full(&config).map_err(|e| SaveError::Render(e.to_string()))?;
                    (rendered, true)
                }
            },
        },
    };

    Config::from_yaml(&new_text).map_err(|e| SaveError::Invalid(e.to_string()))?;

    if needs_backup {
        fs::copy(path, backup_path(path))?;
    }
    atomic_write(path, &new_text)?;
    Ok(SaveOutcome::Saved)
}

/// `config.yaml` -> `config.yaml.bak` di folder yang sama.
fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Menulis lewat berkas sementara di folder yang sama lalu `rename`, sehingga
/// pembaca (termasuk hot-reload) tidak pernah melihat berkas setengah tertulis.
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".tmp-{}", std::process::id()));
    let tmp = path.with_file_name(name);

    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
```

- [ ] **Step 6: Jalankan test, pastikan lolos**

Run: `cargo test -p keyflow-app settings_ui 2>&1 | grep -E "test |test result"`
Expected: 9 test `settings_ui::save::tests::*` `ok`. Test `rename_atomik_memicu_hot_reload_yang_sudah_ada` bergantung waktu (tidur 1 detik, tunggu maksimal 10 detik); bila gagal, jalankan ulang sekali sebelum menyelidiki. Jika gagal konsisten, **berhenti**: artinya `rename` tidak memicu watcher di OS itu dan alur simpan harus ditinjau ulang dengan pengguna.

- [ ] **Step 7: Lint, format, commit**

```bash
cargo fmt --all && cargo clippy -p keyflow-app --all-targets -- -D warnings 2>&1 | tail -3
git add crates/keyflow-app
git commit -m "$(cat <<'EOF'
feat(app): alur muat/simpan config atomik untuk GUI pengaturan

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `settings_ui::state` (logika form murni)

**Files:**
- Create: `crates/keyflow-app/src/settings_ui/state.rs`
- Modify: `crates/keyflow-app/src/settings_ui/mod.rs`

**Interfaces:**
- Consumes: `keyflow_core::config::{OnConflict, Settings}`, `UNDO_HISTORY_LIMIT_RANGE`.
- Produces (`FormState`):
  - `FormState::new(saved: Settings) -> Self`
  - field publik: `dry_run`, `notifications`, `create_missing_dirs: bool`, `undo_limit_text: String`
  - `on_conflict() -> OnConflict`
  - `select_on_conflict(choice) -> bool` (`true` = perlu konfirmasi `overwrite`, pilihan belum diterapkan)
  - `confirm_overwrite()`, `cancel_overwrite()`
  - `undo_limit_error() -> Option<String>`
  - `draft() -> Option<Settings>`, `is_dirty() -> bool`, `can_save() -> bool`

- [ ] **Step 1: Tulis test yang gagal**

Di `settings_ui/mod.rs`, tambahkan `pub mod state;` tepat setelah `pub mod save;`. Buat `crates/keyflow-app/src/settings_ui/state.rs`:

```rust
//! State form pengaturan: logika murni tanpa egui agar mudah diuji.

use keyflow_core::config::{OnConflict, Settings};
use keyflow_core::config_edit::UNDO_HISTORY_LIMIT_RANGE;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_baru_tidak_dirty_dan_tidak_bisa_disimpan() {
        let form = FormState::new(Settings::default());
        assert!(!form.is_dirty());
        assert!(!form.can_save());
        assert_eq!(form.draft(), Some(Settings::default()));
    }

    #[test]
    fn mengubah_field_membuat_dirty_dan_bisa_disimpan() {
        let mut form = FormState::new(Settings::default());
        form.dry_run = true;
        assert!(form.is_dirty());
        assert!(form.can_save());
        assert_eq!(form.draft().map(|s| s.dry_run), Some(true));
    }

    #[test]
    fn mengembalikan_nilai_awal_menghilangkan_dirty() {
        let mut form = FormState::new(Settings::default());
        form.notifications = false;
        form.notifications = true;
        assert!(!form.is_dirty());
    }

    #[test]
    fn batas_undo_tidak_valid_menolak_simpan() {
        let mut form = FormState::new(Settings::default());
        for bad in ["", "abc", "-5", "0", "10001", "2.5"] {
            form.undo_limit_text = bad.to_string();
            assert!(
                form.undo_limit_error().is_some(),
                "{bad:?} seharusnya galat"
            );
            assert_eq!(form.draft(), None, "{bad:?}");
            assert!(!form.can_save(), "{bad:?}");
        }
    }

    #[test]
    fn batas_undo_di_tepi_rentang_valid() {
        let mut form = FormState::new(Settings::default());
        for ok in ["1", " 50 ", "10000"] {
            form.undo_limit_text = ok.to_string();
            assert_eq!(form.undo_limit_error(), None, "{ok:?}");
            assert!(form.draft().is_some(), "{ok:?}");
        }
    }

    #[test]
    fn memilih_overwrite_butuh_konfirmasi_dan_belum_diterapkan() {
        let mut form = FormState::new(Settings::default());
        assert!(form.select_on_conflict(OnConflict::Overwrite));
        assert_eq!(form.on_conflict(), OnConflict::Rename);
        assert!(!form.is_dirty());
    }

    #[test]
    fn konfirmasi_overwrite_menerapkan_pilihan() {
        let mut form = FormState::new(Settings::default());
        form.select_on_conflict(OnConflict::Overwrite);
        form.confirm_overwrite();
        assert_eq!(form.on_conflict(), OnConflict::Overwrite);
        assert!(form.is_dirty());
    }

    #[test]
    fn batal_overwrite_mempertahankan_nilai_sebelumnya() {
        let mut form = FormState::new(Settings::default());
        form.select_on_conflict(OnConflict::Overwrite);
        form.cancel_overwrite();
        form.confirm_overwrite(); // tanpa pending, tidak boleh menerapkan apa pun
        assert_eq!(form.on_conflict(), OnConflict::Rename);
    }

    #[test]
    fn pilihan_lain_diterapkan_langsung_tanpa_konfirmasi() {
        let mut form = FormState::new(Settings::default());
        assert!(!form.select_on_conflict(OnConflict::Skip));
        assert_eq!(form.on_conflict(), OnConflict::Skip);
        assert!(!form.select_on_conflict(OnConflict::Ask));
        assert_eq!(form.on_conflict(), OnConflict::Ask);
    }

    #[test]
    fn dari_overwrite_ke_overwrite_tidak_meminta_konfirmasi_lagi() {
        let saved = Settings {
            on_conflict: OnConflict::Overwrite,
            ..Settings::default()
        };
        let mut form = FormState::new(saved);
        assert!(!form.select_on_conflict(OnConflict::Overwrite));
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p keyflow-app settings_ui::state 2>&1 | tail -10`
Expected: gagal kompilasi, `cannot find type \`FormState\``.

- [ ] **Step 3: Tulis implementasi**

Ganti doc comment sementara dan blok `use` di `state.rs` dengan kode berikut (modul `tests` tidak diubah):

```rust
//! State form pengaturan: logika murni tanpa egui agar mudah diuji.

use keyflow_core::config::{OnConflict, Settings};
use keyflow_core::config_edit::UNDO_HISTORY_LIMIT_RANGE;

/// Nilai form yang sedang diedit beserta nilai yang terakhir tersimpan.
pub struct FormState {
    saved: Settings,
    pub dry_run: bool,
    pub notifications: bool,
    pub create_missing_dirs: bool,
    /// Teks input angka; bisa berisi nilai tidak valid selama pengguna mengetik.
    pub undo_limit_text: String,
    on_conflict: OnConflict,
    pending_overwrite: bool,
}

impl FormState {
    pub fn new(saved: Settings) -> Self {
        Self {
            dry_run: saved.dry_run,
            notifications: saved.notifications,
            create_missing_dirs: saved.create_missing_dirs,
            undo_limit_text: saved.undo_history_limit.to_string(),
            on_conflict: saved.on_conflict,
            pending_overwrite: false,
            saved,
        }
    }

    pub fn on_conflict(&self) -> OnConflict {
        self.on_conflict
    }

    /// Memilih mode konflik. Mengembalikan `true` bila perlu konfirmasi eksplisit
    /// (memilih `overwrite`); pilihan baru belum diterapkan sampai dikonfirmasi.
    pub fn select_on_conflict(&mut self, choice: OnConflict) -> bool {
        if choice == OnConflict::Overwrite && self.on_conflict != OnConflict::Overwrite {
            self.pending_overwrite = true;
            return true;
        }
        self.on_conflict = choice;
        self.pending_overwrite = false;
        false
    }

    pub fn confirm_overwrite(&mut self) {
        if self.pending_overwrite {
            self.on_conflict = OnConflict::Overwrite;
            self.pending_overwrite = false;
        }
    }

    pub fn cancel_overwrite(&mut self) {
        self.pending_overwrite = false;
    }

    /// Pesan galat untuk input batas undo, atau `None` bila valid.
    pub fn undo_limit_error(&self) -> Option<String> {
        match self.undo_limit_text.trim().parse::<usize>() {
            Err(_) => Some("Masukkan angka bulat positif.".to_string()),
            Ok(n) if !UNDO_HISTORY_LIMIT_RANGE.contains(&n) => Some(format!(
                "Harus antara {} dan {}.",
                UNDO_HISTORY_LIMIT_RANGE.start(),
                UNDO_HISTORY_LIMIT_RANGE.end()
            )),
            Ok(_) => None,
        }
    }

    /// `Settings` dari isi form, atau `None` bila ada input yang belum valid.
    pub fn draft(&self) -> Option<Settings> {
        let undo_history_limit = self.undo_limit_text.trim().parse::<usize>().ok()?;
        if !UNDO_HISTORY_LIMIT_RANGE.contains(&undo_history_limit) {
            return None;
        }
        Some(Settings {
            dry_run: self.dry_run,
            notifications: self.notifications,
            on_conflict: self.on_conflict,
            create_missing_dirs: self.create_missing_dirs,
            undo_history_limit,
        })
    }

    /// Ada perbedaan dari yang tersimpan (input tidak valid dihitung berbeda).
    pub fn is_dirty(&self) -> bool {
        self.draft().as_ref() != Some(&self.saved)
    }

    pub fn can_save(&self) -> bool {
        self.draft().is_some() && self.is_dirty()
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lolos**

Run: `cargo test -p keyflow-app settings_ui::state 2>&1 | grep "test result"`
Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Lint, format, commit**

```bash
cargo fmt --all && cargo clippy -p keyflow-app --all-targets -- -D warnings 2>&1 | tail -3
git add crates/keyflow-app
git commit -m "$(cat <<'EOF'
feat(app): FormState untuk logika form pengaturan

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Jendela egui dan subperintah `keyflow settings`

**Files:**
- Create: `crates/keyflow-app/src/settings_ui/view.rs`
- Modify: `crates/keyflow-app/src/settings_ui/mod.rs` (versi final, menghapus `allow(dead_code)` sementara)
- Modify: `crates/keyflow-app/src/main.rs`
- Modify: `crates/keyflow-app/Cargo.toml`

**Interfaces:**
- Consumes: `save::{load, save, LoadOutcome, SaveMode, SaveOutcome, SaveError}`, `state::FormState`, `default_config_path()`.
- Produces: `settings_ui::run(config_path: PathBuf) -> anyhow::Result<()>` (memblokir sampai jendela ditutup); subperintah `keyflow settings`.

- [ ] **Step 1: Tambah dependensi `eframe`**

Di `crates/keyflow-app/Cargo.toml`, tambahkan setelah baris `thiserror`:

```toml
eframe = { version = "0.36", default-features = false, features = ["default_fonts", "glow", "x11", "wayland"] }
```

Konfirmasi versi terbaru dan tidak ada dependensi jaringan:

```bash
cargo search eframe --limit 1
cargo tree -p keyflow-app 2>&1 | grep -i -E "ureq|reqwest|hyper|isahc|curl|tokio" || echo "TIDAK ADA DEPENDENSI JARINGAN"
```
Expected: `eframe = "0.36.x"` (bila sudah ada versi lebih baru, baca changelog sebelum menaikkan; kode di bawah dikompilasi terhadap 0.36.2) dan `TIDAK ADA DEPENDENSI JARINGAN`.

Catat alasan pemilihan di pesan commit (egui dipilih menggantikan Tauri: tanpa toolchain Node/webview, sesuai spec).

- [ ] **Step 2: Buat `view.rs`**

Buat `crates/keyflow-app/src/settings_ui/view.rs`:

```rust
//! Rendering egui jendela pengaturan. Logika bisnis ada di `state` dan `save`.

use std::path::PathBuf;

use eframe::egui;
use keyflow_core::config::{OnConflict, Settings};

use super::save::{self, LoadOutcome, SaveError, SaveMode, SaveOutcome};
use super::state::FormState;

/// Membuka jendela pengaturan dan memblokir sampai ditutup.
pub fn run(config_path: PathBuf) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pengaturan KeyFlow")
            .with_inner_size([500.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pengaturan KeyFlow",
        options,
        Box::new(move |_cc| Ok(Box::new(SettingsApp::new(config_path)))),
    )
    .map_err(|e| anyhow::anyhow!("Gagal membuka jendela pengaturan: {e}"))
}

#[derive(Clone)]
enum Dialog {
    ConfirmOverwrite,
    ConfirmFullRewrite(String),
    ChangedOnDisk,
}

enum Status {
    Saved,
    Error(String),
}

struct SettingsApp {
    path: PathBuf,
    /// Isi berkas saat dimuat; `None` bila berkas belum ada.
    loaded_text: Option<String>,
    form: FormState,
    /// Terisi bila config di disk tidak valid; form dinonaktifkan.
    blocked: Option<String>,
    dialog: Option<Dialog>,
    status: Option<Status>,
}

impl SettingsApp {
    fn new(path: PathBuf) -> Self {
        let mut app = Self {
            path,
            loaded_text: None,
            form: FormState::new(Settings::default()),
            blocked: None,
            dialog: None,
            status: None,
        };
        app.reload();
        app
    }

    /// Membuang isi form dan membaca ulang dari disk.
    fn reload(&mut self) {
        self.status = None;
        self.dialog = None;
        match save::load(&self.path) {
            LoadOutcome::Missing => {
                self.loaded_text = None;
                self.blocked = None;
                self.form = FormState::new(Settings::default());
            }
            LoadOutcome::Loaded { text, settings } => {
                self.loaded_text = Some(text);
                self.blocked = None;
                self.form = FormState::new(settings);
            }
            LoadOutcome::Invalid { message } => {
                self.loaded_text = None;
                self.blocked = Some(message);
                self.form = FormState::new(Settings::default());
            }
        }
    }

    fn try_save(&mut self, mode: SaveMode) {
        let Some(settings) = self.form.draft() else {
            return;
        };
        match save::save(&self.path, self.loaded_text.as_deref(), &settings, mode) {
            Ok(SaveOutcome::Saved) => {
                tracing::info!(path = %self.path.display(), "Pengaturan disimpan dari GUI");
                self.reload();
                self.status = Some(Status::Saved);
            }
            Ok(SaveOutcome::NeedsFullRewrite(reason)) => {
                self.dialog = Some(Dialog::ConfirmFullRewrite(reason.to_string()));
            }
            Err(SaveError::ChangedOnDisk) => self.dialog = Some(Dialog::ChangedOnDisk),
            Err(error) => {
                tracing::warn!(%error, "Gagal menyimpan pengaturan dari GUI");
                self.status = Some(Status::Error(error.to_string()));
            }
        }
    }

    fn draw_form(&mut self, ui: &mut egui::Ui) {
        ui.heading("Pengaturan KeyFlow");
        ui.label(
            egui::RichText::new(self.path.display().to_string())
                .weak()
                .small(),
        );
        ui.add_space(8.0);

        if let Some(message) = &self.blocked {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "Config saat ini tidak valid, jadi pengaturan tidak dapat disimpan dari sini. \
                 Perbaiki config.yaml, lalu klik \"Muat ulang\".",
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    ui.monospace(message);
                });
            ui.add_space(8.0);
            if ui.button("Muat ulang").clicked() {
                self.reload();
            }
            return;
        }

        ui.checkbox(&mut self.form.dry_run, "Mode simulasi (dry run)");
        ui.label(
            egui::RichText::new(
                "Berkas fisik tidak disentuh; hanya dicatat di log dan notifikasi.",
            )
            .weak(),
        );
        ui.add_space(6.0);
        ui.checkbox(&mut self.form.notifications, "Tampilkan notifikasi desktop");
        ui.add_space(6.0);
        ui.checkbox(
            &mut self.form.create_missing_dirs,
            "Buat folder tujuan otomatis bila belum ada",
        );
        ui.add_space(6.0);

        let mut picked = None;
        ui.horizontal(|ui| {
            ui.label("Jika nama berkas sama:");
            egui::ComboBox::from_id_salt("on_conflict")
                .selected_text(conflict_label(self.form.on_conflict()))
                .show_ui(ui, |ui| {
                    for option in [
                        OnConflict::Rename,
                        OnConflict::Skip,
                        OnConflict::Overwrite,
                        OnConflict::Ask,
                    ] {
                        let selected = self.form.on_conflict() == option;
                        if ui
                            .selectable_label(selected, conflict_label(option))
                            .clicked()
                        {
                            picked = Some(option);
                        }
                    }
                });
        });
        if let Some(option) = picked {
            if self.form.select_on_conflict(option) {
                self.dialog = Some(Dialog::ConfirmOverwrite);
            }
        }
        ui.label(egui::RichText::new(conflict_hint(self.form.on_conflict())).weak());
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Batas riwayat undo:");
            ui.add(egui::TextEdit::singleline(&mut self.form.undo_limit_text).desired_width(80.0));
        });
        if let Some(error) = self.form.undo_limit_error() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            let can_save = self.form.can_save();
            if ui
                .add_enabled(can_save, egui::Button::new("Simpan"))
                .clicked()
            {
                self.try_save(SaveMode::PatchOnly);
            }
            if ui.button("Muat ulang").clicked() {
                self.reload();
            }
            if self.form.is_dirty() {
                ui.label(egui::RichText::new("Ada perubahan yang belum disimpan").weak());
            }
        });
        match &self.status {
            Some(Status::Saved) => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Tersimpan.");
            }
            Some(Status::Error(message)) => {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            }
            None => {}
        }
    }

    fn draw_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        match dialog {
            Dialog::ConfirmOverwrite => {
                dialog_window("Konfirmasi: timpa berkas").show(ctx, |ui| {
                    ui.label(
                        "Dengan \"timpa\", berkas lama di folder tujuan akan diganti oleh berkas \
                         baru bernama sama dan bisa hilang. Pilihan aman adalah \"rename\" (bawaan).",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Ya, timpa").clicked() {
                            self.form.confirm_overwrite();
                            self.dialog = None;
                        }
                        if ui.button("Batal").clicked() {
                            self.form.cancel_overwrite();
                            self.dialog = None;
                        }
                    });
                });
            }
            Dialog::ConfirmFullRewrite(reason) => {
                dialog_window("Tulis ulang seluruh berkas?").show(ctx, |ui| {
                    ui.label(format!(
                        "Config tidak dapat diubah tanpa mengubah formatnya ({reason}). \
                         Anda dapat menulis ulang seluruh berkas: komentar akan hilang, dan \
                         cadangan config.yaml.bak dibuat lebih dulu."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Tulis ulang penuh").clicked() {
                            self.dialog = None;
                            self.try_save(SaveMode::AllowFullRewrite);
                        }
                        if ui.button("Batal").clicked() {
                            self.dialog = None;
                        }
                    });
                });
            }
            Dialog::ChangedOnDisk => {
                dialog_window("Config berubah di tempat lain").show(ctx, |ui| {
                    ui.label(
                        "config.yaml diubah di luar jendela ini sejak dimuat. Muat ulang untuk \
                         melihat versi terbaru; perubahan di form ini akan dibuang.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Muat ulang").clicked() {
                            self.reload();
                        }
                        if ui.button("Tutup").clicked() {
                            self.dialog = None;
                        }
                    });
                });
            }
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| self.draw_form(ui));
        self.draw_dialog(&ctx);
    }
}

fn dialog_window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

fn conflict_label(option: OnConflict) -> &'static str {
    match option {
        OnConflict::Rename => "rename (disarankan)",
        OnConflict::Skip => "skip",
        OnConflict::Overwrite => "overwrite (berisiko)",
        OnConflict::Ask => "ask",
    }
}

fn conflict_hint(option: OnConflict) -> &'static str {
    match option {
        OnConflict::Rename => {
            "Nama otomatis diberi nomor, mis. foto (1).jpg. Tidak ada berkas tertimpa."
        }
        OnConflict::Skip => "Berkas dilewati bila nama sudah ada di tujuan.",
        OnConflict::Overwrite => "Berkas lama di tujuan ditimpa. Gunakan dengan hati-hati.",
        OnConflict::Ask => "Meminta konfirmasi setiap kali ada nama yang sama.",
    }
}
```

- [ ] **Step 3: Finalkan `mod.rs`**

Ganti seluruh isi `crates/keyflow-app/src/settings_ui/mod.rs` (ini menghapus `#![allow(dead_code)]` sementara):

```rust
//! GUI pengaturan KeyFlow (tahap 1: pengaturan global).
//!
//! Berjalan sebagai proses terpisah (`keyflow settings`) agar event loop GUI tidak
//! bercampur dengan hook/tray. Hasil simpan berupa `config.yaml` valid; aplikasi yang
//! sedang berjalan mengambilnya lewat hot-reload yang sudah ada.

use std::path::PathBuf;

use directories::ProjectDirs;

pub mod save;
pub mod state;
pub mod view;

pub use view::run;

/// Lokasi `config.yaml` yang sama dengan yang dipakai aplikasi utama.
pub fn default_config_path() -> anyhow::Result<PathBuf> {
    let dir = match ProjectDirs::from("com", "KeyFlow", "KeyFlow") {
        Some(proj) => proj.config_dir().to_path_buf(),
        None => std::env::current_dir()?.join(".keyflow"),
    };
    Ok(dir.join("config.yaml"))
}
```

- [ ] **Step 4: Tambah subperintah di `main.rs`**

Di `crates/keyflow-app/src/main.rs`, sisipkan blok ini tepat setelah `.init();` pada inisialisasi `tracing_subscriber` dan sebelum `#[cfg(target_os = "windows")]`:

```rust
    // Subperintah opsional; tanpa argumen, perilaku aplikasi tidak berubah.
    if let Some(command) = std::env::args().nth(1) {
        return match command.as_str() {
            "settings" => settings_ui::run(settings_ui::default_config_path()?),
            other => anyhow::bail!("Perintah tidak dikenal: {other}. Perintah tersedia: settings"),
        };
    }
```

- [ ] **Step 5: Format, kompilasi, dan lint**

Run:
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -8
```
Expected: `Finished`, tanpa warning. Bila API eframe berbeda dari yang dikompilasi (versi lain), baca pesan kompilator; logika `state`/`save` tidak perlu berubah, hanya pemanggilan egui di `view.rs`.

- [ ] **Step 6: Seluruh test**

Run: `cargo test --workspace 2>&1 | grep -E "test result|FAILED"`
Expected: semua `ok`.

- [ ] **Step 7: Uji manual jendela (butuh display)**

```bash
mkdir -p /tmp/kf-manual && cp examples/config.yaml /tmp/kf-manual/config.yaml
HOME=/tmp/kf-manual XDG_CONFIG_HOME=/tmp/kf-manual cargo run -p keyflow-app -- settings
```
Periksa: jendela terbuka dan menampilkan nilai dari config; ubah `dry_run`, klik Simpan, muncul "Tersimpan."; buka `config.yaml` dan pastikan komentar serta profil tetap utuh; pilih `overwrite` memunculkan dialog konfirmasi dan "Batal" mengembalikan pilihan lama. Bila mesin tidak punya display, lewati dan catat di ringkasan bahwa langkah ini belum dijalankan.

- [ ] **Step 8: Commit**

```bash
git add crates/keyflow-app Cargo.lock
git commit -m "$(cat <<'EOF'
feat(app): jendela pengaturan egui dan subperintah `keyflow settings`

egui/eframe dipilih daripada Tauri: Rust murni, tanpa toolchain Node/webview,
cukup untuk form tahap 1 dan berjalan di proses terpisah dari hook/tray.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Item tray "Pengaturan..." (Windows)

**Files:**
- Modify: `crates/keyflow-app/src/tray.rs`

**Interfaces:**
- Consumes: subperintah `keyflow settings` dari Task 5.
- Produces: item menu tray yang menjalankan `keyflow settings` sebagai proses anak, maksimal satu jendela pada satu waktu.

- [ ] **Step 1: Terapkan perubahan**

Terapkan diff berikut ke `crates/keyflow-app/src/tray.rs` (bisa disimpan sebagai `tray.patch` lalu `git apply tray.patch`, atau diedit manual; diff ini sudah lolos `cargo clippy --target x86_64-pc-windows-gnu`):

```diff
--- a/crates/keyflow-app/src/tray.rs
+++ b/crates/keyflow-app/src/tray.rs
@@ -1,8 +1,9 @@
 //! System Tray icon dan menu kontekstual Windows (T2.4).
 
 use std::path::{Path, PathBuf};
+use std::process::Child;
 use std::sync::atomic::{AtomicBool, Ordering};
-use std::sync::Arc;
+use std::sync::{Arc, Mutex};
 
 use crossbeam_channel::Sender;
 use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
@@ -24,6 +25,7 @@
     is_paused_item: CheckMenuItem,
     undo_item: MenuItem,
     reload_item: MenuItem,
+    settings_item: MenuItem,
     open_config_item: MenuItem,
     open_log_item: MenuItem,
     quit_item: MenuItem,
@@ -31,6 +33,8 @@
     log_path: PathBuf,
     worker_tx: Sender<WorkerTask>,
     is_enabled: Arc<AtomicBool>,
+    /// Proses jendela pengaturan yang sedang berjalan, agar tidak dibuka ganda.
+    settings_child: Mutex<Option<Child>>,
 }
 
 impl TrayManager {
@@ -47,6 +51,7 @@
         let undo_item = MenuItem::new("Undo Terakhir (Ctrl+Shift+Z)", true, None);
         let separator1 = PredefinedMenuItem::separator();
         let reload_item = MenuItem::new("Reload Konfigurasi", true, None);
+        let settings_item = MenuItem::new("Pengaturan...", true, None);
         let open_config_item = MenuItem::new("Buka Folder Konfigurasi", true, None);
         let open_log_item = MenuItem::new("Buka File Log Undo", true, None);
         let separator2 = PredefinedMenuItem::separator();
@@ -55,6 +60,7 @@
         menu.append(&is_paused_item)?;
         menu.append(&undo_item)?;
         menu.append(&separator1)?;
+        menu.append(&settings_item)?;
         menu.append(&reload_item)?;
         menu.append(&open_config_item)?;
         menu.append(&open_log_item)?;
@@ -74,6 +80,7 @@
             is_paused_item,
             undo_item,
             reload_item,
+            settings_item,
             open_config_item,
             open_log_item,
             quit_item,
@@ -81,6 +88,7 @@
             log_path,
             worker_tx,
             is_enabled,
+            settings_child: Mutex::new(None),
         })
     }
 
@@ -103,6 +111,8 @@
                 let _ = self.worker_tx.send(WorkerTask::ExecuteUndo);
             } else if event.id == self.reload_item.id() {
                 result_action = TrayAction::ReloadConfig;
+            } else if event.id == self.settings_item.id() {
+                self.open_settings_window();
             } else if event.id == self.open_config_item.id() {
                 open_in_file_manager(&self.config_dir);
             } else if event.id == self.open_log_item.id() {
@@ -115,6 +125,33 @@
 
         result_action
     }
+
+    /// Menjalankan `keyflow settings` sebagai proses terpisah (GUI tidak berbagi
+    /// event loop dengan tray/hook). Bila jendela masih terbuka, tidak membuka lagi.
+    fn open_settings_window(&self) {
+        let Ok(mut child_slot) = self.settings_child.lock() else {
+            return;
+        };
+        if let Some(child) = child_slot.as_mut() {
+            if matches!(child.try_wait(), Ok(None)) {
+                tracing::info!("Jendela pengaturan sudah terbuka");
+                return;
+            }
+        }
+        *child_slot = None;
+
+        let exe = match std::env::current_exe() {
+            Ok(exe) => exe,
+            Err(e) => {
+                tracing::warn!(error = %e, "Tidak dapat menentukan lokasi keyflow.exe");
+                return;
+            }
+        };
+        match std::process::Command::new(exe).arg("settings").spawn() {
+            Ok(child) => *child_slot = Some(child),
+            Err(e) => tracing::warn!(error = %e, "Gagal membuka jendela pengaturan"),
+        }
+    }
 }
 
 /// Menghasilkan ikon RGBA 32x32 dalam memori untuk icon tray.
```

- [ ] **Step 2: Verifikasi kompilasi target Windows**

Run:
```bash
cargo fmt --all
cargo clippy -p keyflow-app --target x86_64-pc-windows-gnu --all-targets -- -D warnings 2>&1 | tail -5
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
```
Expected: kedua `Finished` tanpa warning (target `x86_64-pc-windows-gnu` sudah terpasang di mesin ini; bila tidak, `rustup target add x86_64-pc-windows-gnu`). Kode Windows tidak boleh ikut terkompilasi di build Linux biasa.

- [ ] **Step 3: Catat uji manual**

Klik "Pengaturan..." di tray Windows hanya bisa diuji di Windows. Tandai di ringkasan akhir bahwa baris ini **belum** diuji manual bila tidak ada mesin Windows (lihat checklist di Task 7).

- [ ] **Step 4: Commit**

```bash
git add crates/keyflow-app/src/tray.rs
git commit -m "$(cat <<'EOF'
feat(app): item tray "Pengaturan..." membuka GUI sebagai proses terpisah

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Dokumentasi dan verifikasi akhir

**Files:**
- Modify: `docs/agents/product.md`, `docs/agents/architecture.md`, `docs/MANUAL_TESTING.md`, `AGENTS.md`

**Interfaces:**
- Consumes: semua task sebelumnya.
- Produces: dokumentasi yang selaras dengan kode; bukti keempat perintah `CLAUDE.md` lolos.

- [ ] **Step 1: `docs/agents/product.md`**

Ganti baris `- UI pengaturan (kandidat: Tauri atau egui).` di bagian "Pasca-MVP" dengan:

```markdown
- UI pengaturan tahap 2: editor profil, konteks, dan rule (tahap 1 sudah ada, lihat di bawah).
```

Tambahkan satu butir di akhir daftar fitur MVP (setelah "Hot-reload konfigurasi saat file berubah."):

```markdown
- GUI Pengaturan (tahap 1): `keyflow settings` atau item tray "Pengaturan..." mengubah blok `settings:` global tanpa menghapus komentar/profil di `config.yaml`.
```

- [ ] **Step 2: `docs/agents/architecture.md`**

Di bagian "Titik perluasan untuk pasca-MVP" ganti butir terakhir ("Config bersifat berversi...") dengan:

```markdown
- Config bersifat berversi. GUI pengaturan (`keyflow-app/src/settings_ui`) membaca dan menulis berkas yang sama lewat `keyflow-core::config_edit` (`patch_settings` menambal teks YAML tanpa mengubah komentar; `render_full` hanya jalur cadangan dengan konfirmasi). GUI berjalan sebagai **proses terpisah** (`keyflow settings`) sehingga event loop egui tidak bercampur dengan hook/tray; perubahan diterapkan lewat hot-reload yang sudah ada. Tahap 2 (editor profil/rule) cukup menambah fungsi patch untuk blok `profiles:`.
```

- [ ] **Step 3: `docs/MANUAL_TESTING.md`**

Tambahkan baris berikut di akhir tabel skenario (lanjutkan nomor setelah baris terakhir yang ada; contoh di bawah memakai 13-17, sesuaikan bila nomor terakhir berbeda):

```markdown
| 13 | **GUI Pengaturan membuka & memuat**: Jalankan `keyflow settings` (atau klik "Pengaturan..." di tray Windows); jendela menampilkan nilai dari `config.yaml`. Klik tray dua kali: tidak muncul jendela kedua. | [ ] | [ ] | [ ] |
| 14 | **Simpan menjaga komentar**: Ubah `dry_run`, klik Simpan; `config.yaml` berubah hanya pada nilai itu, komentar dan profil tetap utuh. | [ ] | [ ] | [ ] |
| 15 | **Hot-reload dari GUI**: Dengan KeyFlow berjalan, simpan dari GUI; perubahan langsung berlaku (mis. `dry_run: true` membuat shortcut hanya notifikasi) tanpa restart. | [ ] | [ ] | [ ] |
| 16 | **Konfirmasi `overwrite`**: Memilih `overwrite` memunculkan dialog peringatan; "Batal" mengembalikan pilihan sebelumnya. | [ ] | [ ] | [ ] |
| 17 | **Config diubah di luar GUI**: Biarkan GUI terbuka, edit `config.yaml` di editor lain, lalu klik Simpan di GUI; muncul peringatan, isi editor tidak tertimpa. Config yang sedang rusak: GUI menampilkan error dan menolak menyimpan. | [ ] | [ ] | [ ] |
```

- [ ] **Step 4: `AGENTS.md`**

Di bagian `## Status`, tambahkan paragraf baru setelah paragraf yang ada (edit hanya `AGENTS.md`, `CLAUDE.md`/`GEMINI.md`/`QWEN.md` adalah symlink):

```markdown
Pasca-MVP: **GUI Pengaturan tahap 1** (blok `settings:` global lewat `keyflow settings`, egui, proses terpisah) selesai. Tahap 2 (editor profil/rule) belum dikerjakan.
```

- [ ] **Step 5: Verifikasi akhir (empat perintah wajib)**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --workspace 2>&1 | tail -2
cargo test --workspace 2>&1 | grep -E "test result|FAILED"
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --all -- --check && echo FMT_OK
cargo clippy -p keyflow-app --target x86_64-pc-windows-gnu --all-targets -- -D warnings 2>&1 | tail -2
```
Expected: semua lolos. Salin keluaran nyata ke ringkasan akhir; jangan menyatakan "lolos" tanpa melihatnya.

- [ ] **Step 6: Commit**

```bash
git add docs AGENTS.md
git commit -m "$(cat <<'EOF'
docs: dokumentasi GUI pengaturan tahap 1 dan checklist uji manual

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 7: Ringkasan akhir (sesuai CLAUDE.md "Cara bekerja")**

Laporkan: apa yang selesai, apa yang diuji (jumlah test per crate), dan apa yang **belum** (uji manual GUI per OS, terutama tray Windows dan Wayland/macOS, plus hasil CI Linux/macOS/Windows setelah push).

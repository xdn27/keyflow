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

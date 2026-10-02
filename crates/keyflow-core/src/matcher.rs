//! Matcher konteks dan tabel rule terkompilasi untuk KeyFlow (T1.4).
//!
//! Mencocokkan input tombol terhadap 3 lapis konteks:
//! 1. Aplikasi (`app`): file manager umum atau nama proses spesifik.
//! 2. Lokasi (`path`): pola glob path folder aktif.
//! 3. Seleksi (`selection`): tipe file terpilih (image, video, doc, ext:[...]).
//!
//! Mendukung resolusi spesifisitas:
//! Path tanpa wildcard > Path dengan wildcard > Tanpa filter path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::GlobMatcher;

use crate::config::{ActionType, Config, KeyCombo, OnConflict, RuleConfig, ThenAction};

/// Representasi konteks runtime saat sebuah tombol ditekan.
#[derive(Debug, Clone, Default)]
pub struct RuntimeContext<'a> {
    pub process_name: &'a str,
    pub current_folder: Option<&'a Path>,
    pub selected_items: &'a [PathBuf],
}

/// Rule yang berhasil dicocokkan dengan konteks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedRule {
    pub profile_name: String,
    pub key: KeyCombo,
    pub action: ActionType,
    pub to: Option<String>,
    pub template: Option<String>,
    pub then: Option<ThenAction>,
    pub dry_run: bool,
    pub on_conflict: OnConflict,
    pub create_missing_dirs: bool,
}

#[derive(Clone)]
struct PathFilter {
    base_exact: Option<String>,
    matcher: GlobMatcher,
}

impl PathFilter {
    fn new(pattern: &str) -> Option<Self> {
        let normalized = pattern.replace('\\', "/");
        let base_exact = if normalized.ends_with("/**") {
            normalized
                .strip_suffix("/**")
                .map(|s| s.trim_end_matches('/').to_string())
        } else {
            None
        };

        let glob = globset::GlobBuilder::new(&normalized)
            .case_insensitive(true)
            .literal_separator(false)
            .build()
            .ok()?
            .compile_matcher();

        Some(Self {
            base_exact,
            matcher: glob,
        })
    }

    fn matches(&self, path_str: &str) -> bool {
        let norm_path = path_str.replace('\\', "/");
        let trimmed = norm_path.trim_end_matches('/');

        if let Some(ref base) = self.base_exact {
            if trimmed.eq_ignore_ascii_case(base) {
                return true;
            }
        }

        if self.matcher.is_match(trimmed) {
            return true;
        }

        self.matcher.is_match(format!("{trimmed}/"))
    }
}

/// Kategori spesifisitas path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PathSpecificity {
    /// Tidak ada filter path (paling umum)
    None = 0,
    /// Path dengan karakter wildcard (*, ?, **)
    Wildcard = 1,
    /// Path spesifik pasti tanpa wildcard (paling spesifik)
    Exact = 2,
}

/// Rule yang sudah dikompilasi ke memori untuk evaluasi kilat (sub-milidetik).
#[derive(Clone)]
struct CompiledCandidate {
    profile_name: String,
    profile_enabled: bool,
    dry_run: bool,
    on_conflict: OnConflict,
    create_missing_dirs: bool,
    rule: RuleConfig,
    key_combo: KeyCombo,
    app_filter: String,
    path_filter: Option<PathFilter>,
    path_specificity: PathSpecificity,
    selection_filter: Option<SelectionFilter>,
}

#[derive(Clone, Debug)]
enum SelectionFilter {
    Any,
    Image,
    Video,
    Audio,
    Document,
    ExtList(Vec<String>),
}

impl SelectionFilter {
    fn parse(s: &str) -> Self {
        let lower = s.trim().to_lowercase();
        match lower.as_str() {
            "any" => Self::Any,
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "document" => Self::Document,
            _ if lower.starts_with("ext:[") && lower.ends_with(']') => {
                let inner = &lower[5..lower.len() - 1];
                let exts = inner
                    .split(',')
                    .map(|e| e.trim().trim_start_matches('.').to_string())
                    .filter(|e| !e.is_empty())
                    .collect();
                Self::ExtList(exts)
            }
            _ => Self::Any,
        }
    }

    fn matches(&self, items: &[PathBuf]) -> bool {
        if items.is_empty() {
            // Jika filter adalah Any, item kosong tetap lolos (mis. rule Undo)
            return matches!(self, Self::Any);
        }

        // Semua item terpilih harus cocok dengan filter
        for item in items {
            let ext = item
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            let matched = match self {
                Self::Any => true,
                Self::Image => matches!(
                    ext.as_str(),
                    "jpg"
                        | "jpeg"
                        | "png"
                        | "gif"
                        | "webp"
                        | "bmp"
                        | "svg"
                        | "tiff"
                        | "tif"
                        | "heic"
                        | "avif"
                ),
                Self::Video => matches!(
                    ext.as_str(),
                    "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v"
                ),
                Self::Audio => matches!(
                    ext.as_str(),
                    "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus"
                ),
                Self::Document => matches!(
                    ext.as_str(),
                    "pdf"
                        | "doc"
                        | "docx"
                        | "xls"
                        | "xlsx"
                        | "ppt"
                        | "pptx"
                        | "txt"
                        | "md"
                        | "csv"
                        | "rtf"
                ),
                Self::ExtList(allowed) => allowed.iter().any(|a| a == &ext),
            };

            if !matched {
                return false;
            }
        }

        true
    }
}

/// Tabel matcher konteks terkompilasi.
pub struct ContextMatcher {
    rules_by_key: HashMap<KeyCombo, Vec<CompiledCandidate>>,
}

impl ContextMatcher {
    /// Mengompilasi Config ke tabel rule cepat.
    pub fn compile(config: &Config) -> Self {
        let mut rules_by_key: HashMap<KeyCombo, Vec<CompiledCandidate>> = HashMap::new();

        for profile in &config.profiles {
            let dry_run = profile.dry_run.unwrap_or(config.settings.dry_run);
            let on_conflict = config.settings.on_conflict;
            let create_missing_dirs = config.settings.create_missing_dirs;

            let app_filter = profile.context.app.to_lowercase();
            let (path_filter, path_specificity) = match &profile.context.path {
                Some(p) => {
                    let has_wildcard = p.contains('*') || p.contains('?');
                    let spec = if has_wildcard {
                        PathSpecificity::Wildcard
                    } else {
                        PathSpecificity::Exact
                    };
                    let filter = PathFilter::new(p);
                    (filter, spec)
                }
                None => (None, PathSpecificity::None),
            };

            let selection_filter = profile
                .context
                .selection
                .as_deref()
                .map(SelectionFilter::parse);

            for rule in &profile.rules {
                let Ok(key_combo) = KeyCombo::parse(&rule.key) else {
                    continue;
                };

                let candidate = CompiledCandidate {
                    profile_name: profile.name.clone(),
                    profile_enabled: profile.enabled,
                    dry_run,
                    on_conflict,
                    create_missing_dirs,
                    rule: rule.clone(),
                    key_combo: key_combo.clone(),
                    app_filter: app_filter.clone(),
                    path_filter: path_filter.clone(),
                    path_specificity,
                    selection_filter: selection_filter.clone(),
                };

                rules_by_key.entry(key_combo).or_default().push(candidate);
            }
        }

        // Urutkan kandidat per tombol berdasarkan spesifisitas menurun (paling spesifik lebih dulu)
        for candidates in rules_by_key.values_mut() {
            candidates.sort_by_key(|a| std::cmp::Reverse(a.path_specificity));
        }

        Self { rules_by_key }
    }

    /// Mencocokkan key event dengan konteks aktif saat ini.
    /// Mengembalikan Some(MatchedRule) bila cocok, atau None bila tidak cocok (fail-open / passthrough).
    pub fn match_rule(&self, key: &KeyCombo, ctx: &RuntimeContext<'_>) -> Option<MatchedRule> {
        let candidates = self.rules_by_key.get(key)?;

        for cand in candidates {
            // Lewati jika profil dimatikan
            if !cand.profile_enabled {
                continue;
            }

            // 1. Cek aplikasi (app)
            if !matches_app(&cand.app_filter, ctx.process_name) {
                continue;
            }

            // 2. Cek lokasi folder aktif (path)
            if let Some(ref filter) = cand.path_filter {
                let Some(current_folder) = ctx.current_folder else {
                    continue;
                };
                let folder_str = current_folder.to_string_lossy();
                if !filter.matches(&folder_str) {
                    continue;
                }
            }

            // 3. Cek seleksi (selection)
            if let Some(ref sel_filter) = cand.selection_filter {
                if !sel_filter.matches(ctx.selected_items) {
                    continue;
                }
            }

            // Aksi file (Move/Copy/Trash/Rename) membutuhkan minimal satu item terpilih.
            // Jika tidak ada item yang dipilih, rule tidak dieksekusi agar tombol tidak ditelan sia-sia (fail-open).
            if cand.rule.action != ActionType::Undo && ctx.selected_items.is_empty() {
                continue;
            }

            // Semua kriteria terpenuhi: rule terpilih!
            return Some(MatchedRule {
                profile_name: cand.profile_name.clone(),
                key: cand.key_combo.clone(),
                action: cand.rule.action.clone(),
                to: cand.rule.to.clone(),
                template: cand.rule.template.clone(),
                then: cand.rule.then.clone(),
                dry_run: cand.dry_run,
                on_conflict: cand.on_conflict,
                create_missing_dirs: cand.create_missing_dirs,
            });
        }

        None
    }
}

/// Memeriksa apakah process_name cocok dengan filter app.
fn matches_app(filter: &str, process_name: &str) -> bool {
    let proc_lower = process_name.to_lowercase();
    let filter_lower = filter.to_lowercase();

    if filter_lower == "file_manager" {
        // Daftar file manager standar lintas sistem operasi
        matches!(
            proc_lower.as_str(),
            "explorer.exe"
                | "explorer"
                | "cabinetwclass"
                | "finder"
                | "nautilus"
                | "dolphin"
                | "thunar"
                | "nemo"
                | "pcmanfm"
                | "caja"
        )
    } else {
        // Nama proses spesifik (exact match atau kecocokan ekstensi .exe)
        proc_lower == filter_lower
            || proc_lower == format!("{filter_lower}.exe")
            || proc_lower.strip_suffix(".exe") == Some(filter_lower.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    #[test]
    fn test_matcher_specificity() {
        let yaml = r#"
version: 1
profiles:
  - name: "Umum"
    context:
      app: file_manager
    rules:
      - key: "1"
        action: move
        to: "/dest/general"

  - name: "Khusus Wildcard"
    context:
      app: file_manager
      path: "/photos/**"
    rules:
      - key: "1"
        action: move
        to: "/dest/wildcard"

  - name: "Khusus Pasti"
    context:
      app: file_manager
      path: "/photos/2026"
    rules:
      - key: "1"
        action: move
        to: "/dest/exact"
"#;
        let cfg = Config::from_yaml(yaml).unwrap();
        let matcher = ContextMatcher::compile(&cfg);
        let key = KeyCombo::parse("1").unwrap();

        let dummy_item = [PathBuf::from("/any/file.txt")];

        // 1. Folder exact /photos/2026 harus memilih "Khusus Pasti"
        let ctx_exact = RuntimeContext {
            process_name: "explorer.exe",
            current_folder: Some(Path::new("/photos/2026")),
            selected_items: &dummy_item,
        };
        let m1 = matcher.match_rule(&key, &ctx_exact).unwrap();
        assert_eq!(m1.profile_name, "Khusus Pasti");

        // 2. Folder /photos/subfolder harus memilih "Khusus Wildcard"
        let ctx_wildcard = RuntimeContext {
            process_name: "explorer.exe",
            current_folder: Some(Path::new("/photos/subfolder")),
            selected_items: &dummy_item,
        };
        let m2 = matcher.match_rule(&key, &ctx_wildcard).unwrap();
        assert_eq!(m2.profile_name, "Khusus Wildcard");

        // 3. Folder /other harus memilih "Umum"
        let ctx_general = RuntimeContext {
            process_name: "explorer.exe",
            current_folder: Some(Path::new("/other")),
            selected_items: &dummy_item,
        };
        let m3 = matcher.match_rule(&key, &ctx_general).unwrap();
        assert_eq!(m3.profile_name, "Umum");

        // 4. Aplikasi lain (mis. Notepad) -> None (PassThrough)
        let ctx_notepad = RuntimeContext {
            process_name: "notepad.exe",
            current_folder: Some(Path::new("/photos/2026")),
            selected_items: &dummy_item,
        };
        assert!(matcher.match_rule(&key, &ctx_notepad).is_none());
    }

    #[test]
    fn test_selection_filter() {
        let yaml = r#"
version: 1
profiles:
  - name: "Foto Saja"
    context:
      app: file_manager
      selection: image
    rules:
      - key: "1"
        action: move
        to: "/dest/images"
"#;
        let cfg = Config::from_yaml(yaml).unwrap();
        let matcher = ContextMatcher::compile(&cfg);
        let key = KeyCombo::parse("1").unwrap();

        let ctx_img = RuntimeContext {
            process_name: "nautilus",
            current_folder: None,
            selected_items: &[PathBuf::from("/path/to/foto.jpg")],
        };
        assert!(matcher.match_rule(&key, &ctx_img).is_some());

        let ctx_doc = RuntimeContext {
            process_name: "nautilus",
            current_folder: None,
            selected_items: &[PathBuf::from("/path/to/data.pdf")],
        };
        assert!(matcher.match_rule(&key, &ctx_doc).is_none());
    }

    #[test]
    fn test_empty_selection_fail_open() {
        let yaml = r#"
version: 1
profiles:
  - name: "Aksi File dan Undo"
    context:
      app: file_manager
    rules:
      - key: "1"
        action: move
        to: "/dest"
      - key: "Ctrl+Z"
        action: undo
"#;
        let cfg = Config::from_yaml(yaml).unwrap();
        let matcher = ContextMatcher::compile(&cfg);
        let key_move = KeyCombo::parse("1").unwrap();
        let key_undo = KeyCombo::parse("Ctrl+Z").unwrap();

        // Ketika tidak ada item yang dipilih di file manager:
        let ctx_empty = RuntimeContext {
            process_name: "explorer.exe",
            current_folder: Some(Path::new("/some/folder")),
            selected_items: &[],
        };

        // Aksi Move harus None (fail-open agar tombol '1' tidak ditelan sia-sia)
        assert!(matcher.match_rule(&key_move, &ctx_empty).is_none());

        // Aksi Undo tetap lolos meskipun selected_items kosong
        let undo_matched = matcher.match_rule(&key_undo, &ctx_empty);
        assert!(undo_matched.is_some());
        assert_eq!(undo_matched.unwrap().action, ActionType::Undo);
    }
}

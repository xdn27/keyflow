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

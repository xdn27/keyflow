//! Titik masuk aplikasi KeyFlow (T2.1–T2.8).

// Build rilis Windows berjalan tanpa jendela konsol (aplikasi tray). Build debug
// tetap memakai konsol agar log `tracing` terlihat saat pengembangan.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

pub mod settings_ui;
pub mod worker;

#[cfg(target_os = "windows")]
pub mod app_windows;
#[cfg(target_os = "windows")]
pub mod tray;

#[cfg(target_os = "macos")]
pub mod app_macos;

#[cfg(target_os = "linux")]
pub mod app_linux;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Subperintah opsional; tanpa argumen, perilaku aplikasi tidak berubah.
    if let Some(command) = std::env::args().nth(1) {
        return match command.as_str() {
            "settings" => settings_ui::run(settings_ui::default_config_path()?),
            other => anyhow::bail!("Perintah tidak dikenal: {other}. Perintah tersedia: settings"),
        };
    }

    #[cfg(target_os = "windows")]
    {
        app_windows::run()
    }

    #[cfg(target_os = "macos")]
    {
        app_macos::run()
    }

    #[cfg(target_os = "linux")]
    {
        app_linux::run()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        tracing::info!("KeyFlow v{} dimulai", env!("CARGO_PKG_VERSION"));
        tracing::info!(
            "Target platform saat ini ({}) belum didukung secara penuh untuk GUI/daemon.",
            std::env::consts::OS
        );
        Ok(())
    }
}

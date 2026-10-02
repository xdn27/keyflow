//! Titik masuk aplikasi KeyFlow (T2.1–T2.8).

pub mod worker;

#[cfg(target_os = "windows")]
pub mod app_windows;
#[cfg(target_os = "windows")]
pub mod tray;

#[cfg(target_os = "macos")]
pub mod app_macos;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[cfg(target_os = "windows")]
    {
        app_windows::run()
    }

    #[cfg(target_os = "macos")]
    {
        app_macos::run()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        tracing::info!("KeyFlow v{} dimulai", env!("CARGO_PKG_VERSION"));
        tracing::info!(
            "Target platform saat ini ({}) belum didukung secara penuh untuk GUI/daemon.",
            std::env::consts::OS
        );
        tracing::info!("Dukungan Linux (X11) sedang dalam roadmap pengembangan berikutnya (M4).");
        Ok(())
    }
}

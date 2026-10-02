//! Titik masuk aplikasi KeyFlow (T2.1–T2.8).

pub mod worker;

#[cfg(target_os = "windows")]
pub mod app_windows;
#[cfg(target_os = "windows")]
pub mod tray;

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

    #[cfg(not(target_os = "windows"))]
    {
        tracing::info!("KeyFlow v{} dimulai", env!("CARGO_PKG_VERSION"));
        tracing::info!(
            "Target platform saat ini ({}) belum didukung secara penuh untuk GUI/tray.",
            std::env::consts::OS
        );
        tracing::info!(
            "Dukungan macOS dan Linux sedang dalam roadmap pengembangan berikutnya (M3/M4)."
        );
        Ok(())
    }
}

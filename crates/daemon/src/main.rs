use anyhow::Result;
use clap::Parser;
use linux_scroll_fix_core::Config;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod linux;

#[derive(Parser)]
#[command(version, about = "Precise, smooth mouse-wheel scrolling for Linux")]
struct Args {
    #[arg(long, default_value = "/etc/linux-scroll-fix/config.toml")]
    config: PathBuf,
    #[arg(long)]
    check_config: bool,
    /// List safe wheel devices and exit.
    #[arg(long)]
    discover: bool,
    /// Explicit evdev source, for example /dev/input/event4.
    #[arg(long, conflicts_with_all = ["auto_device", "discover"])]
    device: Option<PathBuf>,
    /// Use the source automatically when exactly one safe wheel device is found.
    #[arg(long, conflicts_with_all = ["device", "discover"])]
    auto_device: bool,
    /// Exclusively capture the source. Required for actual conversion.
    #[arg(long)]
    grab: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "linux_scroll_fix=info".into()),
        )
        .init();
    let args = Args::parse();
    let config = if args.config.exists() {
        Config::load(&args.config)?
    } else {
        tracing::warn!(path = %args.config.display(), "config not found; using defaults");
        Config::default()
    };
    if args.check_config {
        println!("configuration is valid");
        return Ok(());
    }
    run(
        config,
        args.discover,
        args.device,
        args.auto_device,
        args.grab,
    )
}

#[cfg(target_os = "linux")]
fn run(
    config: Config,
    discover: bool,
    device: Option<PathBuf>,
    auto_device: bool,
    grab: bool,
) -> Result<()> {
    if discover {
        for candidate in linux::discover(&config)? {
            println!(
                "{}\t{}\t{:04x}:{:04x}",
                candidate.path.display(),
                candidate.name,
                candidate.vendor,
                candidate.product
            );
        }
        return Ok(());
    }
    let path = if auto_device {
        let candidates = linux::discover(&config)?;
        match candidates.as_slice() {
            [candidate] => {
                tracing::info!(
                    device = %candidate.path.display(),
                    name = %candidate.name,
                    "automatically selected the only safe wheel device"
                );
                candidate.path.clone()
            }
            [] => anyhow::bail!("automatic selection found no safe wheel device"),
            _ => {
                let choices = candidates
                    .iter()
                    .map(|candidate| format!("{} ({})", candidate.path.display(), candidate.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "automatic selection requires exactly one safe wheel device; found {}: {}",
                    candidates.len(),
                    choices
                )
            }
        }
    } else {
        device.ok_or_else(|| {
            anyhow::anyhow!(
                "select a source with --device after checking --discover, or use --auto-device"
            )
        })?
    };
    linux::run(config, &path, grab)
}

#[cfg(not(target_os = "linux"))]
fn run(
    _config: Config,
    _discover: bool,
    _device: Option<PathBuf>,
    _auto_device: bool,
    _grab: bool,
) -> Result<()> {
    anyhow::bail!("the daemon input backend requires Linux")
}

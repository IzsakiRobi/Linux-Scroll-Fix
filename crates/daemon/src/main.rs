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
    #[arg(long)]
    device: Option<PathBuf>,
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
    run(config, args.discover, args.device, args.grab)
}

#[cfg(target_os = "linux")]
fn run(config: Config, discover: bool, device: Option<PathBuf>, grab: bool) -> Result<()> {
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
    let path = device.ok_or_else(|| {
        anyhow::anyhow!("select a source with --device after checking --discover")
    })?;
    linux::run(config, &path, grab)
}

#[cfg(not(target_os = "linux"))]
fn run(_config: Config, _discover: bool, _device: Option<PathBuf>, _grab: bool) -> Result<()> {
    anyhow::bail!("the daemon input backend requires Linux")
}

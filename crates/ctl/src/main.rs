use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use linux_scroll_fix_core::{Config, Direction};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

const CONFIG_PATH: &str = "/etc/linux-scroll-fix/config.toml";
const PRECISE_PROFILE_PATH: &str = "/usr/local/share/linux-scroll-fix/profiles/precise.toml";
const SERVICE: &str = "linux-scroll-fix.service";

#[derive(Parser)]
#[command(version, about = "Control Linux Scroll Fix safely")]
struct Args {
    #[command(subcommand)]
    command: ControlCommand,
}

#[derive(Subcommand)]
enum ControlCommand {
    /// Print machine-readable service and configuration state.
    Status,
    /// Enable and start smooth scrolling.
    Enable,
    /// Stop and disable smooth scrolling.
    Disable,
    /// Set both scroll axes to the selected direction.
    SetDirection { direction: DirectionArg },
    /// Apply a built-in profile while preserving scroll direction.
    SetProfile { profile: ProfileArg },
}

#[derive(Clone, Copy, ValueEnum)]
enum DirectionArg {
    Traditional,
    Natural,
}

impl From<DirectionArg> for Direction {
    fn from(value: DirectionArg) -> Self {
        match value {
            DirectionArg::Traditional => Self::Traditional,
            DirectionArg::Natural => Self::Natural,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileArg {
    Precise,
}

fn main() -> Result<()> {
    match Args::parse().command {
        ControlCommand::Status => print_status(),
        ControlCommand::Enable => {
            require_root()?;
            systemctl(&["enable", "--now", SERVICE])
        }
        ControlCommand::Disable => {
            require_root()?;
            systemctl(&["disable", "--now", SERVICE])
        }
        ControlCommand::SetDirection { direction } => {
            require_root()?;
            let path = Path::new(CONFIG_PATH);
            let mut config = Config::load(path)?;
            config.vertical.direction = direction.into();
            config.horizontal.direction = direction.into();
            replace_config_and_restart(path, &config)
        }
        ControlCommand::SetProfile { profile } => {
            require_root()?;
            apply_profile(profile)
        }
    }
}

fn print_status() -> Result<()> {
    let config = Config::load(Path::new(CONFIG_PATH))?;
    println!("active={}", service_is("is-active"));
    println!("enabled={}", service_is("is-enabled"));
    println!("profile=precise");
    let direction = if config.vertical.direction == config.horizontal.direction {
        match config.vertical.direction {
            Direction::Traditional => "traditional",
            Direction::Natural => "natural",
        }
    } else {
        "mixed"
    };
    println!("direction={direction}");
    Ok(())
}

fn apply_profile(profile: ProfileArg) -> Result<()> {
    let path = Path::new(CONFIG_PATH);
    let current = Config::load(path)?;
    let profile_path = match profile {
        ProfileArg::Precise => Path::new(PRECISE_PROFILE_PATH),
    };
    let mut replacement = Config::load(profile_path)?;
    replacement.vertical.direction = current.vertical.direction;
    replacement.horizontal.direction = current.horizontal.direction;
    replace_config_and_restart(path, &replacement)
}

fn replace_config_and_restart(path: &Path, config: &Config) -> Result<()> {
    config.validate()?;
    let original = fs::read(path).with_context(|| format!("cannot back up {}", path.display()))?;
    let replacement = toml::to_string_pretty(config).context("cannot serialize configuration")?;
    atomic_write(path, replacement.as_bytes())?;

    let should_restart = service_is("is-active") || service_is("is-enabled");
    if should_restart && let Err(error) = systemctl(&["restart", SERVICE]) {
        atomic_write(path, &original).context("cannot restore previous configuration")?;
        let _ = systemctl(&["restart", SERVICE]);
        return Err(error)
            .context("service rejected the new configuration; previous settings restored");
    }
    Ok(())
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("configuration path has no parent directory")?;
    let temporary = temporary_path(path);
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("cannot create {}", temporary.display()))?;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".tmp-{}", std::process::id()));
    PathBuf::from(name)
}

fn require_root() -> Result<()> {
    if fs::metadata("/proc/self")?.uid() != 0 {
        bail!("this operation requires Polkit authorization")
    }
    Ok(())
}

fn service_is(operation: &str) -> bool {
    Command::new("systemctl")
        .args([operation, "--quiet", SERVICE])
        .status()
        .is_ok_and(|status| status.success())
}

fn systemctl(arguments: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .args(arguments)
        .status()
        .context("cannot run systemctl")?;
    if !status.success() {
        bail!("systemctl {} failed", arguments.join(" "))
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::temporary_path;
    use std::path::Path;

    #[test]
    fn temporary_config_stays_next_to_the_target() {
        let path = temporary_path(Path::new("/etc/linux-scroll-fix/config.toml"));
        assert_eq!(path.parent(), Some(Path::new("/etc/linux-scroll-fix")));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("config.toml.tmp-")
        );
    }
}

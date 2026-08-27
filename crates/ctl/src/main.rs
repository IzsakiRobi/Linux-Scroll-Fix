use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use linux_scroll_fix_core::{AxisConfig, Config, Direction};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

const CONFIG_PATH: &str = "/etc/linux-scroll-fix/config.toml";
const PRECISE_PROFILE_PATH: &str = "/usr/local/share/linux-scroll-fix/profiles/precise.toml";
const BALANCED_PROFILE_PATH: &str = "/usr/local/share/linux-scroll-fix/profiles/balanced.toml";
const RAPID_PROFILE_PATH: &str = "/usr/local/share/linux-scroll-fix/profiles/rapid.toml";
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
    /// Apply a linked custom speed level from 0 (slow) to 8 (fast).
    SetSpeed { level: u8 },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ProfileArg {
    Precise,
    Balanced,
    Rapid,
}

impl ProfileArg {
    const ALL: [Self; 3] = [Self::Precise, Self::Balanced, Self::Rapid];

    fn name(self) -> &'static str {
        match self {
            Self::Precise => "precise",
            Self::Balanced => "balanced",
            Self::Rapid => "rapid",
        }
    }

    fn path(self) -> &'static Path {
        match self {
            Self::Precise => Path::new(PRECISE_PROFILE_PATH),
            Self::Balanced => Path::new(BALANCED_PROFILE_PATH),
            Self::Rapid => Path::new(RAPID_PROFILE_PATH),
        }
    }
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
        ControlCommand::SetSpeed { level } => {
            require_root()?;
            apply_speed(level)
        }
    }
}

fn print_status() -> Result<()> {
    let config = Config::load(Path::new(CONFIG_PATH))?;
    println!("active={}", service_is("is-active"));
    println!("enabled={}", service_is("is-enabled"));
    let profile = detect_profile(&config).unwrap_or("custom");
    println!("profile={profile}");
    println!(
        "speed={}",
        detect_speed(&config).map_or("unknown".into(), |level| level.to_string())
    );
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
    let mut replacement = Config::load(profile.path())?;
    replacement.vertical.direction = current.vertical.direction;
    replacement.horizontal.direction = current.horizontal.direction;
    replace_config_and_restart(path, &replacement)
}

fn detect_profile(config: &Config) -> Option<&'static str> {
    if config.custom_speed_level.is_some() {
        return None;
    }
    ProfileArg::ALL.into_iter().find_map(|profile| {
        let mut candidate = Config::load(profile.path()).ok()?;
        candidate.vertical.direction = config.vertical.direction;
        candidate.horizontal.direction = config.horizontal.direction;
        (candidate == *config).then(|| profile.name())
    })
}

#[derive(Clone, Copy)]
struct SpeedAnchor {
    level: u8,
    min_sensitivity: f64,
    max_sensitivity: f64,
    acceleration_start_tps: f64,
    acceleration_end_tps: f64,
    curvature: f64,
    max_pending: f64,
}

const SPEED_ANCHORS: [SpeedAnchor; 5] = [
    SpeedAnchor {
        level: 0,
        min_sensitivity: 6.0,
        max_sensitivity: 120.0,
        acceleration_start_tps: 7.0,
        acceleration_end_tps: 80.0,
        curvature: 1.35,
        max_pending: 720.0,
    },
    SpeedAnchor {
        level: 3,
        min_sensitivity: 10.0,
        max_sensitivity: 180.0,
        acceleration_start_tps: 6.25,
        acceleration_end_tps: 66.6667,
        curvature: 1.25,
        max_pending: 960.0,
    },
    SpeedAnchor {
        level: 4,
        min_sensitivity: 18.0,
        max_sensitivity: 300.0,
        acceleration_start_tps: 5.5,
        acceleration_end_tps: 45.0,
        curvature: 1.1,
        max_pending: 960.0,
    },
    SpeedAnchor {
        level: 5,
        min_sensitivity: 22.0,
        max_sensitivity: 520.0,
        acceleration_start_tps: 4.5,
        acceleration_end_tps: 32.0,
        curvature: 0.9,
        max_pending: 1440.0,
    },
    SpeedAnchor {
        level: 8,
        min_sensitivity: 30.0,
        max_sensitivity: 900.0,
        acceleration_start_tps: 3.5,
        acceleration_end_tps: 22.0,
        curvature: 0.7,
        max_pending: 2200.0,
    },
];

fn apply_speed(level: u8) -> Result<()> {
    if level > 8 {
        bail!("speed level must be between 0 and 8")
    }
    let path = Path::new(CONFIG_PATH);
    let current = Config::load(path)?;
    let precise = Config::load(ProfileArg::Precise.path())?;
    let mut replacement = custom_speed_config(&precise, level);
    replacement.custom_speed_level = Some(level);
    replacement.vertical.direction = current.vertical.direction;
    replacement.horizontal.direction = current.horizontal.direction;
    replace_config_and_restart(path, &replacement)
}

fn detect_speed(config: &Config) -> Option<u8> {
    if let Some(level) = config.custom_speed_level {
        return Some(level);
    }
    let precise = Config::load(ProfileArg::Precise.path()).ok()?;
    (0..=8).find(|level| {
        let mut candidate = custom_speed_config(&precise, *level);
        candidate.vertical.direction = config.vertical.direction;
        candidate.horizontal.direction = config.horizontal.direction;
        candidate == *config
    })
}

fn custom_speed_config(precise: &Config, level: u8) -> Config {
    let mut config = precise.clone();
    let (lower, upper) = speed_segment(level);
    let span = (upper.level - lower.level) as f64;
    let amount = if span == 0.0 {
        0.0
    } else {
        (level - lower.level) as f64 / span
    };
    config.max_pending = mix(lower.max_pending, upper.max_pending, amount);
    tune_axis(&mut config.vertical, lower, upper, amount);
    tune_axis(&mut config.horizontal, lower, upper, amount);
    config
}

fn speed_segment(level: u8) -> (SpeedAnchor, SpeedAnchor) {
    let level = level.min(8);
    for pair in SPEED_ANCHORS.windows(2) {
        if level <= pair[1].level {
            return (pair[0], pair[1]);
        }
    }
    let last = SPEED_ANCHORS[SPEED_ANCHORS.len() - 1];
    (last, last)
}

fn tune_axis(axis: &mut AxisConfig, lower: SpeedAnchor, upper: SpeedAnchor, amount: f64) {
    axis.mmf_min_sensitivity = mix(lower.min_sensitivity, upper.min_sensitivity, amount);
    axis.mmf_max_sensitivity = mix(lower.max_sensitivity, upper.max_sensitivity, amount);
    axis.mmf_acceleration_start_tps = mix(
        lower.acceleration_start_tps,
        upper.acceleration_start_tps,
        amount,
    );
    axis.mmf_acceleration_end_tps = mix(
        lower.acceleration_end_tps,
        upper.acceleration_end_tps,
        amount,
    );
    axis.mmf_curvature = mix(lower.curvature, upper.curvature, amount);
}

fn mix(start: f64, end: f64, amount: f64) -> f64 {
    start + (end - start) * amount
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
    use super::{ProfileArg, custom_speed_config, temporary_path};
    use linux_scroll_fix_core::{Axis, Config, ScrollEngine};
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

    #[test]
    fn built_in_profile_names_and_paths_are_fixed() {
        assert_eq!(ProfileArg::Precise.name(), "precise");
        assert_eq!(ProfileArg::Balanced.name(), "balanced");
        assert_eq!(ProfileArg::Rapid.name(), "rapid");
        assert!(ProfileArg::Balanced.path().ends_with("balanced.toml"));
        assert!(ProfileArg::Rapid.path().ends_with("rapid.toml"));
    }

    #[test]
    fn speed_anchors_preserve_the_built_in_profiles() {
        let precise: Config = toml::from_str(include_str!("../../../config/default.toml")).unwrap();
        let balanced: Config =
            toml::from_str(include_str!("../../../config/balanced.toml")).unwrap();
        let rapid: Config = toml::from_str(include_str!("../../../config/rapid.toml")).unwrap();
        assert_eq!(custom_speed_config(&precise, 3), precise);
        assert_eq!(custom_speed_config(&precise, 4), balanced);
        assert_eq!(custom_speed_config(&precise, 5), rapid);
    }

    #[test]
    fn custom_speed_range_is_monotonic_and_bounded() {
        let precise: Config = toml::from_str(include_str!("../../../config/default.toml")).unwrap();
        let mut previous = custom_speed_config(&precise, 0);
        for level in 1..=8 {
            let current = custom_speed_config(&precise, level);
            current.validate().unwrap();
            assert!(current.vertical.mmf_min_sensitivity > previous.vertical.mmf_min_sensitivity);
            assert!(current.vertical.mmf_max_sensitivity > previous.vertical.mmf_max_sensitivity);
            assert!(
                current.vertical.mmf_acceleration_end_tps
                    < previous.vertical.mmf_acceleration_end_tps
            );
            previous = current;
        }
    }

    #[test]
    fn every_speed_level_keeps_the_first_tick_visible() {
        let precise: Config = toml::from_str(include_str!("../../../config/default.toml")).unwrap();
        let mut previous_distance = 0;
        for level in 0..=8 {
            let mut engine = ScrollEngine::new(custom_speed_config(&precise, level));
            engine.input(Axis::Vertical, 1, 0.0);
            let distance: i32 = (1..=240)
                .map(|frame| engine.frame(frame as f64 * 8.333).vertical)
                .sum();
            assert!(distance > 0, "level {level} swallowed its first tick");
            assert!(
                distance > previous_distance,
                "level {level} did not increase isolated-tick distance"
            );
            previous_distance = distance;
        }
    }
}

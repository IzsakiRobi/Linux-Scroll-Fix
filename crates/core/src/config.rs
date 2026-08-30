use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Traditional,
    Natural,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccelerationCurve {
    Power,
    #[default]
    Bezier,
    MmfCapped,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalyzerMode {
    #[default]
    MedianEma,
    RollingAverage,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AxisConfig {
    pub enabled: bool,
    pub direction: Direction,
    pub distance_per_tick: f64,
    pub acceleration_max: f64,
    pub acceleration_window_ms: f64,
    pub acceleration_curve: AccelerationCurve,
    pub analyzer_mode: AnalyzerMode,
    pub analyzer_samples: u8,
    pub analyzer_smoothing: f64,
    pub curve_exponent: f64,
    pub curve_blend: f64,
    pub bezier_x1: f64,
    pub bezier_y1: f64,
    pub bezier_x2: f64,
    pub bezier_y2: f64,
    pub mmf_min_sensitivity: f64,
    pub mmf_max_sensitivity: f64,
    pub mmf_acceleration_start_tps: f64,
    pub mmf_acceleration_end_tps: f64,
    pub mmf_curvature: f64,
    pub mmf_unit_scale: f64,
}

impl Default for AxisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            direction: Direction::Traditional,
            distance_per_tick: 48.0,
            acceleration_max: 2.3,
            acceleration_window_ms: 160.0,
            acceleration_curve: AccelerationCurve::Bezier,
            analyzer_mode: AnalyzerMode::MedianEma,
            analyzer_samples: 5,
            analyzer_smoothing: 0.35,
            curve_exponent: 1.3,
            curve_blend: 0.65,
            bezier_x1: 0.18,
            bezier_y1: 0.0,
            bezier_x2: 0.35,
            bezier_y2: 1.0,
            mmf_min_sensitivity: 10.0,
            mmf_max_sensitivity: 180.0,
            mmf_acceleration_start_tps: 6.25,
            mmf_acceleration_end_tps: 66.666_7,
            mmf_curvature: 1.25,
            mmf_unit_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub target_hz: u32,
    pub smoothing: f64,
    pub tail_smoothing: f64,
    pub tail_ramp_ms: f64,
    pub stop_epsilon: f64,
    pub gesture_idle_ms: f64,
    pub gesture_prime_units: i32,
    pub max_pending: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_speed_level: Option<u8>,
    pub device_name_patterns: Vec<String>,
    pub vertical: AxisConfig,
    pub horizontal: AxisConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_hz: 120,
            smoothing: 0.08,
            tail_smoothing: 0.045,
            tail_ramp_ms: 200.0,
            stop_epsilon: 2.0,
            gesture_idle_ms: 1500.0,
            gesture_prime_units: 48,
            max_pending: 320.0,
            custom_speed_level: None,
            device_name_patterns: vec![
                "mouse".into(),
                "receiver".into(),
                "vmware".into(),
                "keyd virtual pointer".into(),
            ],
            vertical: AxisConfig::default(),
            horizontal: AxisConfig {
                distance_per_tick: 36.0,
                ..AxisConfig::default()
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Validation(String),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config: Self = toml::from_str(&text)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(30..=1000).contains(&self.target_hz) {
            return Err(ConfigError::Validation(
                "target_hz must be between 30 and 1000".into(),
            ));
        }
        if !(0.0..=5000.0).contains(&self.gesture_idle_ms) {
            return Err(ConfigError::Validation(
                "gesture_idle_ms must be between 0 and 5000".into(),
            ));
        }
        if !(0..=512).contains(&self.gesture_prime_units) {
            return Err(ConfigError::Validation(
                "gesture_prime_units must be between 0 and 512".into(),
            ));
        }
        if self.custom_speed_level.is_some_and(|level| level > 8) {
            return Err(ConfigError::Validation(
                "custom_speed_level must be between 0 and 8".into(),
            ));
        }
        for (name, axis) in [
            ("vertical", &self.vertical),
            ("horizontal", &self.horizontal),
        ] {
            if axis.distance_per_tick <= 0.0 || axis.acceleration_max < 1.0 {
                return Err(ConfigError::Validation(format!(
                    "{name} distance must be positive and acceleration_max >= 1"
                )));
            }
            if !(0.0..=1.0).contains(&axis.curve_blend) || axis.curve_exponent <= 0.0 {
                return Err(ConfigError::Validation(format!("invalid {name} curve")));
            }
            if !(1..=8).contains(&axis.analyzer_samples) {
                return Err(ConfigError::Validation(format!(
                    "{name}.analyzer_samples must be between 1 and 8"
                )));
            }
            if !(0.0..=1.0).contains(&axis.analyzer_smoothing) {
                return Err(ConfigError::Validation(format!(
                    "{name}.analyzer_smoothing must be between 0 and 1"
                )));
            }
            for (point, value) in [
                ("bezier_x1", axis.bezier_x1),
                ("bezier_y1", axis.bezier_y1),
                ("bezier_x2", axis.bezier_x2),
                ("bezier_y2", axis.bezier_y2),
            ] {
                if !(0.0..=1.0).contains(&value) {
                    return Err(ConfigError::Validation(format!(
                        "{name}.{point} must be between 0 and 1"
                    )));
                }
            }
            if axis.mmf_min_sensitivity <= 0.0
                || axis.mmf_max_sensitivity < axis.mmf_min_sensitivity
                || axis.mmf_acceleration_start_tps <= 0.0
                || axis.mmf_acceleration_end_tps <= axis.mmf_acceleration_start_tps
                || !(0.0..=6.0).contains(&axis.mmf_curvature)
                || axis.mmf_unit_scale <= 0.0
            {
                return Err(ConfigError::Validation(format!("invalid {name} MMF curve")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod profile_tests {
    use super::Config;

    fn parse(source: &str) -> Config {
        let config: Config = toml::from_str(source).expect("bundled profile must parse");
        config.validate().expect("bundled profile must validate");
        config
    }

    #[test]
    fn bundled_profiles_share_the_proven_animation() {
        let precise = parse(include_str!("../../../config/default.toml"));
        let balanced = parse(include_str!("../../../config/balanced.toml"));
        let rapid = parse(include_str!("../../../config/rapid.toml"));

        for profile in [&balanced, &rapid] {
            assert_eq!(profile.target_hz, precise.target_hz);
            assert_eq!(profile.smoothing, precise.smoothing);
            assert_eq!(profile.tail_smoothing, precise.tail_smoothing);
            assert_eq!(profile.tail_ramp_ms, precise.tail_ramp_ms);
            assert_eq!(profile.stop_epsilon, precise.stop_epsilon);
            assert_eq!(profile.gesture_prime_units, precise.gesture_prime_units);
        }
        assert!(
            precise.vertical.mmf_min_sensitivity < balanced.vertical.mmf_min_sensitivity
                && balanced.vertical.mmf_min_sensitivity < rapid.vertical.mmf_min_sensitivity
        );
        assert!(
            precise.vertical.mmf_max_sensitivity < balanced.vertical.mmf_max_sensitivity
                && balanced.vertical.mmf_max_sensitivity < rapid.vertical.mmf_max_sensitivity
        );
    }
}

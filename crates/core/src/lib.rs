mod config;
mod engine;

pub use config::{AccelerationCurve, AnalyzerMode, AxisConfig, Config, ConfigError, Direction};
pub use engine::{Axis, Frame, ScrollEngine};

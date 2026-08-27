use crate::{AccelerationCurve, AnalyzerMode, AxisConfig, Config, Direction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Frame {
    pub vertical: i32,
    pub horizontal: i32,
}

#[derive(Debug, Clone, Default)]
struct AxisState {
    pending: f64,
    fractional: f64,
    last_tick_ms: Option<f64>,
    last_input_ms: Option<f64>,
    direction: i8,
    analyzer: TickAnalyzer,
}

#[derive(Debug, Clone, Default)]
struct TickAnalyzer {
    samples: [f64; 8],
    count: usize,
    cursor: usize,
    filtered: Option<f64>,
}

impl TickAnalyzer {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn observe(&mut self, raw_ms: f64, cfg: &AxisConfig) -> f64 {
        if raw_ms >= cfg.acceleration_window_ms {
            self.reset();
            return raw_ms;
        }
        let capacity = cfg.analyzer_samples as usize;
        self.samples[self.cursor] = raw_ms.max(1.0);
        self.cursor = (self.cursor + 1) % capacity;
        self.count = (self.count + 1).min(capacity);

        if cfg.analyzer_mode == AnalyzerMode::RollingAverage {
            return self.samples[..self.count].iter().sum::<f64>() / self.count as f64;
        }

        // The baseline analyzer rejects one anomalous tick with a median,
        // followed by an EMA to prevent stair-stepping.
        let mut sorted = [0.0; 8];
        sorted[..self.count].copy_from_slice(&self.samples[..self.count]);
        sorted[..self.count].sort_by(f64::total_cmp);
        let median = if self.count % 2 == 0 {
            (sorted[self.count / 2 - 1] + sorted[self.count / 2]) * 0.5
        } else {
            sorted[self.count / 2]
        };
        let filtered = self.filtered.map_or(median, |previous| {
            previous + (median - previous) * cfg.analyzer_smoothing
        });
        self.filtered = Some(filtered);
        filtered
    }
}

pub struct ScrollEngine {
    config: Config,
    vertical: AxisState,
    horizontal: AxisState,
}

impl ScrollEngine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            vertical: AxisState::default(),
            horizontal: AxisState::default(),
        }
    }
    pub fn replace_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn input(&mut self, axis: Axis, ticks: i32, now_ms: f64) {
        if ticks == 0 {
            return;
        }
        let cfg = match axis {
            Axis::Vertical => self.config.vertical.clone(),
            Axis::Horizontal => self.config.horizontal.clone(),
        };
        if !cfg.enabled {
            return;
        }
        let state = match axis {
            Axis::Vertical => &mut self.vertical,
            Axis::Horizontal => &mut self.horizontal,
        };
        let sign = ticks.signum() as i8;
        let interval = state.last_tick_ms.map(|last| (now_ms - last).max(1.0));
        if state.direction != 0 && state.direction != sign {
            state.pending = 0.0;
            state.fractional = 0.0;
            state.analyzer.reset();
        }
        state.direction = sign;
        state.last_tick_ms = Some(now_ms);
        state.last_input_ms = Some(now_ms);
        let analyzed_interval = interval.map(|dt| state.analyzer.observe(dt, &cfg));
        let direction = if cfg.direction == Direction::Natural {
            -1.0
        } else {
            1.0
        };
        state.pending += ticks as f64 * tick_distance(&cfg, analyzed_interval) * direction;
        state.pending = state
            .pending
            .clamp(-self.config.max_pending, self.config.max_pending);
    }

    pub fn frame(&mut self, now_ms: f64) -> Frame {
        Frame {
            vertical: advance(&self.config, &mut self.vertical, now_ms),
            horizontal: advance(&self.config, &mut self.horizontal, now_ms),
        }
    }

    pub fn active(&self) -> bool {
        self.vertical.pending != 0.0 || self.horizontal.pending != 0.0
    }
}

fn acceleration(cfg: &AxisConfig, interval_ms: f64) -> f64 {
    let x = (1.0 - interval_ms / cfg.acceleration_window_ms).clamp(0.0, 1.0);
    let curve = match cfg.acceleration_curve {
        AccelerationCurve::Power => {
            let power = x.powf(cfg.curve_exponent);
            let smoothstep = x * x * (3.0 - 2.0 * x);
            power * (1.0 - cfg.curve_blend) + smoothstep * cfg.curve_blend
        }
        AccelerationCurve::Bezier => cubic_bezier_y_for_x(
            x,
            cfg.bezier_x1,
            cfg.bezier_y1,
            cfg.bezier_x2,
            cfg.bezier_y2,
        ),
        AccelerationCurve::MmfCapped => {
            let ticks_per_second = 1000.0 / interval_ms.max(1.0);
            mmf_capped_normalized(cfg, ticks_per_second)
        }
    };
    1.0 + (cfg.acceleration_max - 1.0) * curve
}

fn tick_distance(cfg: &AxisConfig, interval_ms: Option<f64>) -> f64 {
    if cfg.acceleration_curve == AccelerationCurve::MmfCapped {
        let normalized = interval_ms.map_or(0.0, |interval| {
            mmf_capped_normalized(cfg, 1000.0 / interval.max(1.0))
        });
        let sensitivity = cfg.mmf_min_sensitivity
            + (cfg.mmf_max_sensitivity - cfg.mmf_min_sensitivity) * normalized;
        sensitivity * cfg.mmf_unit_scale
    } else {
        interval_ms.map_or(cfg.distance_per_tick, |interval| {
            cfg.distance_per_tick * acceleration(cfg, interval)
        })
    }
}

fn mmf_capped_normalized(cfg: &AxisConfig, ticks_per_second: f64) -> f64 {
    let x = ((ticks_per_second - cfg.mmf_acceleration_start_tps)
        / (cfg.mmf_acceleration_end_tps - cfg.mmf_acceleration_start_tps))
        .clamp(0.0, 1.0);
    mmf_bezier(x, cfg.mmf_curvature)
}

fn mmf_bezier(x: f64, curvature: f64) -> f64 {
    let degree = curvature + 1.0;
    let count = degree.ceil() as usize + 1;
    let mut xs = [0.0; 8];
    let mut ys = [1.0; 8];
    ys[0] = 0.0;
    for (index, value) in xs[..count].iter_mut().enumerate() {
        *value = (index as f64 / degree).min(1.0);
    }
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        if de_casteljau(xs, count, t) < x {
            low = t;
        } else {
            high = t;
        }
    }
    de_casteljau(ys, count, (low + high) * 0.5)
}

fn de_casteljau(mut points: [f64; 8], count: usize, t: f64) -> f64 {
    for level in (1..count).rev() {
        for index in 0..level {
            points[index] = points[index] * (1.0 - t) + points[index + 1] * t;
        }
    }
    points[0]
}

fn cubic_bezier_y_for_x(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    // The x component is monotonic for control points in [0, 1]. Solve x(t)
    // with bounded bisection; deterministic work is preferable in an input loop.
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        if cubic(t, x1, x2) < x {
            low = t;
        } else {
            high = t;
        }
    }
    cubic((low + high) * 0.5, y1, y2)
}

fn cubic(t: f64, p1: f64, p2: f64) -> f64 {
    let one_minus = 1.0 - t;
    3.0 * one_minus * one_minus * t * p1 + 3.0 * one_minus * t * t * p2 + t * t * t
}

fn advance(cfg: &Config, state: &mut AxisState, now_ms: f64) -> i32 {
    if state.pending == 0.0 {
        return 0;
    }
    if state.pending.abs() <= cfg.stop_epsilon {
        // Below the visible motion threshold, dissipate the remaining energy.
        // Flushing it into one frame causes a final kick; conserving it over
        // several frames causes sparse one-pixel dribble.
        state.pending = 0.0;
        state.fractional = 0.0;
        return 0;
    }
    let quiet_ms = now_ms - state.last_input_ms.unwrap_or(now_ms);
    let ramp = (quiet_ms / cfg.tail_ramp_ms).clamp(0.0, 1.0);
    let smoothing = cfg.smoothing + (cfg.tail_smoothing - cfg.smoothing) * ramp;
    let delta = state.pending * smoothing;
    state.pending -= delta;
    state.fractional += delta;
    let whole = state.fractional.trunc() as i32;
    state.fractional -= whole as f64;
    whole
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_ticks_accelerate_but_are_bounded() {
        let cfg = AxisConfig::default();
        assert!(acceleration(&cfg, 15.0) > acceleration(&cfg, 120.0));
        assert!(acceleration(&cfg, 1.0) <= cfg.acceleration_max);
    }

    #[test]
    fn bezier_curve_has_exact_endpoints_and_is_monotonic() {
        let cfg = AxisConfig::default();
        let mut previous = 1.0;
        for interval in (1..=160).rev() {
            let value = acceleration(&cfg, interval as f64);
            assert!(value >= previous - 0.001);
            previous = value;
        }
        assert!((acceleration(&cfg, 160.0) - 1.0).abs() < 0.001);
        assert!(acceleration(&cfg, 0.0) <= cfg.acceleration_max + 0.001);
    }

    #[test]
    fn analyzer_rejects_a_single_fast_outlier() {
        let cfg = AxisConfig::default();
        let mut analyzer = TickAnalyzer::default();
        for interval in [40.0, 42.0, 39.0, 2.0] {
            analyzer.observe(interval, &cfg);
        }
        let filtered = analyzer.observe(41.0, &cfg);
        assert!(
            filtered > 30.0,
            "outlier pulled interval down to {filtered}"
        );
    }

    #[test]
    fn analyzer_resets_after_a_quiet_gap() {
        let cfg = AxisConfig::default();
        let mut analyzer = TickAnalyzer::default();
        analyzer.observe(12.0, &cfg);
        assert_eq!(analyzer.observe(200.0, &cfg), 200.0);
        assert_eq!(analyzer.count, 0);
        assert_eq!(analyzer.filtered, None);
    }

    #[test]
    fn mmf_medium_precision_uses_source_sensitivities() {
        let cfg = AxisConfig {
            acceleration_curve: AccelerationCurve::MmfCapped,
            ..AxisConfig::default()
        };
        assert!((tick_distance(&cfg, Some(160.0)) - 10.0).abs() < 0.01);
        assert!((tick_distance(&cfg, Some(15.0)) - 180.0).abs() < 0.1);
        assert!((tick_distance(&cfg, None) - 10.0).abs() < 0.01);
    }

    #[test]
    fn mmf_rolling_average_matches_three_tick_mean() {
        let cfg = AxisConfig {
            analyzer_mode: AnalyzerMode::RollingAverage,
            analyzer_samples: 3,
            ..AxisConfig::default()
        };
        let mut analyzer = TickAnalyzer::default();
        analyzer.observe(30.0, &cfg);
        analyzer.observe(60.0, &cfg);
        assert_eq!(analyzer.observe(90.0, &cfg), 60.0);
    }

    #[test]
    fn direction_change_discards_old_tail() {
        let mut e = ScrollEngine::new(Config::default());
        e.input(Axis::Vertical, 1, 0.0);
        e.input(Axis::Vertical, -1, 10.0);
        assert!(e.frame(11.0).vertical <= 0);
    }

    #[test]
    fn output_is_monotonic_and_dissipates_only_the_invisible_tail() {
        let mut e = ScrollEngine::new(Config::default());
        e.input(Axis::Vertical, 1, 0.0);
        let mut total = 0;
        for i in 1..2000 {
            let x = e.frame(i as f64 * 8.33).vertical;
            assert!(x >= 0);
            total += x;
        }
        assert!(
            (46..=48).contains(&total),
            "unexpected output distance: {total}"
        );
    }

    #[test]
    fn stopping_does_not_flush_the_tail_into_a_final_kick() {
        let config = Config {
            stop_epsilon: 2.0,
            ..Config::default()
        };
        let mut state = AxisState {
            pending: 1.9,
            fractional: 0.8,
            last_input_ms: Some(0.0),
            ..AxisState::default()
        };
        assert_eq!(advance(&config, &mut state, 1000.0), 0);
        assert_eq!(state.pending, 0.0);
        assert_eq!(state.fractional, 0.0);
    }
}

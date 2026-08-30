use anyhow::{Context, Result, bail};
use evdev::{
    AbsInfo, AbsoluteAxisCode as Abs, AttributeSet, BusType, Device, EventType, InputEvent,
    InputId, KeyCode as Key, PropType, RelativeAxisCode as Rel, UinputAbsSetup, enumerate,
    uinput::VirtualDevice,
};
use linux_scroll_fix_core::{Axis, Config, ScrollEngine};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_PREFIX: &str = "Linux Scroll Fix";
const KEYD_POINTER_NAME: &str = "keyd virtual pointer";
const KEYD_VENDOR: u16 = 0x0fac;
const KEYD_POINTER_PRODUCT: u16 = 0x1ade;

#[derive(Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
}

pub fn discover(config: &Config) -> Result<Vec<Candidate>> {
    let mut result = Vec::new();
    for (path, mut device) in enumerate() {
        let name = device.name().unwrap_or("Unnamed input device").to_owned();
        if name.starts_with(OUTPUT_PREFIX) {
            continue;
        }
        let Some(rel) = device.supported_relative_axes() else {
            continue;
        };
        if !(rel.contains(Rel::REL_X) && rel.contains(Rel::REL_Y) && rel.contains(Rel::REL_WHEEL)) {
            continue;
        }
        let lower = name.to_lowercase();
        let id = device.input_id();
        let is_keyd_pointer = name == KEYD_POINTER_NAME
            && id.vendor() == KEYD_VENDOR
            && id.product() == KEYD_POINTER_PRODUCT;
        if !is_keyd_pointer
            && !config.device_name_patterns.is_empty()
            && !config
                .device_name_patterns
                .iter()
                .any(|p| lower.contains(&p.to_lowercase()))
        {
            continue;
        }
        if let Err(error) = device.grab() {
            tracing::debug!(
                device = %path.display(),
                %name,
                %error,
                "skipping wheel device already captured by another process"
            );
            continue;
        }
        if let Err(error) = device.ungrab() {
            tracing::warn!(
                device = %path.display(),
                %name,
                %error,
                "skipping wheel device that could not be released after availability check"
            );
            continue;
        }
        result.push(Candidate {
            path,
            name,
            vendor: id.vendor(),
            product: id.product(),
        });
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

pub fn run(config: Config, path: &Path, grab: bool) -> Result<()> {
    if !grab {
        bail!("refusing to duplicate input without --grab; first test with --discover")
    }
    let mut source =
        Device::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    verify_source(&source)?;
    source.set_nonblocking(true)?;
    let source_name = source.name().unwrap_or("Unnamed mouse").to_owned();
    let mut outputs = Outputs::new().context("cannot create uinput devices")?;
    // Outputs exist before the exclusive grab, so setup failure cannot remove the cursor.
    source.grab().context("cannot exclusively grab source")?;
    tracing::info!(device = %path.display(), name = source_name, "capturing mouse");

    let started = Instant::now();
    let frame_time = Duration::from_secs_f64(1.0 / config.target_hz as f64);
    let gesture_idle = Duration::from_secs_f64(config.gesture_idle_ms / 1000.0);
    let gesture_prime_units = config.gesture_prime_units;
    let mut next_frame = Instant::now();
    let mut engine = ScrollEngine::new(config);
    let mut gesture = false;
    let mut last_scroll_input = None;
    let result = (|| -> Result<()> {
        loop {
            match source.fetch_events() {
                Ok(events) => {
                    for event in events {
                        if event.event_type() == EventType::RELATIVE
                            && event.code() == Rel::REL_WHEEL.0
                        {
                            last_scroll_input = Some(Instant::now());
                            engine.input(Axis::Vertical, event.value(), elapsed_ms(started));
                        } else if event.event_type() == EventType::RELATIVE
                            && event.code() == Rel::REL_HWHEEL.0
                        {
                            last_scroll_input = Some(Instant::now());
                            engine.input(Axis::Horizontal, event.value(), elapsed_ms(started));
                        } else if event.event_type() == EventType::RELATIVE
                            && matches!(event.code(), x if x == Rel::REL_WHEEL_HI_RES.0 || x == Rel::REL_HWHEEL_HI_RES.0)
                        {
                            // Ignore the high-resolution companion of a legacy detent to avoid double input.
                        } else if event.event_type() != EventType::SYNCHRONIZATION {
                            outputs.forward(event)?;
                        }
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }

            let now = Instant::now();
            if now >= next_frame {
                let frame = engine.frame(elapsed_ms(started));
                if (frame.vertical != 0 || frame.horizontal != 0) && !gesture {
                    outputs.begin(frame.horizontal, frame.vertical, gesture_prime_units)?;
                    gesture = true;
                }
                if frame.vertical != 0 || frame.horizontal != 0 {
                    outputs.scroll(frame.horizontal, frame.vertical)?;
                }
                if gesture
                    && gesture_should_end(
                        engine.active(),
                        last_scroll_input.map(|last| now.duration_since(last)),
                        gesture_idle,
                    )
                {
                    outputs.end()?;
                    gesture = false;
                }
                next_frame = now + frame_time;
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    if gesture {
        let _ = outputs.end();
    }
    let _ = source.ungrab();
    result
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn gesture_should_end(
    engine_active: bool,
    quiet_for: Option<Duration>,
    gesture_idle: Duration,
) -> bool {
    !engine_active && quiet_for.is_none_or(|quiet| quiet >= gesture_idle)
}

fn verify_source(device: &Device) -> Result<()> {
    let rel = device
        .supported_relative_axes()
        .context("source has no relative axes")?;
    if !(rel.contains(Rel::REL_X) && rel.contains(Rel::REL_Y) && rel.contains(Rel::REL_WHEEL)) {
        bail!("unsafe source: REL_X, REL_Y and REL_WHEEL are all required")
    }
    Ok(())
}

struct Outputs {
    mouse: VirtualDevice,
    touch: VirtualDevice,
    x: i32,
    y: i32,
    tracking: i32,
}

impl Outputs {
    const MIN: i32 = 0;
    const MAX: i32 = 8000;
    const START: i32 = 4000;
    const GAP: i32 = 400;
    const MARGIN: i32 = 600;

    fn new() -> Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for key in [
            Key::BTN_LEFT,
            Key::BTN_RIGHT,
            Key::BTN_MIDDLE,
            Key::BTN_SIDE,
            Key::BTN_EXTRA,
        ] {
            keys.insert(key);
        }
        let mut rel = AttributeSet::<Rel>::new();
        rel.insert(Rel::REL_X);
        rel.insert(Rel::REL_Y);
        let mouse = VirtualDevice::builder()?
            .name("Linux Scroll Fix Mouse")
            .input_id(InputId::new(BusType::BUS_USB, 0x1d6b, 0x4d46, 1))
            .with_keys(&keys)?
            .with_relative_axes(&rel)?
            .build()?;

        let mut touch_keys = AttributeSet::<Key>::new();
        for key in [
            Key::BTN_TOUCH,
            Key::BTN_TOOL_FINGER,
            Key::BTN_TOOL_DOUBLETAP,
            Key::BTN_LEFT,
        ] {
            touch_keys.insert(key);
        }
        let mut props = AttributeSet::<PropType>::new();
        props.insert(PropType::POINTER);
        props.insert(PropType::BUTTONPAD);
        let xy = AbsInfo::new(0, Self::MIN, Self::MAX, 0, 0, 40);
        let slot = AbsInfo::new(0, 0, 1, 0, 0, 0);
        let tracking = AbsInfo::new(0, 0, 65535, 0, 0, 0);
        let mut builder = VirtualDevice::builder()?
            .name("Linux Scroll Fix Touchpad")
            .input_id(InputId::new(BusType::BUS_USB, 0x1d6b, 0x4d47, 1))
            .with_keys(&touch_keys)?
            .with_properties(&props)?;
        for (code, info) in [
            (Abs::ABS_X, xy),
            (Abs::ABS_Y, xy),
            (Abs::ABS_MT_SLOT, slot),
            (Abs::ABS_MT_TRACKING_ID, tracking),
            (Abs::ABS_MT_POSITION_X, xy),
            (Abs::ABS_MT_POSITION_Y, xy),
        ] {
            builder = builder.with_absolute_axis(&UinputAbsSetup::new(code, info))?;
        }
        let touch = builder.build()?;
        Ok(Self {
            mouse,
            touch,
            x: Self::START,
            y: Self::START,
            tracking: 100,
        })
    }

    fn forward(&mut self, event: InputEvent) -> Result<()> {
        self.mouse.emit(&[event])?;
        Ok(())
    }

    fn begin(&mut self, dx: i32, dy: i32, prime_units: i32) -> Result<()> {
        self.tracking = (self.tracking + 2) % 65000;
        let prime_x = gesture_prime(dx, prime_units);
        let prime_y = gesture_prime(dy, prime_units);
        let start_x = self.x - prime_x;
        let start_y = self.y - prime_y;
        self.touch.emit(&[
            ev(EventType::KEY, Key::BTN_TOUCH.0, 1),
            ev(EventType::KEY, Key::BTN_TOOL_DOUBLETAP.0, 1),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 0),
            ev(
                EventType::ABSOLUTE,
                Abs::ABS_MT_TRACKING_ID.0,
                self.tracking,
            ),
            ev(
                EventType::ABSOLUTE,
                Abs::ABS_MT_POSITION_X.0,
                start_x - Self::GAP,
            ),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_Y.0, start_y),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 1),
            ev(
                EventType::ABSOLUTE,
                Abs::ABS_MT_TRACKING_ID.0,
                self.tracking + 1,
            ),
            ev(
                EventType::ABSOLUTE,
                Abs::ABS_MT_POSITION_X.0,
                start_x + Self::GAP,
            ),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_Y.0, start_y),
            ev(EventType::ABSOLUTE, Abs::ABS_X.0, start_x),
            ev(EventType::ABSOLUTE, Abs::ABS_Y.0, start_y),
        ])?;
        if prime_x != 0 || prime_y != 0 {
            self.emit_position(self.x, self.y)?;
        }
        Ok(())
    }

    fn emit_position(&mut self, x: i32, y: i32) -> Result<()> {
        self.touch.emit(&[
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 0),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_X.0, x - Self::GAP),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_Y.0, y),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 1),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_X.0, x + Self::GAP),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_Y.0, y),
            ev(EventType::ABSOLUTE, Abs::ABS_X.0, x),
            ev(EventType::ABSOLUTE, Abs::ABS_Y.0, y),
        ])?;
        Ok(())
    }

    fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        let scaled_x = dx * 2;
        let scaled_y = dy * 2;
        let next_x = self.x + scaled_x;
        let next_y = self.y + scaled_y;
        if !(Self::MIN + Self::MARGIN..=Self::MAX - Self::MARGIN).contains(&next_x)
            || !(Self::MIN + Self::MARGIN..=Self::MAX - Self::MARGIN).contains(&next_y)
        {
            // A stationary finger at the virtual edge produces no scroll even
            // while the engine is active. End the gesture cleanly and resume
            // from the opposite edge so an arbitrarily long stream can flow.
            self.end()?;
            self.x = restart_position(scaled_x, Self::MIN, Self::MAX, Self::MARGIN);
            self.y = restart_position(scaled_y, Self::MIN, Self::MAX, Self::MARGIN);
            self.begin(0, 0, 0)?;
        }
        self.x += scaled_x;
        self.y += scaled_y;
        self.touch.emit(&[
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 0),
            ev(
                EventType::ABSOLUTE,
                Abs::ABS_MT_POSITION_X.0,
                self.x - Self::GAP,
            ),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_Y.0, self.y),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 1),
            ev(
                EventType::ABSOLUTE,
                Abs::ABS_MT_POSITION_X.0,
                self.x + Self::GAP,
            ),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_POSITION_Y.0, self.y),
            ev(EventType::ABSOLUTE, Abs::ABS_X.0, self.x),
            ev(EventType::ABSOLUTE, Abs::ABS_Y.0, self.y),
        ])?;
        Ok(())
    }

    fn end(&mut self) -> Result<()> {
        self.touch.emit(&[
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 0),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_TRACKING_ID.0, -1),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_SLOT.0, 1),
            ev(EventType::ABSOLUTE, Abs::ABS_MT_TRACKING_ID.0, -1),
            ev(EventType::KEY, Key::BTN_TOOL_DOUBLETAP.0, 0),
            ev(EventType::KEY, Key::BTN_TOUCH.0, 0),
        ])?;
        self.x = Self::START;
        self.y = Self::START;
        Ok(())
    }
}

fn restart_position(delta: i32, minimum: i32, maximum: i32, margin: i32) -> i32 {
    match delta.cmp(&0) {
        std::cmp::Ordering::Greater => minimum + margin,
        std::cmp::Ordering::Less => maximum - margin,
        std::cmp::Ordering::Equal => (minimum + maximum) / 2,
    }
}

fn gesture_prime(delta: i32, prime_units: i32) -> i32 {
    delta.signum() * prime_units
}

fn ev(kind: EventType, code: u16, value: i32) -> InputEvent {
    InputEvent::new(kind.0, code, value)
}

#[cfg(test)]
mod tests {
    use super::{gesture_prime, gesture_should_end, restart_position};
    use std::time::Duration;

    #[test]
    fn recenter_restarts_on_the_opposite_side() {
        assert_eq!(restart_position(10, 0, 8000, 600), 600);
        assert_eq!(restart_position(-10, 0, 8000, 600), 7400);
        assert_eq!(restart_position(0, 0, 8000, 600), 4000);
    }

    #[test]
    fn slow_detents_keep_the_virtual_gesture_alive() {
        let idle = Duration::from_millis(1500);
        assert!(!gesture_should_end(
            false,
            Some(Duration::from_millis(500)),
            idle,
        ));
        assert!(!gesture_should_end(
            true,
            Some(Duration::from_millis(2000)),
            idle,
        ));
        assert!(gesture_should_end(
            false,
            Some(Duration::from_millis(1500)),
            idle,
        ));
    }

    #[test]
    fn gesture_primer_follows_the_first_tick_direction() {
        assert_eq!(gesture_prime(1, 48), 48);
        assert_eq!(gesture_prime(-1, 48), -48);
        assert_eq!(gesture_prime(0, 48), 0);
    }
}

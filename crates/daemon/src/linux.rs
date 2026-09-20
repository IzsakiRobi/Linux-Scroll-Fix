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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    pub usb_source: Option<String>,
    pub is_virtual: bool,
}

#[derive(Default)]
struct Discovery {
    available: Vec<Candidate>,
    busy_usb_sources: Vec<String>,
    busy_wheel_count: usize,
}

fn usb_source(phys: &str) -> Option<String> {
    let (device, _) = phys.split_once("/input")?;
    device.starts_with("usb-").then(|| device.to_owned())
}

fn virtual_input_path(sysfs_path: &Path) -> bool {
    sysfs_path.starts_with("/sys/devices/virtual/input")
}

fn is_virtual_input(event_path: &Path) -> bool {
    event_path
        .file_name()
        .and_then(|name| {
            std::fs::canonicalize(Path::new("/sys/class/input").join(name).join("device")).ok()
        })
        .is_some_and(|path| virtual_input_path(&path))
}

pub fn discover(config: &Config) -> Result<Vec<Candidate>> {
    Ok(scan(config)?.available)
}

fn scan(config: &Config) -> Result<Discovery> {
    let mut result = Discovery::default();
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
        let id = device.input_id();
        let is_virtual = is_virtual_input(&path);
        let source = if is_virtual {
            None
        } else {
            device.physical_path().and_then(usb_source)
        };
        if let Err(error) = device.grab() {
            // Only EBUSY indicates an existing exclusive owner, not EACCES or ENODEV.
            if error.raw_os_error() == Some(16) {
                result.busy_wheel_count += 1;
                if let Some(source) = source {
                    result.busy_usb_sources.push(source);
                }
            }
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
        // Explicit physical-name filters remain supported. Remapper outputs are
        // identified by the kernel, not by application names or vendor IDs.
        if !is_virtual
            && !config.device_name_patterns.is_empty()
            && !config
                .device_name_patterns
                .iter()
                .any(|pattern| name.to_lowercase().contains(&pattern.to_lowercase()))
        {
            continue;
        }
        result.available.push(Candidate {
            path,
            name,
            vendor: id.vendor(),
            product: id.product(),
            usb_source: source,
            is_virtual,
        });
    }
    result.available.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

fn select_candidate(discovery: &Discovery) -> Result<&Candidate> {
    let has_virtual = discovery.available.iter().any(|c| c.is_virtual);
    let is_captured_sibling = |candidate: &&Candidate| {
        candidate
            .usb_source
            .as_ref()
            .is_some_and(|source| discovery.busy_usb_sources.contains(source))
    };
    // A remapper may create its output after grabbing a receiver interface.
    // Never use the free sibling as a fallback during that gap.
    if !has_virtual && discovery.available.iter().any(|c| is_captured_sibling(&c)) {
        bail!("receiver mouse is captured; waiting for a safe upstream pointer");
    }
    let mut candidates: Vec<_> = discovery
        .available
        .iter()
        .filter(|candidate| !is_captured_sibling(candidate))
        .collect();
    // Keyboard-only remappers can expose an idle virtual pointer. With no
    // captured wheel source, a physical mouse must not be displaced by it.
    if discovery.busy_wheel_count == 0 && candidates.iter().any(|c| !c.is_virtual) {
        candidates.retain(|c| !c.is_virtual);
    }
    match candidates.as_slice() {
        [candidate] => Ok(candidate),
        [] => bail!("automatic selection found no safe wheel device"),
        _ => bail!(
            "automatic selection requires exactly one safe wheel device; found {}: {}",
            candidates.len(),
            candidates
                .iter()
                .map(|c| format!("{} ({})", c.path.display(), c.name))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub fn auto_device(config: &Config) -> Result<PathBuf> {
    // Remappers may start asynchronously. Allow enumeration and grabs to settle
    // without depending on a particular remapper service or kernel version.
    let started = Instant::now();
    let mut previous = None;
    let mut stable_since = Instant::now();
    loop {
        let discovery = scan(config)?;
        let selection = select_candidate(&discovery).cloned();
        let current = selection.as_ref().ok().cloned();
        if current != previous {
            stable_since = Instant::now();
            previous = current;
        }
        if started.elapsed() >= Duration::from_secs(2)
            && stable_since.elapsed() >= Duration::from_secs(1)
        {
            if let Ok(candidate) = &selection {
                tracing::info!(device = %candidate.path.display(), name = %candidate.name,
                    "automatically selected a stable wheel source");
                return Ok(candidate.path.clone());
            }
        }
        if started.elapsed() >= Duration::from_secs(10) {
            selection?;
            bail!("automatic wheel source did not stabilize within 10 seconds");
        }
        thread::sleep(Duration::from_millis(250));
    }
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
    let mut outputs = Outputs::new(&source).context("cannot create uinput devices")?;
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

    fn new(source: &Device) -> Result<Self> {
        // Preserve all source buttons and key mappings, including remappers
        // that combine keyboard shortcuts with a relative pointer.
        let mut rel = AttributeSet::<Rel>::new();
        if let Some(axes) = source.supported_relative_axes() {
            for axis in axes.iter() {
                if ![
                    Rel::REL_WHEEL,
                    Rel::REL_HWHEEL,
                    Rel::REL_WHEEL_HI_RES,
                    Rel::REL_HWHEEL_HI_RES,
                ]
                .contains(&axis)
                {
                    rel.insert(axis);
                }
            }
        }
        let mut builder = VirtualDevice::builder()?
            .name("Linux Scroll Fix Mouse")
            .input_id(InputId::new(BusType::BUS_USB, 0x1d6b, 0x4d46, 1))
            .with_relative_axes(&rel)?;
        if let Some(keys) = source.supported_keys() {
            builder = builder.with_keys(keys)?;
        }
        let mouse = builder.build()?;

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
    use super::{
        Candidate, Discovery, gesture_prime, gesture_should_end, restart_position,
        select_candidate, usb_source, virtual_input_path,
    };
    use std::{path::Path, time::Duration};

    fn physical(event: u8, phys: &str) -> Candidate {
        Candidate {
            path: format!("/dev/input/event{event}").into(),
            name: "Pointer".into(),
            vendor: 0x1234,
            product: 0x5678,
            usb_source: usb_source(phys),
            is_virtual: false,
        }
    }

    fn virtual_pointer(event: u8, name: &str) -> Candidate {
        Candidate {
            name: name.into(),
            is_virtual: true,
            ..physical(event, "")
        }
    }

    fn captured_receiver(available: Vec<Candidate>) -> Discovery {
        Discovery {
            available,
            busy_usb_sources: vec!["usb-controller-port1".into()],
            busy_wheel_count: 1,
        }
    }

    #[test]
    fn receiver_sibling_uses_any_remapper_output() {
        for name in [
            "keyd virtual pointer",
            "input-remapper output",
            "Custom Device",
        ] {
            let upstream = virtual_pointer(42, name);
            let discovery = captured_receiver(vec![
                physical(3, "usb-controller-port1/input2:1"),
                upstream.clone(),
            ]);
            assert_eq!(select_candidate(&discovery).unwrap(), &upstream);
        }
    }

    #[test]
    fn captured_receiver_waits_for_delayed_remapper() {
        let mut discovery = captured_receiver(vec![physical(3, "usb-controller-port1/input2:1")]);
        assert!(select_candidate(&discovery).is_err());
        let upstream = virtual_pointer(99, "Arbitrary Output");
        discovery.available.push(upstream.clone());
        assert_eq!(select_candidate(&discovery).unwrap(), &upstream);
    }

    #[test]
    fn unrelated_mouse_and_multiple_outputs_remain_ambiguous() {
        for available in [
            vec![
                physical(3, "usb-controller-port2/input0"),
                virtual_pointer(42, "Output"),
            ],
            vec![
                virtual_pointer(42, "Output A"),
                virtual_pointer(43, "Output B"),
            ],
        ] {
            assert!(select_candidate(&captured_receiver(available)).is_err());
        }
    }

    #[test]
    fn keyboard_only_remapper_does_not_displace_physical_mouse() {
        let mouse = physical(3, "usb-controller-port1/input0");
        let discovery = Discovery {
            available: vec![mouse.clone(), virtual_pointer(42, "keyd virtual pointer")],
            ..Discovery::default()
        };
        assert_eq!(select_candidate(&discovery).unwrap(), &mouse);
    }

    #[test]
    fn physical_names_and_ids_do_not_create_virtual_identity() {
        let mut mouse = physical(3, "usb-controller-port2/input0");
        mouse.name = "keyd virtual pointer".into();
        mouse.vendor = 0x0fac;
        mouse.product = 0x1ade;
        let discovery = captured_receiver(vec![mouse, virtual_pointer(42, "Output")]);
        assert!(select_candidate(&discovery).is_err());
    }

    #[test]
    fn no_remapper_supports_usb_bluetooth_and_other_pointer_names() {
        for (name, phys) in [
            ("Mouse", "usb-controller-port1/input0"),
            ("Trackball", "bluetooth-device"),
            ("Office Pointer", "serio0/input0"),
        ] {
            let mut mouse = physical(3, phys);
            mouse.name = name.into();
            let discovery = Discovery {
                available: vec![mouse.clone()],
                ..Discovery::default()
            };
            assert_eq!(select_candidate(&discovery).unwrap(), &mouse);
        }
        assert!(select_candidate(&Discovery::default()).is_err());
    }

    #[test]
    fn multiple_physical_mice_are_not_selected_arbitrarily() {
        let discovery = Discovery {
            available: vec![physical(3, ""), physical(4, "")],
            ..Discovery::default()
        };
        assert!(select_candidate(&discovery).is_err());
    }

    #[test]
    fn captured_non_usb_mouse_or_remapper_chain_uses_remaining_output() {
        let output = virtual_pointer(51, "Final Output");
        let discovery = Discovery {
            available: vec![output.clone()],
            busy_wheel_count: 2,
            ..Discovery::default()
        };
        assert_eq!(select_candidate(&discovery).unwrap(), &output);
    }

    #[test]
    fn virtual_identity_comes_from_sysfs_not_name_or_bus() {
        assert!(virtual_input_path(Path::new(
            "/sys/devices/virtual/input/input42"
        )));
        assert!(!virtual_input_path(Path::new(
            "/sys/devices/pci0000/input/input42"
        )));
        assert!(!virtual_input_path(Path::new(
            "/sys/devices/virtual/input-other/input42"
        )));
    }

    #[test]
    fn usb_interfaces_group_by_receiver_without_merging_ports() {
        assert_eq!(
            usb_source("usb-controller-port1/input1"),
            usb_source("usb-controller-port1/input2:1")
        );
        assert_ne!(
            usb_source("usb-controller-port1/input1"),
            usb_source("usb-controller-port2/input1")
        );
        assert_eq!(usb_source(""), None);
        assert_eq!(usb_source("bluetooth/input0"), None);
    }

    #[test]
    #[ignore = "requires access to /dev/uinput and /dev/input; emits no input events"]
    fn kernel_virtual_source_identity_and_forwarded_capabilities() {
        use super::{AttributeSet, Device, Key, Outputs, Rel, VirtualDevice, is_virtual_input};
        let mut keys = AttributeSet::<Key>::new();
        for key in [Key::BTN_LEFT, Key::BTN_TASK, Key::KEY_A] {
            keys.insert(key);
        }
        let mut axes = AttributeSet::<Rel>::new();
        for axis in [Rel::REL_X, Rel::REL_Y, Rel::REL_WHEEL] {
            axes.insert(axis);
        }
        let mut input = VirtualDevice::builder()
            .unwrap()
            .name("ScrollFix Capability Test")
            .with_keys(&keys)
            .unwrap()
            .with_relative_axes(&axes)
            .unwrap()
            .build()
            .unwrap();
        let path = input
            .enumerate_dev_nodes_blocking()
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert!(is_virtual_input(&path));
        let source = Device::open(path).unwrap();
        let mut output = Outputs::new(&source).unwrap();
        let path = output
            .mouse
            .enumerate_dev_nodes_blocking()
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let forwarded = Device::open(path).unwrap();
        let forwarded_keys = forwarded.supported_keys().unwrap();
        assert!(forwarded_keys.contains(Key::KEY_A));
        assert!(forwarded_keys.contains(Key::BTN_TASK));
        assert!(forwarded_keys.contains(Key::BTN_LEFT));
        let forwarded_axes = forwarded.supported_relative_axes().unwrap();
        assert!(forwarded_axes.contains(Rel::REL_X));
        assert!(forwarded_axes.contains(Rel::REL_Y));
        assert!(!forwarded_axes.contains(Rel::REL_WHEEL));
    }

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

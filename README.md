# Linux Scroll Fix

Precise, smooth mouse-wheel scrolling for Linux, with a native GNOME control panel. Physical mouse input is captured through `evdev`; pointer movement and buttons are forwarded through a virtual mouse, while wheel ticks drive a virtual two-finger touchpad.

The included **Precise** profile retains the Mac Mouse Fix High smoothness + Medium speed + Precision tuning and is validated on native Fedora hardware with a physical Logitech mouse. Every mechanical wheel detent starts immediately, isolated ticks move only a few pixels, and faster scrolling accelerates smoothly. **Balanced** keeps that smooth motion with a wider everyday speed range, while **Rapid** is designed for quickly traversing long pages and documents.

## Install on Fedora

Download the RPM from the latest GitHub release and install it with:

```bash
sudo dnf install ./linux-scroll-fix-0.5.3-1.*.rpm
```

Alternatively, build and install directly from the repository:

```bash
git clone https://github.com/IzsakiRobi/Linux-Scroll-Fix.git
cd Linux-Scroll-Fix
sudo ./scripts/install.sh
```

The installer obtains the Rust, Vala, GTK 4, and libadwaita build dependencies from Fedora, builds the daemon and control panel, installs the default configuration, and enables the systemd service immediately and for future graphical boots.

The service waits briefly for input devices and remappers to settle, then starts only when exactly one safe wheel source is available. A free logical mouse sharing a USB receiver with an already captured mouse is not selected as an independent source. Unrelated mice remain ambiguous and are never grabbed arbitrarily.

## Control panel

Open **Linux Scroll Fix** from the GNOME application grid. The native libadwaita control panel provides:

- a **Smooth Scrolling** switch that starts/stops the service and controls automatic startup;
- **Precise**, **Balanced**, and **Rapid** scrolling profiles;
- a nine-position **Scroll Speed** control spanning slower-than-Precise to faster-than-Rapid behavior;
- **Traditional** and **Natural** scroll directions.

The application runs as the desktop user. System changes are performed by a narrowly scoped helper after graphical Polkit authentication; the GUI itself never runs as root.

## Service

Check the service and its current-boot log:

```bash
sudo systemctl status linux-scroll-fix.service
sudo journalctl -u linux-scroll-fix.service -b
```

Stop or start scrolling without uninstalling:

```bash
sudo systemctl stop linux-scroll-fix.service
sudo systemctl start linux-scroll-fix.service
```

## Manual run

Stop the service before running a foreground copy:

```bash
sudo systemctl stop linux-scroll-fix.service
```

Discover suitable, currently available wheel devices:

```bash
sudo linux-scroll-fixd --discover
```

Devices already captured by another input remapper are omitted. Virtual wheel
pointers are identified through the kernel's input-device topology, independent
of application name, vendor ID, event number, or kernel version. With a remapper
such as keyd, the unique available output is used when the physical mouse is
captured. An idle virtual pointer from a keyboard-only remapper does not override
an available physical mouse.

When a USB receiver exposes multiple mouse interfaces, a free sibling of an
already captured interface is excluded from automatic selection. Discovery still
lists it as available; `--auto-device` additionally checks receiver relationships.
A source already captured by Scroll Fix itself is also omitted from `--discover`.

Remappers must expose an available evdev/uinput relative pointer with `REL_X`,
`REL_Y`, and `REL_WHEEL`. A program that exclusively grabs the mouse without
forwarding a readable pointer cannot be chained this way. Multiple unrelated
mice or multiple possible remapper outputs remain ambiguous; use an explicit
source instead of relying on arbitrary event ordering. Source key/button codes
are preserved, including extra mouse buttons and remapped keyboard shortcuts.
Remappers using broad device-matching rules must exclude the `Linux Scroll Fix`
virtual outputs to avoid capturing their own downstream input again.

New configurations use an empty `device_name_patterns` list, admitting all
wheel-capable pointer names. Existing name filters are preserved for physical
devices; use an empty list to remove an old naming restriction. Virtual remapper
outputs are discovered independently of these physical-name filters.

Choose only the reported event node, then run in the foreground:

```bash
sudo linux-scroll-fixd --device /dev/input/eventX --grab
```

Press `Ctrl+C` to stop. The program refuses to capture input without `--grab` and either an explicit device or unambiguous `--auto-device` selection.

## Configuration

The active configuration is stored at `/etc/linux-scroll-fix/config.toml`. Built-in profiles live under `/usr/local/share/linux-scroll-fix/profiles`; applying a profile or custom speed preserves the selected scroll direction. The custom speed control moves the linked sensitivity, acceleration, curvature, and pending-distance limits together so the scrolling model stays internally consistent. To reverse an axis manually, set its `direction` to `"natural"`; use `"traditional"` for the default direction.

## Uninstall

```bash
sudo ./scripts/uninstall.sh
```

The configuration is kept for future installations.

## Attribution and license

Linux Scroll Fix was inspired by [Mac Mouse Fix](https://github.com/noah-nuebling/mac-mouse-fix) and addresses the lack of configurable smooth scrolling for traditional mouse wheels on Linux desktops. Parts of the scrolling model and its High + Medium + Precision values are derived from Mac Mouse Fix under the [MMF License](https://github.com/noah-nuebling/mac-mouse-fix/blob/master/License). The Linux input backend, animation and tail handling, system integration, and GNOME interface are independently designed.

Independently written portions of Linux Scroll Fix are released under the MIT License; see [LICENSE](LICENSE). MMF-derived portions remain subject to the MMF License; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

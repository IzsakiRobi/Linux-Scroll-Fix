# Linux Scroll Fix

Precise, smooth mouse-wheel scrolling for Linux, with a native GNOME control panel. Physical mouse input is captured through `evdev`; pointer movement and buttons are forwarded through a virtual mouse, while wheel ticks drive a virtual two-finger touchpad.

The included **Precise** profile retains the Mac Mouse Fix High smoothness + Medium speed + Precision tuning and is validated on native Fedora hardware with a physical Logitech mouse. Every mechanical wheel detent starts immediately, isolated ticks move only a few pixels, and faster scrolling accelerates smoothly. **Balanced** keeps that smooth motion with a wider everyday speed range, while **Rapid** is designed for quickly traversing long pages and documents.

## Install on Fedora

Download the RPM from the latest GitHub release and install it with:

```bash
sudo dnf install ./linux-scroll-fix-0.5.1-1.*.rpm
```

Alternatively, build and install directly from the repository:

```bash
git clone https://github.com/IzsakiRobi/Linux-Scroll-Fix.git
cd Linux-Scroll-Fix
sudo ./scripts/install.sh
```

The installer obtains the Rust, Vala, GTK 4, and libadwaita build dependencies from Fedora, builds the daemon and control panel, installs the default configuration, and enables the systemd service immediately and for future graphical boots.

The service starts only when exactly one safe wheel device matches the configured device-name patterns. This prevents an ambiguous device from being grabbed automatically.

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

Devices already captured by another input remapper are omitted. When `keyd`
owns the physical mouse, Linux Scroll Fix automatically uses the upstream
`keyd virtual pointer` instead. This keeps existing keyd button mappings in
the input path without competing for the same exclusive evdev grab.

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

# Linux Scroll Fix

Precise, smooth mouse-wheel scrolling for Linux. Physical mouse input is captured through `evdev`; pointer movement and buttons are forwarded through a virtual mouse, while wheel ticks drive a virtual two-finger touchpad.

The included **Precise** profile is Fedora-tested with VMware Fusion and tuned to match Mac Mouse Fix High smoothness + Medium speed + Precision behavior. Every mechanical wheel detent starts immediately, isolated ticks move only a few pixels, and faster scrolling accelerates smoothly.

## Install on Fedora

```bash
git clone https://github.com/IzsakiRobi/Linux-Scroll-Fix.git
cd Linux-Scroll-Fix
sudo ./scripts/install.sh
```

The installer obtains `cargo`, `rust`, and `gcc` from Fedora, builds a release binary, and installs the default configuration. It does not install or enable a service yet.

## Run

Discover suitable wheel devices:

```bash
sudo linux-scroll-fixd --discover
```

Choose only the reported event node, then run in the foreground:

```bash
sudo linux-scroll-fixd --device /dev/input/eventX --grab
```

Press `Ctrl+C` to stop. The program refuses to capture input unless both an explicit device and `--grab` are supplied.

## Configuration

The configuration is stored at `/etc/linux-scroll-fix/config.toml`. To reverse an axis, set its `direction` to `"natural"`; use `"traditional"` for the default direction.

## Uninstall

```bash
sudo ./scripts/uninstall.sh
```

The configuration is kept for future installations.

## Attribution and license

The acceleration model and High + Medium + Precision values are derived from [Mac Mouse Fix](https://github.com/noah-nuebling/mac-mouse-fix) by Noah Nuebling and are used under the [MMF License](https://github.com/noah-nuebling/mac-mouse-fix/blob/master/License). Linux Scroll Fix retains its independently designed Linux animation and tail model.

Linux Scroll Fix is released under the MIT License; see [LICENSE](LICENSE).

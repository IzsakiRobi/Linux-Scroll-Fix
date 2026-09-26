# Linux Scroll Fix

Smooth mouse-wheel scrolling for Linux, with a native GNOME control panel for profiles, speed, and scroll direction.

Fedora RPMs for **x86_64** and **aarch64**, plus an amd64 DEB for Ubuntu-based distributions, are available in [Releases](https://github.com/IzsakiRobi/Linux-Scroll-Fix/releases/latest).

## Manual install / uninstall

On Fedora or an Ubuntu-based distribution (Ubuntu 24.04+ or an equivalent base), run from the repository directory:

```bash
sudo ./scripts/install.sh
```

Installs dependencies, builds the app, and enables the service. Open **Linux Scroll Fix** from the application menu.

To uninstall (keeps your configuration):

```bash
sudo ./scripts/uninstall.sh
```

## Manual run

Stop the service, find your mouse, then use its event path:

```bash
sudo systemctl stop linux-scroll-fix.service
sudo linux-scroll-fixd --discover
sudo linux-scroll-fixd --device /dev/input/eventX --grab
```

Replace `eventX` with the discovered device. Press `Ctrl+C` to stop; run `sudo systemctl start linux-scroll-fix.service` to resume the background service.

## Configuration

Settings: `/etc/linux-scroll-fix/config.toml`. After manual edits, run `sudo systemctl restart linux-scroll-fix.service`.

With remappers such as keyd, select an available virtual mouse output and exclude the `Linux Scroll Fix` virtual devices from the remapper's capture rules.

## Firefox / Thunderbird

Repeated scrolling at page edges can cause seemingly endless overscroll. To disable it while keeping smooth scrolling, set **`apz.overscroll.enabled` to `false`** in Firefox's `about:config` or Thunderbird's **Settings → General → Config Editor**.

## License

See [LICENSE](LICENSE) and [third-party notices](THIRD_PARTY_NOTICES.md).

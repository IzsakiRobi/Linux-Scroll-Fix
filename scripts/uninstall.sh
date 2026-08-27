#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
    echo "Run with sudo: sudo ./scripts/uninstall.sh" >&2
    exit 1
fi

systemctl disable --now linux-scroll-fix.service 2>/dev/null || true
rm -f /etc/systemd/system/linux-scroll-fix.service
systemctl daemon-reload
systemctl reset-failed linux-scroll-fix.service 2>/dev/null || true
rm -f /usr/local/bin/linux-scroll-fixd
rm -f /usr/local/bin/linux-scroll-fix
rm -f /usr/local/libexec/linux-scroll-fixctl
rm -f /usr/local/share/applications/io.github.izsakirobi.LinuxScrollFix.desktop
rm -f /usr/local/share/metainfo/io.github.izsakirobi.LinuxScrollFix.metainfo.xml
rm -f /usr/local/share/linux-scroll-fix/profiles/precise.toml
rmdir /usr/local/share/linux-scroll-fix/profiles 2>/dev/null || true
rmdir /usr/local/share/linux-scroll-fix 2>/dev/null || true
rm -f /usr/share/polkit-1/actions/io.github.izsakirobi.linux-scroll-fix.policy
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/local/share/applications
fi
echo "Linux Scroll Fix removed. Configuration was kept in /etc/linux-scroll-fix."

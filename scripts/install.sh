#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
    echo "Run with sudo: sudo ./scripts/install.sh" >&2
    exit 1
fi

project_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dnf install -y cargo rust gcc

build_user=${SUDO_USER:-root}
if [[ ${build_user} == root ]]; then
    cargo build --release --manifest-path "${project_dir}/Cargo.toml"
else
    runuser -u "${build_user}" -- cargo build --release --manifest-path "${project_dir}/Cargo.toml"
fi

install -Dm0755 "${project_dir}/target/release/linux-scroll-fixd" /usr/local/bin/linux-scroll-fixd
if [[ ! -e /etc/linux-scroll-fix/config.toml ]]; then
    install -Dm0644 "${project_dir}/config/default.toml" /etc/linux-scroll-fix/config.toml
fi
install -Dm0644 "${project_dir}/systemd/linux-scroll-fix.service" /etc/systemd/system/linux-scroll-fix.service
systemctl daemon-reload
systemctl enable --now linux-scroll-fix.service

echo "Linux Scroll Fix installed and enabled."
echo "Status: sudo systemctl status linux-scroll-fix.service"
echo "Logs:   sudo journalctl -u linux-scroll-fix.service -b"

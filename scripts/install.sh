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

echo "Linux Scroll Fix installed. No service was enabled or started."
echo "1. sudo linux-scroll-fixd --discover"
echo "2. sudo linux-scroll-fixd --device /dev/input/eventX --grab"
echo "3. Press Ctrl+C to stop."

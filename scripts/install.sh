#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
    echo "Run with sudo: sudo ./scripts/install.sh" >&2
    exit 1
fi

project_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dnf install -y \
    cargo \
    gcc \
    gtk4-devel \
    libadwaita-devel \
    meson \
    ninja-build \
    polkit \
    rust \
    vala

build_user=${SUDO_USER:-root}
run_as_build_user() {
    if [[ ${build_user} == root ]]; then
        "$@"
    else
        runuser -u "${build_user}" -- "$@"
    fi
}

run_as_build_user cargo build --release --manifest-path "${project_dir}/Cargo.toml"
if [[ -f ${project_dir}/build/gui/meson-private/coredata.dat ]]; then
    run_as_build_user meson setup \
        "${project_dir}/build/gui" \
        "${project_dir}" \
        --prefix=/usr/local \
        --buildtype=release \
        --reconfigure
else
    run_as_build_user meson setup \
        "${project_dir}/build/gui" \
        "${project_dir}" \
        --prefix=/usr/local \
        --buildtype=release
fi
run_as_build_user meson compile -C "${project_dir}/build/gui"

install -Dm0755 "${project_dir}/target/release/linux-scroll-fixd" /usr/local/bin/linux-scroll-fixd
install -Dm0755 "${project_dir}/target/release/linux-scroll-fixctl" /usr/local/libexec/linux-scroll-fixctl
if [[ ! -e /etc/linux-scroll-fix/config.toml ]]; then
    install -Dm0644 "${project_dir}/config/default.toml" /etc/linux-scroll-fix/config.toml
fi
install -Dm0644 "${project_dir}/config/default.toml" /usr/local/share/linux-scroll-fix/profiles/precise.toml
install -Dm0644 "${project_dir}/systemd/linux-scroll-fix.service" /etc/systemd/system/linux-scroll-fix.service
install -Dm0644 \
    "${project_dir}/polkit/io.github.izsakirobi.linux-scroll-fix.policy" \
    /usr/share/polkit-1/actions/io.github.izsakirobi.linux-scroll-fix.policy
meson install -C "${project_dir}/build/gui" --no-rebuild
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/local/share/applications
fi
systemctl daemon-reload
systemctl enable linux-scroll-fix.service
systemctl restart linux-scroll-fix.service

echo "Linux Scroll Fix installed and enabled."
echo "Open 'Linux Scroll Fix' from the application grid to configure it."
echo "Status: sudo systemctl status linux-scroll-fix.service"
echo "Logs:   sudo journalctl -u linux-scroll-fix.service -b"

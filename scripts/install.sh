#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
    echo "Run with sudo: sudo ./scripts/install.sh" >&2
    exit 1
fi

project_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# Match the distribution, not whichever package manager happens to be installed.
. /etc/os-release
cargo_bin=/usr/bin/cargo
rustc_bin=/usr/bin/rustc
case " ${ID:-} ${ID_LIKE:-} " in
    *" fedora "*|*" rhel "*)
        dnf install -y \
            cargo gcc gtk4-devel libadwaita-devel meson ninja-build \
            pkgconf-pkg-config polkit rust vala
        ;;
    *" ubuntu "*|*" debian "*|*" elementary "*)
        apt-get update
        rust_packages=(cargo rustc)
        # Ubuntu 24.04's unversioned Rust is too old for edition 2024.
        rust_candidate=$(LC_ALL=C apt-cache policy rustc | awk '/Candidate:/ {print $2}')
        if [[ -z ${rust_candidate} || ${rust_candidate} == '(none)' ]]; then
            echo "No Rust compiler available. Check your APT repositories." >&2
            exit 1
        fi
        if dpkg --compare-versions "${rust_candidate}" lt 1.85; then
            rust_packages=(cargo-1.85 rustc-1.85)
            # Use the real Cargo binary: older versioned wrappers can launch
            # the unversioned Cargo instead (Ubuntu bug 2100266).
            cargo_bin=/usr/lib/rust-1.85/share/cargo/bin/cargo
            rustc_bin=/usr/bin/rustc-1.85
        fi
        apt-get install -y \
            "${rust_packages[@]}" build-essential libgtk-4-dev libadwaita-1-dev \
            meson ninja-build pkg-config valac pkexec polkitd \
            desktop-file-utils libgtk-4-bin ca-certificates
        ;;
    *)
        echo "Unsupported distribution: ${ID:-unknown}. Use Fedora or an Ubuntu/Debian-based system." >&2
        exit 1
        ;;
esac

# Fail before installing app files if the desktop libraries are too old.
if ! pkg-config --atleast-version=4.10 gtk4 || \
   ! pkg-config --atleast-version=1.4 libadwaita-1; then
    echo "GTK >= 4.10 and libadwaita >= 1.4 are required (Ubuntu 24.04 / elementary OS 8 or newer)." >&2
    exit 1
fi

build_user=${SUDO_USER:-root}
run_as_build_user() {
    if [[ ${build_user} == root ]]; then
        "$@"
    else
        runuser -u "${build_user}" -- "$@"
    fi
}

rust_version=$(run_as_build_user "${rustc_bin}" --version | awk '{print $2}')
if [[ $(printf '%s\n' 1.85 "${rust_version}" | sort -V | head -n 1) != 1.85 ]]; then
    echo "Rust >= 1.85 is required; found ${rust_version}." >&2
    exit 1
fi
run_as_build_user env RUSTC="${rustc_bin}" "${cargo_bin}" build --release --locked --manifest-path "${project_dir}/Cargo.toml"
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
install -Dm0644 "${project_dir}/config/balanced.toml" /usr/local/share/linux-scroll-fix/profiles/balanced.toml
install -Dm0644 "${project_dir}/config/rapid.toml" /usr/local/share/linux-scroll-fix/profiles/rapid.toml
install -Dm0644 "${project_dir}/systemd/linux-scroll-fix.service" /etc/systemd/system/linux-scroll-fix.service
install -Dm0644 \
    "${project_dir}/polkit/io.github.izsakirobi.linux-scroll-fix.policy" \
    /usr/share/polkit-1/actions/io.github.izsakirobi.linux-scroll-fix.policy
meson install -C "${project_dir}/build/gui" --no-rebuild
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/local/share/applications
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache --ignore-theme-index --force /usr/local/share/icons/hicolor
fi
systemctl daemon-reload
systemctl enable linux-scroll-fix.service
systemctl restart linux-scroll-fix.service

echo "Linux Scroll Fix installed and enabled."
echo "Open 'Linux Scroll Fix' from the application grid to configure it."
echo "Status: sudo systemctl status linux-scroll-fix.service"
echo "Logs:   sudo journalctl -u linux-scroll-fix.service -b"

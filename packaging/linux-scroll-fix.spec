Name:           linux-scroll-fix
Version:        0.5.1
Release:        1%{?dist}
Summary:        Precise, smooth mouse-wheel scrolling for Linux

License:        MIT AND LicenseRef-MMF
URL:            https://github.com/IzsakiRobi/Linux-Scroll-Fix
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  meson
BuildRequires:  ninja-build
BuildRequires:  rust
BuildRequires:  vala
Requires:       polkit
Requires:       systemd

%description
Linux Scroll Fix converts traditional mouse-wheel ticks into precise, smooth
scrolling and provides a native GNOME control panel for profiles, speed,
direction, and service control.

%prep
%autosetup

%build
cargo build --release --locked
%meson --prefix=/usr/local
%meson_build

%install
install -Dm0755 target/release/linux-scroll-fixd %{buildroot}/usr/local/bin/linux-scroll-fixd
install -Dm0755 target/release/linux-scroll-fixctl %{buildroot}/usr/local/libexec/linux-scroll-fixctl
install -Dm0644 config/default.toml %{buildroot}/etc/linux-scroll-fix/config.toml
install -Dm0644 config/default.toml %{buildroot}/usr/local/share/linux-scroll-fix/profiles/precise.toml
install -Dm0644 config/balanced.toml %{buildroot}/usr/local/share/linux-scroll-fix/profiles/balanced.toml
install -Dm0644 config/rapid.toml %{buildroot}/usr/local/share/linux-scroll-fix/profiles/rapid.toml
install -Dm0644 systemd/linux-scroll-fix.service %{buildroot}%{_unitdir}/linux-scroll-fix.service
install -Dm0644 polkit/io.github.izsakirobi.linux-scroll-fix.policy %{buildroot}%{_datadir}/polkit-1/actions/io.github.izsakirobi.linux-scroll-fix.policy
DESTDIR=%{buildroot} %meson_install

%post
%systemd_post linux-scroll-fix.service
systemctl enable --now linux-scroll-fix.service >/dev/null 2>&1 || :

%preun
%systemd_preun linux-scroll-fix.service

%postun
%systemd_postun_with_restart linux-scroll-fix.service

%files
%license LICENSE THIRD_PARTY_NOTICES.md
%doc README.md
%config(noreplace) /etc/linux-scroll-fix/config.toml
%{_bindir}/linux-scroll-fix
/usr/local/bin/linux-scroll-fixd
/usr/local/libexec/linux-scroll-fixctl
%{_datadir}/applications/io.github.izsakirobi.LinuxScrollFix.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.izsakirobi.LinuxScrollFix.svg
%{_datadir}/metainfo/io.github.izsakirobi.LinuxScrollFix.metainfo.xml
/usr/local/share/linux-scroll-fix/profiles/precise.toml
/usr/local/share/linux-scroll-fix/profiles/balanced.toml
/usr/local/share/linux-scroll-fix/profiles/rapid.toml
%{_unitdir}/linux-scroll-fix.service
%{_datadir}/polkit-1/actions/io.github.izsakirobi.linux-scroll-fix.policy

%changelog
* Sun Aug 30 2026 IzsakiRobi <izsakirobi@users.noreply.github.com> - 0.5.1-1
- Add keyd-compatible input discovery and bound service restart failures
- Restore compatibility with the declared Rust 1.85 minimum version

* Thu Aug 27 2026 IzsakiRobi <izsakirobi@users.noreply.github.com> - 0.5.0-1
- First public release

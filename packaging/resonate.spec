# Resonate RPM spec — builds a local, installable package.
#
# Build (from a clean checkout):
#   VERSION=$(awk -F'"' '/^version/{print $2; exit}' Cargo.toml)
#   git archive --format=tar.gz --prefix=resonate-$VERSION/ -o ~/rpmbuild/SOURCES/resonate-$VERSION.tar.gz HEAD
#   rpmbuild -bb packaging/resonate.spec
# The .rpm lands in ~/rpmbuild/RPMS/<arch>/.
#
# NOTE: %build runs `cargo build`, which fetches crates from the network. For a
# fully offline build, run `cargo vendor` first and point CARGO_HOME at it.

Name:           resonate
Version:        0.1.0
Release:        1%{?dist}
Summary:        Soundboard with a virtual microphone and real-time mic effects

License:        GPL-3.0-or-later
URL:            https://github.com/kilo2071/Resonate
Source0:        %{name}-%{version}.tar.gz

ExclusiveArch:  %{rust_arches}

# The release binary is built without debug symbols, so skip the (empty)
# -debuginfo subpackage that rpmbuild would otherwise try to create.
%global debug_package %{nil}

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  pkgconfig(libpipewire-0.3)
BuildRequires:  pkgconfig(dbus-1)
BuildRequires:  lilv-devel
BuildRequires:  lv2-devel
BuildRequires:  serd-devel
BuildRequires:  sord-devel
BuildRequires:  sratom-devel
BuildRequires:  desktop-file-utils

# Runtime: PipeWire + the WirePlumber tooling used to set the default input.
Requires:       pipewire
Requires:       pipewire-pulseaudio
Requires:       wireplumber

# Effects work with the built-in Gain/Gate out of the box; these provide a rich
# set of LV2 plugins (compressor, EQ, autogain, …) but are not required.
Recommends:     lsp-plugins-lv2

%global appid io.github.kilo2071.Resonate

%description
Resonate is a native GNOME soundboard built with GTK 4 and Libadwaita. It
exposes a PipeWire virtual microphone that mixes soundboard playback with your
real microphone, and applies a real-time effects chain to the mic input using
built-in Noise Gate / Gain plugins plus any installed LV2 plugin. It can run in
the background (with a tray indicator) and start on login, acting as a
voice-effects processor for calls, streaming and recording.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --release --locked

%install
install -Dm0755 target/release/resonate %{buildroot}%{_bindir}/resonate
install -Dm0644 data/%{appid}.desktop \
    %{buildroot}%{_datadir}/applications/%{appid}.desktop
install -Dm0644 data/icons/%{appid}.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{appid}.svg

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{appid}.desktop

%files
%license LICENSE
%doc README.md
%{_bindir}/resonate
%{_datadir}/applications/%{appid}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{appid}.svg

%changelog
* Sat Jun 07 2026 kilo2071 <gerhardprins@icloud.com> - 0.1.0-1
- Initial package: soundboard, PipeWire virtual mic, LV2 mic effects,
  background mode with tray indicator and start-on-login.

project(
  'linux-scroll-fix-gui',
  ['vala', 'c'],
  version: '0.5.0',
  meson_version: '>= 0.61',
)

gtk = dependency('gtk4', version: '>= 4.10')
libadwaita = dependency('libadwaita-1', version: '>= 1.4')
math = meson.get_compiler('c').find_library('m', required: false)

executable(
  'linux-scroll-fix',
  'gui/src/main.vala',
  dependencies: [gtk, libadwaita, math],
  install: true,
)

install_data(
  'gui/data/io.github.izsakirobi.LinuxScrollFix.desktop',
  install_dir: join_paths(get_option('datadir'), 'applications'),
)

install_data(
  'gui/data/io.github.izsakirobi.LinuxScrollFix.metainfo.xml',
  install_dir: join_paths(get_option('datadir'), 'metainfo'),
)

install_data(
  'gui/data/icons/hicolor/scalable/apps/io.github.izsakirobi.LinuxScrollFix.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'),
)

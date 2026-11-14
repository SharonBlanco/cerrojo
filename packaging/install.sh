#!/bin/sh
# Instala Cerrojo en Linux (x86_64 o ARM64):
#
#   curl -fsSL https://raw.githubusercontent.com/sthifen/cerrojo/main/packaging/install.sh | sh
#
# Descarga la AppImage a ~/.local/bin/cerrojo y la añade al menú de aplicaciones.
# Desinstalar:  sh install.sh --desinstalar
set -eu
REPO=sthifen/cerrojo
BIN="$HOME/.local/bin/cerrojo"
DESKTOP="$HOME/.local/share/applications/dev.cerrojo.desktop"
ICON="$HOME/.local/share/icons/hicolor/scalable/apps/dev.cerrojo.svg"
say() { printf '\033[1;36m→\033[0m %s\n' "$*"; }

if [ "${1:-}" = "--desinstalar" ] || [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN" "$DESKTOP" "$ICON"
  say "Cerrojo desinstalado. Tus candados siguen en el celular: quítalos antes desde la app si quieres."
  exit 0
fi

case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) echo "Arquitectura no soportada: $(uname -m)"; exit 1 ;;
esac

mkdir -p "$(dirname "$BIN")" "$(dirname "$DESKTOP")" "$(dirname "$ICON")"
say "Descargando Cerrojo ($ARCH)…"
curl -fL --progress-bar -o "$BIN.tmp" "https://github.com/$REPO/releases/latest/download/Cerrojo-$ARCH.AppImage"
chmod +x "$BIN.tmp" && mv "$BIN.tmp" "$BIN"
curl -fsSL -o "$ICON" "https://raw.githubusercontent.com/$REPO/main/packaging/cerrojo.svg"
cat > "$DESKTOP" <<EOD
[Desktop Entry]
Type=Application
Name=Cerrojo
Comment=Bloquea en el celular las apps de reels y video vertical
Exec=$BIN
Icon=dev.cerrojo
Categories=Utility;
StartupWMClass=dev.cerrojo
EOD
command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$DESKTOP")" 2>/dev/null || true

if ! command -v adb >/dev/null; then
  say "Falta adb. Instálalo con el gestor de tu distro:"
  echo "    Fedora:  sudo dnf install android-tools"
  echo "    Ubuntu/Debian/Mint:  sudo apt install adb"
  echo "    Arch:  sudo pacman -S android-tools"
fi
say "Listo. Abre «Cerrojo» desde el menú de aplicaciones."

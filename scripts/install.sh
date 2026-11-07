#!/bin/sh
# Margin installer for Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/arxhr007/margin/main/scripts/install.sh | sh
#
# Options (put them after `sh -s --`):
#   --uninstall       remove Margin (settings in ~/.config/margin are kept)
#   --no-service      don't set up the systemd user service
#   --no-permissions  don't touch udev rules or groups (you must grant /dev/uinput + /dev/input access yourself)
#   --version vX.Y.Z  install a specific release instead of the latest
#
# Environment: MARGIN_BIN_DIR (default ~/.local/bin), MARGIN_BASE_URL (download from somewhere else, for testing)
#
# What it does: downloads the static margin binary, checks its SHA-256, installs it, lets your user
# use /dev/uinput and read touchpads (one sudo step), and starts Margin as a systemd user service.

set -eu

REPO="arxhr007/margin"
BIN_DIR="${MARGIN_BIN_DIR:-$HOME/.local/bin}"
UNIT_DIR="$HOME/.config/systemd/user"
UDEV_RULE="/etc/udev/rules.d/60-margin-uinput.rules"
MODULES_CONF="/etc/modules-load.d/margin-uinput.conf"

UNINSTALL=0
SERVICE=1
PERMISSIONS=1
VERSION=""

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) UNINSTALL=1 ;;
    --no-service) SERVICE=0 ;;
    --no-permissions) PERMISSIONS=0 ;;
    --version) shift; [ $# -gt 0 ] || die "--version needs a value"; VERSION="$1" ;;
    -h|--help) sed -n '2,16p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

[ "$(uname -s)" = "Linux" ] || die "this installer is for Linux (Windows: see the README for the PowerShell one-liner)"

have() { command -v "$1" >/dev/null 2>&1; }

# ---- uninstall ----
if [ "$UNINSTALL" -eq 1 ]; then
  say "Removing Margin..."
  if have systemctl; then
    systemctl --user disable --now margin.service >/dev/null 2>&1 || true
  fi
  rm -f "$UNIT_DIR/margin.service" "$BIN_DIR/margin"
  have systemctl && systemctl --user daemon-reload >/dev/null 2>&1 || true
  if [ -e "$UDEV_RULE" ] || [ -e "$MODULES_CONF" ]; then
    if have sudo; then
      say "Removing the udev rule (sudo asks for your password)..."
      sudo rm -f "$UDEV_RULE" "$MODULES_CONF" || warn "could not remove $UDEV_RULE and $MODULES_CONF"
    else
      warn "remove $UDEV_RULE and $MODULES_CONF yourself (needs root)"
    fi
  fi
  say "Done. Your settings in ~/.config/margin were left alone; delete that folder to remove them too."
  exit 0
fi

# ---- download ----
case "$(uname -m)" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) die "unsupported CPU: $(uname -m) (Margin ships x86_64 and aarch64 builds)" ;;
esac
ASSET="margin-linux-$ARCH"

if [ -n "${MARGIN_BASE_URL:-}" ]; then
  BASE="$MARGIN_BASE_URL"
elif [ -n "$VERSION" ]; then
  BASE="https://github.com/$REPO/releases/download/$VERSION"
else
  BASE="https://github.com/$REPO/releases/latest/download"
fi

fetch() { # fetch URL DEST
  if have curl; then
    curl -fsSL "$1" -o "$2"
  elif have wget; then
    wget -q "$1" -O "$2"
  else
    die "need curl or wget to download Margin"
  fi
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

say "Downloading $ASSET ..."
fetch "$BASE/$ASSET" "$TMP/$ASSET" || die "download failed: $BASE/$ASSET (is there a Linux build in that release?)"
if fetch "$BASE/$ASSET.sha256" "$TMP/$ASSET.sha256" 2>/dev/null; then
  if have sha256sum; then
    (cd "$TMP" && sha256sum -c "$ASSET.sha256" >/dev/null 2>&1) || die "checksum mismatch: the download is corrupt or has been tampered with"
    say "Checksum OK."
  elif have shasum; then
    (cd "$TMP" && shasum -a 256 -c "$ASSET.sha256" >/dev/null 2>&1) || die "checksum mismatch: the download is corrupt or has been tampered with"
    say "Checksum OK."
  else
    warn "no sha256sum or shasum found; skipping the checksum check"
  fi
else
  warn "no checksum file in this release; skipping the checksum check"
fi

chmod 755 "$TMP/$ASSET"
"$TMP/$ASSET" --version >/dev/null 2>&1 || die "the downloaded program does not run on this system"

mkdir -p "$BIN_DIR"
# replace atomically: a running copy keeps working until it is restarted
install -m 755 "$TMP/$ASSET" "$BIN_DIR/margin.new" && mv -f "$BIN_DIR/margin.new" "$BIN_DIR/margin"
say "Installed $BIN_DIR/margin ($("$BIN_DIR/margin" --version))"

# ---- permissions: /dev/uinput (send keys) and /dev/input (read the touchpad) ----
NEED_RELOGIN=0
if [ "$PERMISSIONS" -eq 1 ]; then
  user="$(id -un)"
  in_input_group=0
  id -nG "$user" 2>/dev/null | tr ' ' '\n' | grep -qx input && in_input_group=1
  uinput_ok=0
  [ -w /dev/uinput ] && uinput_ok=1

  if [ "$in_input_group" -eq 1 ] && [ "$uinput_ok" -eq 1 ]; then
    say "Permissions already fine."
  elif have sudo; then
    say "Margin needs to read your touchpad and create a virtual keyboard."
    say "That means: membership of the 'input' group and write access to /dev/uinput."
    say "Running a few commands with sudo (it may ask for your password)..."
    sudo sh -c '
      set -e
      getent group input >/dev/null || groupadd input
      printf "%s\n" "KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"input\", OPTIONS+=\"static_node=uinput\"" > "'"$UDEV_RULE"'"
      printf "%s\n" uinput > "'"$MODULES_CONF"'"
      modprobe uinput 2>/dev/null || true
      udevadm control --reload-rules 2>/dev/null || true
      udevadm trigger --subsystem-match=misc --sysname-match=uinput 2>/dev/null || true
      udevadm settle 2>/dev/null || true
      chgrp input /dev/uinput 2>/dev/null && chmod 0660 /dev/uinput 2>/dev/null || true
    ' || warn "the sudo step failed; see the README section \"Permissions\" to do it by hand"
    if [ "$in_input_group" -eq 0 ]; then
      sudo usermod -aG input "$user" && NEED_RELOGIN=1 || warn "could not add $user to the input group (run: sudo usermod -aG input $user)"
    fi
  else
    warn "sudo is not available. As root, run:"
    warn "  groupadd -f input && usermod -aG input $user"
    warn "  echo 'KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"input\"' > $UDEV_RULE && modprobe uinput"
  fi
fi

# ---- service ----
STARTED=0
if [ "$SERVICE" -eq 1 ]; then
  if have systemctl && systemctl --user show-environment >/dev/null 2>&1; then
    mkdir -p "$UNIT_DIR"
    cat > "$UNIT_DIR/margin.service" <<EOF
[Unit]
Description=Margin: touchpad edge and corner controls
After=graphical-session.target
PartOf=graphical-session.target

[Service]
ExecStart=$BIN_DIR/margin
Restart=on-failure
RestartSec=3

[Install]
WantedBy=graphical-session.target
EOF
    systemctl --user daemon-reload
    systemctl --user enable margin.service >/dev/null 2>&1 || warn "could not enable the service"
    if [ "$NEED_RELOGIN" -eq 0 ]; then
      systemctl --user restart margin.service >/dev/null 2>&1 && STARTED=1 || warn "could not start the service; check: systemctl --user status margin"
    fi
  else
    warn "systemd user services are not available here; start Margin yourself with: $BIN_DIR/margin"
  fi
fi

# ---- summary ----
say ""
say "Margin is installed."
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "Note: $BIN_DIR is not in your PATH. Add it, or run $BIN_DIR/margin directly." ;;
esac
if [ "$NEED_RELOGIN" -eq 1 ]; then
  say "Log out and back in once (you were just added to the 'input' group). Margin starts by itself after that."
elif [ "$STARTED" -eq 1 ]; then
  say "It is running now and will start with your desktop session."
fi
say "Change what the edges and corners do:   margin settings"
say "See what it detects (in a terminal):    margin --diag"
say "Uninstall:  curl -fsSL https://raw.githubusercontent.com/$REPO/main/scripts/install.sh | sh -s -- --uninstall"

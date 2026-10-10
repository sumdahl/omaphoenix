#!/usr/bin/env bash
# OmaPhoenix and Security Audit installer (the Rust versions).
#
#   curl -fsSL https://raw.githubusercontent.com/sumdahl/omaphoenix-secaudit/<tag>/install.sh | bash
#   ... | bash -s -- --gui         also install the desktop app (asked otherwise)
#   ... | bash -s -- --no-gui      terminal apps only
#   ... | bash -s -- --sysctl      also apply the kernel hardening drop-in (sudo)
#   ... | bash -s -- --uninstall   remove the commands, launchers and icons
#
# Downloads a release, verifies it against SHA256SUMS, installs `omaphoenix`
# (CLI and terminal app), `secaudit`, `secaudit-tui` and `secaudit-ui` to
# ~/.local/bin, optionally the `omaphoenix-gui` desktop app, adds launcher
# entries with their icons, then offers to back this machine up or restore a
# snapshot. The terminal binaries are static; the desktop app uses the
# system's graphics libraries. Everything lives in main(), so a download cut
# off mid-way runs nothing.
#
# Environment:
#   OMAPHOENIX_VERSION   release tag to install (default: latest)
#   OMAPHOENIX_BIN_DIR   install directory (default: ~/.local/bin)
#   OMAPHOENIX_BASE_URL  release download base, for mirrors and tests
#   OMAPHOENIX_GUI       1 or 0: install the desktop app without asking
#   OMAPHOENIX_NO_MENU   set to skip the backup/restore menu

say() { printf '\033[1m%s\033[0m\n' "$*"; }
die() {
  printf '\033[31merror:\033[0m %s\n' "$*" >&2
  exit 1
}

main() {
  set -euo pipefail

  local repo=sumdahl/omaphoenix-secaudit
  local version=${OMAPHOENIX_VERSION:-latest}
  local bin_dir=${OMAPHOENIX_BIN_DIR:-$HOME/.local/bin}
  local data_dir=${XDG_DATA_HOME:-$HOME/.local/share}
  local gui=${OMAPHOENIX_GUI:-} sysctl=0
  local bins=(omaphoenix secaudit secaudit-tui secaudit-ui)

  while (($#)); do
    case $1 in
    --gui) gui=1 ;;
    --no-gui) gui=0 ;;
    --sysctl) sysctl=1 ;;
    --uninstall)
      uninstall "$bin_dir" "$data_dir"
      return
      ;;
    -h | --help)
      echo "usage: install.sh [--gui | --no-gui] [--sysctl] [--uninstall]"
      return
      ;;
    *) die "unknown option: $1" ;;
    esac
    shift
  done

  [[ $(uname -s) == Linux ]] || die "omaphoenix runs on Linux (Omarchy)"
  local arch
  case $(uname -m) in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) die "unsupported CPU: $(uname -m)" ;;
  esac
  command -v sha256sum >/dev/null || die "sha256sum is required"

  local asset="omaphoenix_linux_${arch}.tar.gz"
  local tmp
  tmp=$(mktemp -d)
  # Expanded now: the trap runs after main returns and its locals are gone.
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" EXIT

  say "Downloading omaphoenix ($version, linux/$arch)"
  fetch "$asset" "$tmp" "$repo" "$version"
  tar -xzf "$tmp/$asset" -C "$tmp"
  mkdir -p "$bin_dir"
  local b
  for b in "${bins[@]}"; do
    install -m 0755 "$tmp/$b" "$bin_dir/$b"
    say "✓ installed $b → $bin_dir/$b"
  done

  # The desktop app: asked in a graphical session, unless --gui, --no-gui or
  # OMAPHOENIX_GUI decided. The terminal app is always there.
  if [[ -z $gui ]]; then
    gui=0
    if [[ -n ${WAYLAND_DISPLAY:-}${DISPLAY:-} ]]; then
      gui=1
      if { exec 4</dev/tty; } 2>/dev/null; then
        local reply=''
        read -r -p "Install the OmaPhoenix desktop app too? The terminal app is always installed. [Y/n] " reply <&4 || true
        [[ $reply == [nN]* ]] && gui=0
        exec 4<&-
      fi
    fi
  fi
  if [[ $gui == 1 ]]; then
    if [[ $arch != amd64 ]]; then
      echo "  note: the desktop app is built for amd64 only; use 'omaphoenix tui'"
    else
      local gui_asset="omaphoenix-gui_linux_${arch}.tar.gz"
      fetch "$gui_asset" "$tmp" "$repo" "$version"
      tar -xzf "$tmp/$gui_asset" -C "$tmp" omaphoenix-gui
      install -m 0755 "$tmp/omaphoenix-gui" "$bin_dir/omaphoenix-gui"
      say "✓ installed omaphoenix-gui → $bin_dir/omaphoenix-gui"
    fi
  fi

  launchers "$tmp/share" "$bin_dir" "$data_dir"

  if [[ $sysctl == 1 ]]; then
    sudo install -Dm644 "$tmp/extras/sysctl/99-zz-hardening.conf" /etc/sysctl.d/99-zz-hardening.conf
    sudo sysctl --system >/dev/null
    say "✓ applied /etc/sysctl.d/99-zz-hardening.conf"
  fi

  case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "  note: $bin_dir is not on your PATH" ;;
  esac
  local dep missing=()
  for dep in ss pacman; do command -v "$dep" >/dev/null || missing+=("$dep"); done
  ((${#missing[@]})) && echo "  note: missing ${missing[*]} (some Security Audit checks need them)"
  local apps="omaphoenix tui"
  [[ -x $bin_dir/omaphoenix-gui ]] && apps+=" · omaphoenix gui"
  echo "  apps: $apps · secaudit-tui · secaudit-ui"

  [[ -n ${OMAPHOENIX_NO_MENU:-} ]] && return
  # curl | bash leaves stdin as the pipe; questions go to the terminal.
  if ! { exec 3</dev/tty; } 2>/dev/null; then
    echo "next: omaphoenix init --github <you>/omaphoenix-backup   (or: omaphoenix restore --from <url>)"
    return
  fi
  menu "$bin_dir/omaphoenix" <&3
}

# fetch ASSET DIR REPO VERSION: downloads a release asset and checks it
# against the release's SHA256SUMS; refuses anything that doesn't match.
fetch() {
  local asset=$1 tmp=$2 repo=$3 version=$4
  if [[ -n ${OMAPHOENIX_BASE_URL:-} ]]; then
    curl -fsSL -o "$tmp/$asset" "$OMAPHOENIX_BASE_URL/$asset"
    curl -fsSL -o "$tmp/SHA256SUMS" "$OMAPHOENIX_BASE_URL/SHA256SUMS"
  elif command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
    # gh works for private repos too.
    local tag=()
    [[ $version != latest ]] && tag=("$version")
    gh release download "${tag[@]}" -R "$repo" -D "$tmp" -p "$asset" -p SHA256SUMS --clobber
  else
    local base="https://github.com/$repo/releases/latest/download"
    [[ $version != latest ]] && base="https://github.com/$repo/releases/download/$version"
    curl -fsSL -o "$tmp/$asset" "$base/$asset" ||
      die "download failed (if the repo is private: install gh, run 'gh auth login', retry)"
    curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"
  fi
  (cd "$tmp" && grep " $asset\$" SHA256SUMS | sha256sum -c --quiet -) ||
    die "checksum mismatch for $asset; refusing to install"
}

# launchers SHARE BIN_DIR DATA_DIR: icons and desktop entries. On Omarchy the
# terminal apps open in a terminal window through uwsm; elsewhere the desktop
# picks a terminal (Terminal=true).
launchers() {
  local share=$1 bin_dir=$2 data_dir=$3
  local apps="$data_dir/applications" icons="$data_dir/icons/hicolor/scalable/apps"
  mkdir -p "$apps" "$icons"
  install -m 0644 "$share/icons/omaphoenix.svg" "$share/icons/secaudit.svg" "$icons/"
  local entry launch terminal id title
  for entry in secaudit omaphoenix-tui omaphoenix; do
    [[ $entry == omaphoenix && ! -x $bin_dir/omaphoenix-gui ]] && continue
    launch='' terminal=true
    if command -v uwsm-app >/dev/null && command -v xdg-terminal-exec >/dev/null; then
      id=org.secaudit.tui title="Security Audit"
      [[ $entry == omaphoenix-tui ]] && id=org.omaphoenix.tui title=OmaPhoenix
      launch="uwsm-app -- xdg-terminal-exec --app-id=$id --title=\"$title\" -e" terminal=false
    fi
    sed -e "s|@BINDIR@|$bin_dir|g" -e "s|@LAUNCH@ |${launch:+$launch }|" -e "s|@TERMINAL@|$terminal|" \
      "$share/applications/$entry.desktop.in" >"$apps/$entry.desktop"
    say "✓ launcher $apps/$entry.desktop"
  done
  if command -v update-desktop-database >/dev/null; then update-desktop-database "$apps" 2>/dev/null || true; fi
  if command -v gtk-update-icon-cache >/dev/null; then gtk-update-icon-cache -q "$data_dir/icons/hicolor" 2>/dev/null || true; fi
}

# uninstall BIN_DIR DATA_DIR: the commands, launchers and icons this script
# installed. Snapshots, the backup repo, scan history and the sysctl drop-in
# stay. Every path is guarded: an empty variable stops here, never reaches /.
uninstall() {
  local bin_dir=${1:?} data_dir=${2:?} b f
  if [[ -x $bin_dir/omaphoenix ]]; then "$bin_dir/omaphoenix" timer off >/dev/null 2>&1 || true; fi
  for b in omaphoenix omaphoenix-gui secaudit secaudit-tui secaudit-ui; do
    if [[ -e ${bin_dir:?}/$b ]]; then
      rm -f -- "${bin_dir:?}/$b"
      say "removed $bin_dir/$b"
    fi
  done
  for f in applications/secaudit.desktop applications/omaphoenix.desktop applications/omaphoenix-tui.desktop \
    icons/hicolor/scalable/apps/omaphoenix.svg icons/hicolor/scalable/apps/secaudit.svg; do
    if [[ -e ${data_dir:?}/$f ]]; then
      rm -f -- "${data_dir:?}/$f"
      say "removed $data_dir/$f"
    fi
  done
  if [[ -f /etc/sysctl.d/99-zz-hardening.conf ]]; then
    echo "  kept /etc/sysctl.d/99-zz-hardening.conf (remove with: sudo rm /etc/sysctl.d/99-zz-hardening.conf && sudo sysctl --system)"
  fi
  echo "  kept your snapshots and scan history (${XDG_STATE_HOME:-$HOME/.local/state}/secaudit)"
}

ask() {
  if command -v gum >/dev/null; then
    gum input --value "$2" --header "$1"
  else
    local reply
    read -r -p "$1 [$2] " reply
    echo "${reply:-$2}"
  fi
}

choose() {
  if command -v gum >/dev/null; then
    gum choose "$@"
  else
    select c in "$@"; do
      echo "$c"
      return
    done
  fi
}

confirm() {
  if command -v gum >/dev/null; then
    gum confirm "$1"
  else
    local reply
    read -r -p "$1 [y/N] " reply
    [[ $reply == [yY]* ]]
  fi
}

menu() {
  local omp=$1
  if ! command -v gh >/dev/null; then
    if command -v omarchy-pkg-add >/dev/null; then
      omarchy-pkg-add github-cli
    else
      echo "install GitHub CLI (gh) first, then rerun"
      return 1
    fi
  fi
  gh auth status >/dev/null 2>&1 || gh auth login

  local user
  user=$(gh api user -q .login)
  local default="$user/omaphoenix-backup"

  case $(choose "Back up this machine" "Restore from a snapshot" "Quit") in
  "Back up this machine")
    local name
    name=$(ask "Private GitHub repo for snapshots" "$default")
    if gh repo view "$name" >/dev/null 2>&1; then
      "$omp" init --remote "https://github.com/$name.git"
    else
      "$omp" init --github "$name"
    fi
    if confirm "Include SSH keys (encrypted with a passphrase you choose)?"; then
      "$omp" secrets init
    fi
    "$omp" snapshot
    local every
    every=$(choose 30m 1h 6h daily 15m off)
    [[ $every != off ]] && "$omp" timer on "$every"
    ;;
  "Restore from a snapshot")
    local name
    name=$(ask "GitHub repo holding your snapshots" "$default")
    "$omp" restore --from "https://github.com/$name.git" --dry-run
    if confirm "Apply this?"; then
      "$omp" restore
    fi
    ;;
  esac
}

main "$@"

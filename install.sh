#!/usr/bin/env bash
# OmaPhoenix installer (the Rust version, with Security Audit built in).
#
#   curl -fsSL https://raw.githubusercontent.com/sumdahl/omaphoenix-secaudit/<tag>/install.sh | bash
#
# Downloads a release, verifies it against SHA256SUMS, installs `omaphoenix`
# and `secaudit` to ~/.local/bin, then offers to back this machine up or
# restore a snapshot. The binaries are static: nothing else to install.
# Everything lives in main(), so a download cut off mid-way runs nothing.
#
# Environment:
#   OMAPHOENIX_VERSION   release tag to install (default: latest)
#   OMAPHOENIX_BIN_DIR   install directory (default: ~/.local/bin)
#   OMAPHOENIX_BASE_URL  release download base, for mirrors and tests
#   OMAPHOENIX_NO_MENU   set to skip the backup/restore menu

main() {
  set -euo pipefail

  local repo=sumdahl/omaphoenix-secaudit
  local version=${OMAPHOENIX_VERSION:-latest}
  local bin_dir=${OMAPHOENIX_BIN_DIR:-$HOME/.local/bin}

  say() { printf '\033[1m%s\033[0m\n' "$*"; }
  die() {
    printf '\033[31merror:\033[0m %s\n' "$*" >&2
    exit 1
  }

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
  if [[ -n ${OMAPHOENIX_BASE_URL:-} ]]; then
    curl -fsSL -o "$tmp/$asset" "$OMAPHOENIX_BASE_URL/$asset"
    curl -fsSL -o "$tmp/SHA256SUMS" "$OMAPHOENIX_BASE_URL/SHA256SUMS"
  elif command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
    # gh works for private repos too.
    local tag=()
    [[ $version != latest ]] && tag=("$version")
    gh release download "${tag[@]}" -R "$repo" -D "$tmp" -p "$asset" -p SHA256SUMS
  else
    local base="https://github.com/$repo/releases/latest/download"
    [[ $version != latest ]] && base="https://github.com/$repo/releases/download/$version"
    curl -fsSL -o "$tmp/$asset" "$base/$asset" ||
      die "download failed (if the repo is private: install gh, run 'gh auth login', retry)"
    curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"
  fi

  (cd "$tmp" && grep " $asset\$" SHA256SUMS | sha256sum -c --quiet -) ||
    die "checksum mismatch for $asset; refusing to install"

  tar -xzf "$tmp/$asset" -C "$tmp" omaphoenix secaudit
  mkdir -p "$bin_dir"
  local b
  for b in omaphoenix secaudit; do
    install -m 0755 "$tmp/$b" "$bin_dir/$b"
    say "✓ installed $("$bin_dir/$b" --version) → $bin_dir/$b"
  done
  case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "  note: $bin_dir is not on your PATH" ;;
  esac

  [[ -n ${OMAPHOENIX_NO_MENU:-} ]] && return
  # curl | bash leaves stdin as the pipe; questions go to the terminal.
  if ! { exec 3</dev/tty; } 2>/dev/null; then
    echo "next: omaphoenix init --github <you>/omaphoenix-backup   (or: omaphoenix restore --from <url>)"
    return
  fi
  menu "$bin_dir/omaphoenix" <&3
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

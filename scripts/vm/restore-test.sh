#!/usr/bin/env bash
# One full restore rehearsal on a fresh Omarchy VM (docs/14): stage the Rust
# omaphoenix and a test snapshot, revert the VM to its clean checkpoint, restore,
# and print the report.
#
#   restore-test.sh                     build the current tree (cargo, release)
#   restore-test.sh --release v0.1.0-rc.8   the published tarball, checked against SHA256SUMS
#   restore-test.sh --dry-run           pass --dry-run to the restore (T1)
#
# Environment:
#   OMAPHOENIX_VM_DIR   VM state (default ~/.cache/omaphoenix-vm)
#   VM_CHECKPOINT       clean checkpoint to revert to (default: fresh)
#   VM_SNAP             test snapshot repo to restore (default: $OMAPHOENIX_VM_DIR/snap)
#   VM_BIN              an omaphoenix binary to test as is (e.g. a build from another branch)
#
# The VM gets fake keys only: the test snapshot carries no secrets (scripts/vm/make-test-snapshot
# checks that), SSH uses the throwaway vm_key, and private repos go in as file mirrors.
set -euo pipefail

dir=${OMAPHOENIX_VM_DIR:-$HOME/.cache/omaphoenix-vm}
checkpoint=${VM_CHECKPOINT:-fresh}
snap=${VM_SNAP:-$dir/snap}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
vm="$here/vm.sh"
release="" dry=""

while (($#)); do
  case $1 in
  --release) release=${2:?--release needs a tag}; shift 2 ;;
  --dry-run) dry=--dry-run; shift ;;
  *) sed -n '2,15p' "$0"; exit 2 ;;
  esac
done

stage_binary() {
  local out=$dir/transfer/omaphoenix
  if [[ -n ${VM_BIN:-} ]]; then
    install -m755 "$VM_BIN" "$out"
    return
  fi
  if [[ -z $release ]]; then
    cargo build -q --release -p omaphoenix --manifest-path "$repo/Cargo.toml"
    install -m755 "$repo/target/release/omaphoenix" "$out"
    return
  fi
  local tmp asset=omaphoenix_linux_amd64.tar.gz
  tmp=$(mktemp -d)
  gh release download "$release" -R sumdahl/omaphoenix-secaudit -p "$asset" -p SHA256SUMS -D "$tmp"
  (cd "$tmp" && grep " $asset\$" SHA256SUMS | sha256sum -c --quiet -) || { echo "checksum mismatch"; exit 1; }
  tar -xzf "$tmp/$asset" -C "$tmp" omaphoenix
  install -m755 "$tmp/omaphoenix" "$out"
  rm -rf "${tmp:?}"
}

[[ -d $snap/.git ]] || { echo "no test snapshot at $snap; run scripts/vm/make-test-snapshot first"; exit 1; }
mkdir -p "$dir/transfer"
stage_binary
rm -rf "${dir:?}/transfer/snap.git"
git clone -q --bare "$snap" "$dir/transfer/snap.git"
# Test-only sudo, inside the VM only: omarchy-update runs `sudo -v`, which prompts unless
# every rule for the user is passwordless, so NOPASSWD needs `!authenticate` as well.
printf 'phx ALL=(ALL) NOPASSWD: ALL\nDefaults:phx !authenticate\n' >"$dir/transfer/test-sudoers"
want=$(sha256sum <"$dir/transfer/omaphoenix" | cut -c1-16)

"$vm" stop
"$vm" revert "$checkpoint"
"$vm" boot
for _ in $(seq 60); do "$vm" ssh true 2>/dev/null && break; sleep 3; done

tar -C "$dir/transfer" -cf - . | "$vm" ssh 'rm -rf ~/phx-test && mkdir -p ~/phx-test && tar -C ~/phx-test -xf -'
"$vm" ssh "set -e
echo phoenix-test-only | sudo -S -p '' install -m 440 -o root -g root ~/phx-test/test-sudoers /etc/sudoers.d/99-omaphoenix-test
sudo -n visudo -cq
sudo -K
sudo -n -v 2>/dev/null || { echo 'TEST SUDO NOT PASSWORDLESS (sudo -v)'; exit 1; }
for m in ~/phx-test/*.git; do
  [ \"\$m\" = ~/phx-test/snap.git ] && continue
  url=\$(git -C \"\$m\" config --get remote.origin.url)
  sudo git config --system url.\"file://\$m\".insteadOf \"\$url\"
done
mkdir -p ~/.local/bin && install -m755 ~/phx-test/omaphoenix ~/.local/bin/omaphoenix
test \"\$(sha256sum <~/.local/bin/omaphoenix | cut -c1-16)\" = $want || { echo 'STALE BINARY'; exit 1; }
rm -f ~/restore.log
OMAPHOENIX_REPORT_HWM=1 OMAPHOENIX_NO_NOTIFY=1 nohup ~/.local/bin/omaphoenix restore --from ~/phx-test/snap.git --no-restart $dry >~/restore.log 2>&1 </dev/null &"

echo "omaphoenix in the VM: $("$vm" ssh '~/.local/bin/omaphoenix --version')"
start=$(date +%s)
until ! "$vm" ssh 'pgrep -f "omaphoenix restor[e]" >/dev/null' 2>/dev/null; do sleep 30; done
echo "restore took $((($(date +%s) - start) / 60)) min"
"$vm" ssh 'grep -E "^(✓|✗|⏸)|^    (✗|⏸)|^(dry run|restore) of|^error:|VmHWM" ~/restore.log'

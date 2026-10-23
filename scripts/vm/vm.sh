#!/usr/bin/env bash
# A throwaway Omarchy VM for the full-restore test (docs/14). QEMU runs inside a
# Docker container (image built from scripts/vm/Dockerfile) with /dev/kvm, so
# nothing is installed on the host and no sudo is needed.
#
#   vm.sh install     fresh disk, boot the ISO with cidata: unattended install
#   vm.sh boot        boot the installed disk
#   vm.sh ssh [cmd]   run a command in the VM as the test user
#   vm.sh shot <png>  screenshot of the VM display
#   vm.sh save <tag>  qcow2 internal snapshot (VM must be stopped)
#   vm.sh revert <tag>
#   vm.sh stop
#
# State lives in $OMAPHOENIX_VM_DIR (default ~/.cache/omaphoenix-vm).
set -euo pipefail

dir=${OMAPHOENIX_VM_DIR:-$HOME/.cache/omaphoenix-vm}
iso=${OMAPHOENIX_VM_ISO:-$dir/omarchy-4.0.4.iso}
disk_bytes=$((64 * 1024 * 1024 * 1024))
name=omaphoenix-vm
port=2222
here=$(cd "$(dirname "$0")" && pwd)

in_box() { docker run --rm -i --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$dir:/w" -w /w omaphoenix-vm "$@"; }

qemu() { # extra qemu args...
  docker rm -f "$name" >/dev/null 2>&1 || true
  rm -f "$dir/qmp.sock"
  docker run -d --name "$name" --user "$(id -u):$(id -g)" --network host --device /dev/kvm -v "$dir:/w" -w /w omaphoenix-vm \
    qemu-system-x86_64 -enable-kvm -machine q35 -cpu host -smp 4 -m 6144 \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
    -drive if=pflash,format=raw,file=/w/OVMF_VARS.fd \
    -drive file=/w/disk.qcow2,if=none,id=d0,format=qcow2 -device virtio-blk-pci,drive=d0,bootindex=1 \
    -device virtio-vga -display none \
    -netdev user,id=n0,hostfwd=tcp:127.0.0.1:$port-:22 -device virtio-net-pci,netdev=n0 \
    -qmp unix:/w/qmp.sock,server,nowait -serial file:/w/serial.log \
    "$@" >/dev/null
  echo "VM running (container $name); ssh via: $0 ssh"
}

qmp() { # json command
  printf '{"execute":"qmp_capabilities"}\n%s\n' "$1" |
    docker exec -i "$name" socat - UNIX-CONNECT:/w/qmp.sock | tail -1
}

case ${1:-} in
install)
  docker build -q -t omaphoenix-vm "$here" >/dev/null
  in_box bash -c "rm -f disk.qcow2 && qemu-img create -q -f qcow2 disk.qcow2 $disk_bytes && cp /usr/share/edk2/x64/OVMF_VARS.4m.fd OVMF_VARS.fd"
  in_box bash -s -- /w "$disk_bytes" <"$here/make-cidata.sh"
  qemu -drive "file=/w/$(basename "$iso"),media=cdrom,readonly=on,if=none,id=c0" -device ide-cd,drive=c0,bus=ide.0,bootindex=2 \
    -drive file=/w/cidata.iso,media=cdrom,readonly=on,if=none,id=c1 -device ide-cd,drive=c1,bus=ide.1
  ;;
boot) qemu ;;
ssh)
  shift
  ssh -q -i "$dir/vm_key" -p $port -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -o ConnectTimeout=5 -o LogLevel=ERROR "${VM_USER:-phx}@127.0.0.1" "$@"
  ;;
shot)
  qmp '{"execute":"screendump","arguments":{"filename":"/w/shot.ppm"}}' >/dev/null
  docker exec "$name" magick /w/shot.ppm "/w/$(basename "$2")"
  echo "$dir/$(basename "$2")"
  ;;
stop)
  qmp '{"execute":"system_powerdown"}' >/dev/null 2>&1 || true
  for _ in $(seq 60); do docker ps -q -f name="^$name$" | grep -q . || break; sleep 2; done
  docker rm -f "$name" >/dev/null 2>&1 || true
  ;;
save) in_box qemu-img snapshot -c "$2" disk.qcow2 && echo "saved $2" ;;
revert) in_box qemu-img snapshot -a "$2" disk.qcow2 && echo "reverted to $2" ;;
*) sed -n '2,15p' "$0" ;;
esac

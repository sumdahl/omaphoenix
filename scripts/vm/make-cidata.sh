#!/usr/bin/env bash
# Build cidata.iso for an unattended Omarchy install into a test VM.
#
# The Omarchy ISO skips its setup wizard when a drive labelled "cidata" holds
# the wizard's own output files (see /usr/local/bin/omarchy-cidata-load on the
# ISO). user_configuration.json mirrors the full_disk template in the ISO's
# configurator (Omarchy 4.0.6; 4.0.6 added input_method and input_xkb_layout); regenerate it from there if a new ISO changes it.
#
# Usage: make-cidata.sh <workdir> <disk-bytes>
# Writes <workdir>/cidata.iso and <workdir>/vm_key (a throwaway SSH key that
# only ever unlocks the test VM). Nothing here is a real credential.
set -euo pipefail

work=$1
disk_size=$2
user=${VM_USER:-phx}
password=${VM_PASSWORD:-phoenix-test-only}
hostname=${VM_HOSTNAME:-phoenix-vm}
timezone=${VM_TIMEZONE:-Asia/Kathmandu}
disk=/dev/vda

mib=$((1024 * 1024))
gib=$((1024 * mib))
boot_start=$mib
boot_size=$((2 * gib))
main_start=$((boot_start + boot_size))
main_size=$((disk_size / mib * mib - main_start - mib))

dir=$work/cidata
rm -rf "$dir"
mkdir -p "$dir"
[[ -f $work/vm_key ]] || ssh-keygen -q -t ed25519 -N '' -C omaphoenix-vm-test -f "$work/vm_key"
cp "$work/vm_key.pub" "$dir/authorized_keys"

hash=$(openssl passwd -6 "$password")
cat >"$dir/user_credentials.json" <<EOF
{
    "root_enc_password": "$hash",
    "users": [
        { "enc_password": "$hash", "groups": [], "sudo": true, "username": "$user" }
    ]
}
EOF
echo "Phoenix Test" >"$dir/user_full_name.txt"
echo "phoenix@example.invalid" >"$dir/user_email_address.txt"
echo false >"$dir/user_encrypt_installation.txt"

part() { # fs mountpoint start size id flags btrfs mount_options
  cat <<EOF
{
    "btrfs": $7, "dev_path": null, "flags": $6, "fs_type": "$1",
    "mount_options": $8, "mountpoint": $2, "obj_id": "$5",
    "size": { "sector_size": { "unit": "B", "value": 512 }, "unit": "B", "value": $4 },
    "start": { "sector_size": { "unit": "B", "value": 512 }, "unit": "B", "value": $3 },
    "status": "create", "type": "primary"
}
EOF
}
subvols='[ { "mountpoint": "/", "name": "@" }, { "mountpoint": "/home", "name": "@home" },
           { "mountpoint": "/var/log", "name": "@log" }, { "mountpoint": "/var/cache/pacman/pkg", "name": "@pkg" } ]'

cat >"$dir/user_configuration.json" <<EOF
{
    "app_config": null,
    "archinstall-language": "English",
    "auth_config": {},
    "audio_config": { "audio": "pipewire" },
    "bootloader_config": { "bootloader": "Limine", "uki": false, "removable": false },
    "custom_commands": [],
    "omarchy_install": {
        "input_method": "none",
        "input_xkb_layout": "",
        "mode": "full_disk",
        "defer_provisioning": false,
        "target_mount": "/mnt",
        "boot": { "esp_mount": "/boot", "esp_path": "/EFI/limine", "efi_binary": "limine_x64.efi", "enable_fallback": true },
        "storage": { "kernel": "linux-omarchy" }
    },
    "disk_config": {
        "config_type": "default_layout",
        "device_modifications": [ {
            "device": "$disk",
            "partitions": [
                $(part fat32 '"/boot"' $boot_start $boot_size ea21d3f2-82bb-49cc-ab5d-6f81ae94e18d '[ "boot", "esp" ]' '[]' '[]'),
                $(part btrfs null $main_start $main_size 8c2c2b92-1070-455d-b76a-56263bab24aa '[]' "$subvols" '[ "compress=zstd" ]')
            ],
            "wipe": true
        } ]
    },
    "hostname": "$hostname",
    "kernels": [ "linux-omarchy" ],
    "network_config": { "type": "iso" },
    "ntp": true,
    "parallel_downloads": 8,
    "script": null,
    "services": [],
    "swap": true,
    "timezone": "$timezone",
    "locale_config": { "kb_layout": "us", "sys_enc": "UTF-8", "sys_lang": "en_US.UTF-8" },
    "mirror_config": {
        "custom_repositories": [],
        "custom_servers": [
            {"url": "https://mirror.omarchy.org/\$repo/os/\$arch"},
            {"url": "https://mirror.rackspace.com/archlinux/\$repo/os/\$arch"},
            {"url": "https://geo.mirror.pkgbuild.com/\$repo/os/\$arch"}
        ],
        "mirror_regions": {},
        "optional_repositories": []
    },
    "packages": [ "base-devel", "git", "omarchy-keyring", "omarchy-settings", "omarchy" ],
    "profile_config": { "gfx_driver": null, "greeter": null, "profile": {} },
    "version": "3.0.9"
}
EOF

python3 -c 'import json,sys; [json.load(open(f)) for f in sys.argv[1:]]' "$dir"/*.json
genisoimage -quiet -output "$work/cidata.iso" -volid cidata -joliet -rock "$dir"
echo "cidata.iso ready (user $user, host $hostname, disk $disk)"

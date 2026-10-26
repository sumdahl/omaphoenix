FROM archlinux:latest
RUN pacman -Syu --noconfirm --needed qemu-base qemu-hw-display-virtio-vga qemu-hw-display-virtio-gpu qemu-hw-display-virtio-gpu-pci edk2-ovmf openssh cdrtools python socat openssl imagemagick libarchive squashfs-tools && pacman -Scc --noconfirm

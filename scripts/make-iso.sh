#!/bin/bash
# Package kernel/target/.../kernel into target/vkrust.iso.
# Uses host xorriso/gcc when they exist, otherwise the vkrust-osdev image.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

have_tools() {
    command -v xorriso >/dev/null 2>&1 \
        && command -v gcc >/dev/null 2>&1 \
        && command -v git >/dev/null 2>&1 \
        && command -v make >/dev/null 2>&1
}

# The vkrust-osdev image ships gcc without libc headers. Install them when root.
if [[ "$(id -u)" -eq 0 && ! -f /usr/include/stdio.h ]] && command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y libc6-dev
fi

if ! have_tools; then
    if command -v docker >/dev/null 2>&1 && docker image inspect vkrust-osdev >/dev/null 2>&1; then
        exec docker run --rm -v "$ROOT:/src" -w /src vkrust-osdev bash scripts/make-iso.sh
    fi
    echo "make-iso: need gcc, make, git, and xorriso (or the vkrust-osdev docker image)" >&2
    exit 1
fi

KERNEL="kernel/target/x86_64-unknown-none/release/kernel"
if [[ ! -f "$KERNEL" ]]; then
    echo "make-iso: missing $KERNEL. Run \`make kernel\` first." >&2
    exit 1
fi

LIMINE="target/limine"
if [[ ! -x "$LIMINE/limine" ]]; then
    rm -rf "$LIMINE"
    git clone https://github.com/limine-bootloader/limine.git \
        --branch=v10.8.5-binary --depth=1 "$LIMINE"
    make -C "$LIMINE"
fi

STAGE="target/iso-root"
rm -rf "$STAGE"
mkdir -p "$STAGE/boot/limine" "$STAGE/EFI/BOOT"
cp "$KERNEL" "$STAGE/boot/kernel"
cp limine.conf "$STAGE/boot/limine/limine.conf"
cp "$LIMINE/limine-bios.sys" "$STAGE/boot/limine/"
cp "$LIMINE/limine-bios-cd.bin" "$STAGE/boot/limine/"
cp "$LIMINE/limine-uefi-cd.bin" "$STAGE/boot/limine/"
cp "$LIMINE/BOOTX64.EFI" "$STAGE/EFI/BOOT/"

mkdir -p target
xorriso -as mkisofs -R -r -J \
    -b boot/limine/limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    --efi-boot boot/limine/limine-uefi-cd.bin \
    -efi-boot-part --efi-boot-image --protective-msdos-label \
    "$STAGE" -o target/vkrust.iso
"$LIMINE/limine" bios-install target/vkrust.iso
echo "wrote target/vkrust.iso"

#!/bin/bash
# Package the UEFI loader and the kernel into target/vkrust.iso.
# The FAT image is built with host python3. xorriso runs on the host, or
# inside the vkrust-osdev image when the host does not have it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

KERNEL="kernel/target/x86_64-unknown-none/release/kernel"
LOADER="loader/target/x86_64-unknown-uefi/release/loader.efi"
if [[ ! -f "$KERNEL" ]]; then
    echo "make-iso: missing $KERNEL. Run \`make kernel\` first." >&2
    exit 1
fi
if [[ ! -f "$LOADER" ]]; then
    echo "make-iso: missing $LOADER. Run \`make loader\` first." >&2
    exit 1
fi

mkdir -p target
python3 scripts/make-fat.py "$LOADER" "$KERNEL" target/efi.img

# A previous docker run may have left root-owned files behind.
remove() {
    if [[ ! -e "$1" ]]; then
        return 0
    fi
    if rm -rf "$1"; then
        return 0
    fi
    docker run --rm -v "$ROOT:/src" -w /src vkrust-osdev rm -rf "$1"
}

STAGE="target/iso-root"
remove "$STAGE"
remove target/vkrust.iso
mkdir -p "$STAGE/EFI/BOOT" "$STAGE/boot"
cp target/efi.img "$STAGE/efi.img"
# The same files sit on the ISO9660 tree. OVMF sometimes hands the loader the
# CD volume instead of the FAT image, and either one can satisfy \boot\kernel.
cp "$LOADER" "$STAGE/EFI/BOOT/BOOTX64.EFI"
cp "$KERNEL" "$STAGE/boot/kernel"

XORRISO=(
    xorriso -as mkisofs -R -r -J
    --efi-boot efi.img
    -efi-boot-part --efi-boot-image --protective-msdos-label
    "$STAGE" -o target/vkrust.iso
)

if command -v xorriso >/dev/null 2>&1; then
    "${XORRISO[@]}"
elif command -v docker >/dev/null 2>&1 && docker image inspect vkrust-osdev >/dev/null 2>&1; then
    docker run --rm -u "$(id -u):$(id -g)" -v "$ROOT:/src" -w /src vkrust-osdev \
        xorriso -as mkisofs -R -r -J \
        --efi-boot efi.img \
        -efi-boot-part --efi-boot-image --protective-msdos-label \
        target/iso-root -o target/vkrust.iso
else
    echo "make-iso: need xorriso, or the vkrust-osdev docker image" >&2
    exit 1
fi
echo "wrote target/vkrust.iso"

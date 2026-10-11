#!/bin/bash
# Print the QEMU arguments that boot this image with OVMF, one argument per line.
# Prefer the combined firmware on the default pc machine. The kernel programs
# the 8259 PIC, which that machine has. Fall back to the split 4 MiB build on q35.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -f /usr/share/ovmf/OVMF.fd ]]; then
    printf '%s\n' -bios /usr/share/ovmf/OVMF.fd
    exit 0
fi
if [[ -f /usr/share/qemu/OVMF.fd ]]; then
    printf '%s\n' -bios /usr/share/qemu/OVMF.fd
    exit 0
fi

CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
VARS_SRC="/usr/share/OVMF/OVMF_VARS_4M.fd"
if [[ -f "$CODE" && -f "$VARS_SRC" ]]; then
    mkdir -p "$ROOT/target"
    VARS="$ROOT/target/ovmf-vars.fd"
    if [[ ! -f "$VARS" ]]; then
        cp "$VARS_SRC" "$VARS"
    fi
    printf '%s\n' \
        -machine q35 \
        -drive "if=pflash,format=raw,readonly=on,file=${CODE}" \
        -drive "if=pflash,format=raw,file=${VARS}"
    exit 0
fi

echo "qemu-fw: no OVMF firmware found (looked for OVMF.fd and OVMF_CODE_4M.fd)" >&2
exit 1

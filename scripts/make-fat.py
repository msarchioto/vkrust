#!/usr/bin/env python3
"""Write a FAT16 image containing \\EFI\\BOOT\\BOOTX64.EFI and \\boot\\kernel.

The loader opens the volume it was loaded from. On a UEFI CD that volume is
this FAT image, not the ISO9660 tree around it, so both files live here.
"""

import struct
import sys

SECTOR = 512
SPC = 4
CLUSTER = SECTOR * SPC
RESERVED = 1
FAT_COUNT = 2
ROOT_ENTRIES = 512
SPF = 32
TOTAL_SECTORS = 32768
ROOT_SECTORS = ROOT_ENTRIES * 32 // SECTOR
DATA_LBA = RESERVED + FAT_COUNT * SPF + ROOT_SECTORS
ATTR_DIR = 0x10
ATTR_FILE = 0x20


def main():
    if len(sys.argv) != 4:
        sys.exit("usage: make-fat.py BOOTX64.EFI kernel efi.img")
    loader = open(sys.argv[1], "rb").read()
    kernel = open(sys.argv[2], "rb").read()
    image = build(loader, kernel)
    check(image, loader, kernel)
    with open(sys.argv[3], "wb") as out:
        out.write(image)
    print(f"wrote {sys.argv[3]} ({len(image)} bytes)")


def build(loader, kernel):
    fat_slots = SPF * SECTOR // 2
    fat = [0] * fat_slots
    fat[0] = 0xFFF8
    fat[1] = 0xFFFF
    nxt = 2
    disk = bytearray(TOTAL_SECTORS * SECTOR)

    def alloc(count):
        nonlocal nxt
        if count < 1 or nxt + count > fat_slots:
            sys.exit("make-fat: image is too small for the loader and kernel")
        first = nxt
        for i in range(count):
            fat[nxt] = 0xFFFF if i == count - 1 else nxt + 1
            nxt += 1
        return first

    def write_cluster_bytes(cluster, data):
        off = cluster_offset(cluster)
        disk[off : off + len(data)] = data

    efi_dir = alloc(1)
    boot_dir = alloc(1)
    efi_boot_dir = alloc(1)
    loader_cluster = alloc(clusters_for(len(loader)))
    kernel_cluster = alloc(clusters_for(len(kernel)))
    write_cluster_bytes(loader_cluster, loader)
    write_cluster_bytes(kernel_cluster, kernel)

    write_cluster_bytes(
        efi_dir,
        b"".join(
            [
                entry(".", "", ATTR_DIR, efi_dir, 0),
                entry("..", "", ATTR_DIR, 0, 0),
                entry("BOOT", "", ATTR_DIR, efi_boot_dir, 0),
            ]
        ),
    )
    write_cluster_bytes(
        efi_boot_dir,
        b"".join(
            [
                entry(".", "", ATTR_DIR, efi_boot_dir, 0),
                entry("..", "", ATTR_DIR, efi_dir, 0),
                entry("BOOTX64", "EFI", ATTR_FILE, loader_cluster, len(loader)),
            ]
        ),
    )
    write_cluster_bytes(
        boot_dir,
        b"".join(
            [
                entry(".", "", ATTR_DIR, boot_dir, 0),
                entry("..", "", ATTR_DIR, 0, 0),
                entry("KERNEL", "", ATTR_FILE, kernel_cluster, len(kernel)),
            ]
        ),
    )

    root = bytearray(ROOT_ENTRIES * 32)
    root[0:32] = entry("EFI", "", ATTR_DIR, efi_dir, 0)
    root[32:64] = entry("BOOT", "", ATTR_DIR, boot_dir, 0)
    root_off = (RESERVED + FAT_COUNT * SPF) * SECTOR
    disk[root_off : root_off + len(root)] = root

    for copy in range(FAT_COUNT):
        off = (RESERVED + copy * SPF) * SECTOR
        for i, value in enumerate(fat):
            struct.pack_into("<H", disk, off + i * 2, value)

    disk[0:3] = b"\xeb\x3c\x90"
    disk[3:11] = b"VKRUST  "
    struct.pack_into("<H", disk, 11, SECTOR)
    disk[13] = SPC
    struct.pack_into("<H", disk, 14, RESERVED)
    disk[16] = FAT_COUNT
    struct.pack_into("<H", disk, 17, ROOT_ENTRIES)
    struct.pack_into("<H", disk, 19, TOTAL_SECTORS)
    disk[21] = 0xF8
    struct.pack_into("<H", disk, 22, SPF)
    struct.pack_into("<H", disk, 24, 32)
    struct.pack_into("<H", disk, 26, 64)
    disk[36] = 0x80
    disk[38] = 0x29
    struct.pack_into("<I", disk, 39, 0x564B5255)
    disk[43:54] = b"VKRUST     "
    disk[54:62] = b"FAT16   "
    disk[510] = 0x55
    disk[511] = 0xAA
    return disk


def clusters_for(size):
    return max(1, (size + CLUSTER - 1) // CLUSTER)


def cluster_offset(cluster):
    return (DATA_LBA + (cluster - 2) * SPC) * SECTOR


def entry(name, ext, attr, cluster, size):
    raw = bytearray(32)
    raw[0:8] = name.encode("ascii").ljust(8)
    raw[8:11] = ext.encode("ascii").ljust(3)
    raw[11] = attr
    struct.pack_into("<H", raw, 26, cluster & 0xFFFF)
    struct.pack_into("<I", raw, 28, size)
    return bytes(raw)


def check(image, loader, kernel):
    assert read_file(image, ["EFI", "BOOT", "BOOTX64.EFI"]) == loader
    assert read_file(image, ["BOOT", "KERNEL"]) == kernel


def read_file(image, parts):
    cluster = 0
    directory = True
    data = root_bytes(image)
    for part in parts:
        found = None
        for raw in iter_dir(image, data, cluster):
            name = raw[0:8].decode("ascii").rstrip(" ")
            ext = raw[8:11].decode("ascii").rstrip(" ")
            label = name if not ext else f"{name}.{ext}"
            if label == part:
                found = raw
                break
        if found is None:
            sys.exit(f"make-fat: missing {'/'.join(parts)}")
        cluster = struct.unpack_from("<H", found, 26)[0]
        size = struct.unpack_from("<I", found, 28)[0]
        directory = found[11] & ATTR_DIR != 0
        data = read_chain(image, cluster, None if directory else size)
    if directory:
        sys.exit(f"make-fat: {'/'.join(parts)} is a directory")
    return data


def root_bytes(image):
    off = (RESERVED + FAT_COUNT * SPF) * SECTOR
    return image[off : off + ROOT_ENTRIES * 32]


def iter_dir(image, data, cluster):
    # `data` is the whole directory. Cluster 0 means the root, already loaded.
    del image, cluster
    for off in range(0, len(data), 32):
        raw = data[off : off + 32]
        if raw[0] == 0:
            break
        if raw[0] == 0xE5:
            continue
        yield raw


def read_chain(image, cluster, size):
    out = bytearray()
    seen = set()
    while cluster < 0xFFF8:
        if cluster in seen or cluster < 2:
            sys.exit("make-fat: broken cluster chain")
        seen.add(cluster)
        off = cluster_offset(cluster)
        out += image[off : off + CLUSTER]
        cluster = struct.unpack_from("<H", image, (RESERVED * SECTOR) + cluster * 2)[0]
        if size is not None and len(out) >= size:
            return bytes(out[:size])
    if size is None:
        return bytes(out)
    return bytes(out[:size])


if __name__ == "__main__":
    main()

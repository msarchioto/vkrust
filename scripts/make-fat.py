#!/usr/bin/env python3
"""Write a FAT12 image containing \\EFI\\BOOT\\BOOTX64.EFI and \\boot\\kernel.

The image has one cluster per directory and however many clusters each file
needs. UEFI calls a volume FAT12 while it has fewer than 4,085 clusters, so
the cluster size doubles if the files would cross that line.

The loader opens the volume it was loaded from. On a UEFI CD that volume is
this FAT image, not the ISO9660 tree around it, so both files live here.
"""

import struct
import sys

SECTOR = 512
RESERVED = 1
FAT_COUNT = 2
ROOT_ENTRIES = 16
ROOT_SECTORS = (ROOT_ENTRIES * 32 + SECTOR - 1) // SECTOR
# Microsoft's count: below this many clusters the volume is FAT12.
FAT12_MAX_CLUSTERS = 4085
ATTR_DIR = 0x10
ATTR_FILE = 0x20
EOC = 0xFFF


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
    spc, count = choose_clusters(len(loader), len(kernel))
    cluster = SECTOR * spc
    spf = fat_sectors(count)
    total = RESERVED + FAT_COUNT * spf + ROOT_SECTORS + count * spc
    if total >= 65536:
        sys.exit("make-fat: image does not fit in a 16-bit sector count")
    disk = bytearray(total * SECTOR)
    data_lba = RESERVED + FAT_COUNT * spf + ROOT_SECTORS

    nxt = 2

    def alloc(n):
        nonlocal nxt
        if n < 1 or nxt + n > count + 2:
            sys.exit("make-fat: cluster count does not cover the files")
        first = nxt
        for i in range(n):
            fat_set(disk, spf, nxt, EOC if i == n - 1 else nxt + 1)
            nxt += 1
        return first

    def write_at(cluster_no, data):
        off = (data_lba + (cluster_no - 2) * spc) * SECTOR
        disk[off : off + len(data)] = data

    efi_dir = alloc(1)
    boot_dir = alloc(1)
    efi_boot_dir = alloc(1)
    loader_cluster = alloc(max(1, (len(loader) + cluster - 1) // cluster))
    kernel_cluster = alloc(max(1, (len(kernel) + cluster - 1) // cluster))
    write_at(loader_cluster, loader)
    write_at(kernel_cluster, kernel)
    write_at(
        efi_dir,
        b"".join(
            [
                entry(".", "", ATTR_DIR, efi_dir, 0),
                entry("..", "", ATTR_DIR, 0, 0),
                entry("BOOT", "", ATTR_DIR, efi_boot_dir, 0),
            ]
        ),
    )
    write_at(
        efi_boot_dir,
        b"".join(
            [
                entry(".", "", ATTR_DIR, efi_boot_dir, 0),
                entry("..", "", ATTR_DIR, efi_dir, 0),
                entry("BOOTX64", "EFI", ATTR_FILE, loader_cluster, len(loader)),
            ]
        ),
    )
    write_at(
        boot_dir,
        b"".join(
            [
                entry(".", "", ATTR_DIR, boot_dir, 0),
                entry("..", "", ATTR_DIR, 0, 0),
                entry("KERNEL", "", ATTR_FILE, kernel_cluster, len(kernel)),
            ]
        ),
    )

    fat_set(disk, spf, 0, 0xFF8)
    fat_set(disk, spf, 1, EOC)
    fat1 = disk[RESERVED * SECTOR : (RESERVED + spf) * SECTOR]
    disk[(RESERVED + spf) * SECTOR : (RESERVED + 2 * spf) * SECTOR] = fat1

    root = bytearray(ROOT_ENTRIES * 32)
    root[0:32] = entry("EFI", "", ATTR_DIR, efi_dir, 0)
    root[32:64] = entry("BOOT", "", ATTR_DIR, boot_dir, 0)
    root_off = (RESERVED + FAT_COUNT * spf) * SECTOR
    disk[root_off : root_off + len(root)] = root

    disk[0:3] = b"\xeb\x3c\x90"
    disk[3:11] = b"VKRUST  "
    struct.pack_into("<H", disk, 11, SECTOR)
    disk[13] = spc
    struct.pack_into("<H", disk, 14, RESERVED)
    disk[16] = FAT_COUNT
    struct.pack_into("<H", disk, 17, ROOT_ENTRIES)
    struct.pack_into("<H", disk, 19, total)
    disk[21] = 0xF8
    struct.pack_into("<H", disk, 22, spf)
    struct.pack_into("<H", disk, 24, 32)
    struct.pack_into("<H", disk, 26, 64)
    disk[36] = 0x80
    disk[38] = 0x29
    struct.pack_into("<I", disk, 39, 0x564B5255)
    disk[43:54] = b"VKRUST     "
    disk[54:62] = b"FAT12   "
    disk[510] = 0x55
    disk[511] = 0xAA
    return disk


def choose_clusters(loader_len, kernel_len):
    spc = 1
    while spc <= 128:
        cluster = SECTOR * spc
        count = 3 + blocks(loader_len, cluster) + blocks(kernel_len, cluster)
        if count < FAT12_MAX_CLUSTERS:
            return spc, count
        spc *= 2
    sys.exit("make-fat: files are too large for a FAT12 image")


def blocks(size, cluster):
    return max(1, (size + cluster - 1) // cluster)


def fat_sectors(cluster_count):
    entries = cluster_count + 2
    fat_bytes = (entries * 3 + 1) // 2
    return (fat_bytes + SECTOR - 1) // SECTOR


def fat_offset(cluster):
    return cluster + cluster // 2


def fat_set(disk, spf, cluster, value):
    base = RESERVED * SECTOR
    # A 12-bit store touches two bytes. Keep the spare nibble.
    off = base + fat_offset(cluster)
    if cluster & 1:
        disk[off] = (disk[off] & 0x0F) | ((value & 0x0F) << 4)
        disk[off + 1] = (value >> 4) & 0xFF
    else:
        disk[off] = value & 0xFF
        disk[off + 1] = (disk[off + 1] & 0xF0) | ((value >> 8) & 0x0F)
    if off + 1 >= (RESERVED + spf) * SECTOR:
        sys.exit("make-fat: FAT12 entry runs past the table")


def fat_get(image, cluster):
    off = RESERVED * SECTOR + fat_offset(cluster)
    if cluster & 1:
        return (image[off] >> 4) | (image[off + 1] << 4)
    return image[off] | ((image[off + 1] & 0x0F) << 8)


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
    geo = geometry(image)
    clusters = (geo["total"] - geo["data_lba"]) // geo["spc"]
    if clusters >= FAT12_MAX_CLUSTERS:
        sys.exit("make-fat: volume is not FAT12")
    if image[54:62] != b"FAT12   ":
        sys.exit("make-fat: filesystem type is not FAT12")


def geometry(image):
    spc = image[13]
    reserved = struct.unpack_from("<H", image, 14)[0]
    fats = image[16]
    root_entries = struct.unpack_from("<H", image, 17)[0]
    total = struct.unpack_from("<H", image, 19)[0] or struct.unpack_from("<I", image, 32)[0]
    spf = struct.unpack_from("<H", image, 22)[0]
    root_sectors = (root_entries * 32 + SECTOR - 1) // SECTOR
    data_lba = reserved + fats * spf + root_sectors
    return {
        "spc": spc,
        "reserved": reserved,
        "spf": spf,
        "root_entries": root_entries,
        "total": total,
        "data_lba": data_lba,
        "cluster": SECTOR * spc,
    }


def read_file(image, parts):
    geo = geometry(image)
    cluster = 0
    directory = True
    data = root_bytes(image, geo)
    for part in parts:
        found = None
        for raw in iter_dir(data):
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
        data = read_chain(image, geo, cluster, None if directory else size)
    if directory:
        sys.exit(f"make-fat: {'/'.join(parts)} is a directory")
    return data


def root_bytes(image, geo):
    off = geo["data_lba"] * SECTOR - ROOT_SECTORS * SECTOR
    return image[off : off + geo["root_entries"] * 32]


def iter_dir(data):
    for off in range(0, len(data), 32):
        raw = data[off : off + 32]
        if not raw or raw[0] == 0:
            break
        if raw[0] == 0xE5:
            continue
        yield raw


def read_chain(image, geo, cluster, size):
    out = bytearray()
    seen = set()
    while cluster < 0xFF8:
        if cluster in seen or cluster < 2:
            sys.exit("make-fat: broken cluster chain")
        seen.add(cluster)
        off = (geo["data_lba"] + (cluster - 2) * geo["spc"]) * SECTOR
        out += image[off : off + geo["cluster"]]
        cluster = fat_get(image, cluster)
        if size is not None and len(out) >= size:
            return bytes(out[:size])
    if size is None:
        return bytes(out)
    return bytes(out[:size])


if __name__ == "__main__":
    main()

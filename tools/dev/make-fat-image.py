"""Image FAT16 de test pour The Sleuth Kit (crates/recovery/tests/real_tsk.rs) : un fichier présent et
deux fichiers supprimés comme le fait Windows (premier octet du nom remplacé par 0xE5, grappes libérées
dans la FAT, données laissées en place).

Usage : python tools/dev/make-fat-image.py <image.img>
"""
import struct
import sys
import zlib

SECTOR = 512
SPC = 4                      # secteurs par grappe (2 Kio)
RESERVED = 1
FATS = 2
ROOT_ENTRIES = 512
TOTAL = 32768                # 16 Mio
FAT_SECTORS = 32
ROOT_SECTORS = ROOT_ENTRIES * 32 // SECTOR
FIRST_DATA = RESERVED + FATS * FAT_SECTORS + ROOT_SECTORS
CLUSTER = SPC * SECTOR


def png(w=64, h=48):
    rows = b"".join(
        b"\x00" + b"".join(bytes(((x * 4) % 256, (y * 5) % 256, ((x + y) * 3) % 256)) for x in range(w))
        for y in range(h)
    )

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def pdf():
    return (b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n"
            b"2 0 obj << /Type /Pages /Kids [] /Count 0 >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n")


image = bytearray(TOTAL * SECTOR)
boot = bytearray(SECTOR)
boot[0:3] = b"\xEB\x3C\x90"
boot[3:11] = b"MSDOS5.0"
struct.pack_into("<HBHBHHBHHHII", boot, 11, SECTOR, SPC, RESERVED, FATS, ROOT_ENTRIES, TOTAL, 0xF8,
                 FAT_SECTORS, 63, 255, 0, 0)
struct.pack_into("<BBBI11s8s", boot, 36, 0x80, 0, 0x29, 0x12345678, b"PCCHECK    ", b"FAT16   ")
boot[510:512] = b"\x55\xAA"
image[0:SECTOR] = boot

fat = bytearray(FAT_SECTORS * SECTOR)
struct.pack_into("<HH", fat, 0, 0xFFF8, 0xFFFF)
root = bytearray(ROOT_SECTORS * SECTOR)
next_cluster = 2
entry = 0


def add(name, ext, data, deleted):
    """Écrit le fichier, et sa chaîne dans la FAT seulement s'il est présent."""
    global next_cluster, entry
    first = next_cluster
    count = max(1, -(-len(data) // CLUSTER))
    for i in range(count):
        c = first + i
        off = (FIRST_DATA + (c - 2) * SPC) * SECTOR
        part = data[i * CLUSTER:(i + 1) * CLUSTER]
        image[off:off + len(part)] = part
        if not deleted:
            struct.pack_into("<H", fat, c * 2, 0xFFFF if i == count - 1 else c + 1)
    next_cluster += count
    raw_name = name.ljust(8).encode() + ext.ljust(3).encode()
    if deleted:
        raw_name = b"\xE5" + raw_name[1:]
    struct.pack_into("<11sB10xHHHI", root, entry * 32, raw_name, 0x20, 0, 0x5B21, first, len(data))
    entry += 1


add("GARDE", "TXT", b"Fichier toujours present.\r\n" * 40, deleted=False)
add("PHOTO", "PNG", png(), deleted=True)
add("RAPPORT", "PDF", pdf(), deleted=True)

for i in range(FATS):
    off = (RESERVED + i * FAT_SECTORS) * SECTOR
    image[off:off + len(fat)] = fat
off = (RESERVED + FATS * FAT_SECTORS) * SECTOR
image[off:off + len(root)] = root

with open(sys.argv[1], "wb") as f:
    f.write(image)
print(sys.argv[1], "FAT16 16 Mio : GARDE.TXT présent, PHOTO.PNG et RAPPORT.PDF supprimés")

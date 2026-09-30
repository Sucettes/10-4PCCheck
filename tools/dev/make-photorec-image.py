"""Image disque de test pour PhotoRec (crates/recovery/tests/real_photorec.rs) : fichiers « supprimés » posés à des secteurs, sans système de
fichiers (comme des données restées sur le disque après suppression)."""
import io, os, struct, sys, zipfile, zlib

# Usage : python tools/dev/make-photorec-image.py <image.dd>
out = sys.argv[1]
SIZE = 64 << 20


def png(w=64, h=48):
    raw = b"".join(b"\x00" + bytes((x * 4 % 256, y * 5 % 256, (x + y) % 256) * 1)[:3] * 1 for y in range(h) for x in range(1))  # placeholder
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            row += bytes(((x * 4) % 256, (y * 5) % 256, ((x + y) * 3) % 256))
        rows.append(bytes(row))
    data = zlib.compress(b"".join(rows), 9)

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", data) + chunk(b"IEND", b"")


def pdf():
    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        None,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    stream = b"BT /F1 24 Tf 72 700 Td (Fichier de test PhotoRec) Tj ET"
    objs[3] = b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream"
    body = b"%PDF-1.4\n"
    offsets = []
    for i, o in enumerate(objs, 1):
        offsets.append(len(body))
        body += b"%d 0 obj\n" % i + o + b"\nendobj\n"
    xref = len(body)
    body += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for off in offsets:
        body += b"%010d 00000 n \n" % off
    body += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
    return body


def zip_doc():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("notes.txt", "Document de test pour PhotoRec.\n" * 200)
        z.writestr("autre.txt", "Deuxième fichier de l'archive.\n" * 100)
    return buf.getvalue()


image = bytearray(SIZE)
files = [(1 << 20, png(), "png"), (9 << 20, pdf(), "pdf"), (20 << 20, zip_doc(), "zip"), (40 << 20, png(200, 120), "png")]
for off, data, kind in files:
    image[off:off + len(data)] = data
with open(out, "wb") as f:
    f.write(image)
print(out, [(k, len(d)) for _, d, k in files])

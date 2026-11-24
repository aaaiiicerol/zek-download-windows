#!/usr/bin/env python3
"""Gera assets/icon.ico (16..256 px) e assets/icon64.rgba (ícone da janela) com o "Z" do Zek Download."""
import math, struct, zlib, pathlib

def render(size, ss=3):
    n = size * ss
    s = n / 128.0
    pts = [(40, 40), (88, 40), (40, 84), (88, 84)]
    segs = [(pts[i], pts[i + 1]) for i in range(3)]
    half = 6 * s

    def dist_seg(px, py, ax, ay, bx, by):
        dx, dy = bx - ax, by - ay
        t = max(0, min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
        return math.hypot(px - (ax + t * dx), py - (ay + t * dy))

    rows = []
    for y in range(n):
        row = []
        for x in range(n):
            gx, gy = x / s, y / s
            cx, cy = min(max(gx, 36), 92), min(max(gy, 36), 92)
            inside = math.hypot(gx - cx, gy - cy) <= 28 and 8 <= gx <= 120 and 8 <= gy <= 120
            if not inside:
                row.append((0, 0, 0, 0)); continue
            t = (gx + gy - 16) / 224.0
            r = int(99 + (34 - 99) * t); g = int(102 + (184 - 102) * t); b = int(241 + (224 - 241) * t)
            white = any(dist_seg(x, y, a[0] * s, a[1] * s, c[0] * s, c[1] * s) <= half for a, c in segs)
            if 36 * s <= x <= 92 * s and 98 * s <= y <= 105 * s:
                white = True
            row.append((255, 255, 255, 255) if white else (r, g, b, 255))
        rows.append(row)
    out = []
    for y in range(size):
        line = []
        for x in range(size):
            acc = [0, 0, 0, 0]
            for dy in range(ss):
                for dx in range(ss):
                    p = rows[y * ss + dy][x * ss + dx]
                    a = p[3]
                    acc[0] += p[0] * a; acc[1] += p[1] * a; acc[2] += p[2] * a; acc[3] += a
            if acc[3] == 0:
                line.append((0, 0, 0, 0))
            else:
                line.append((acc[0] // acc[3], acc[1] // acc[3], acc[2] // acc[3], acc[3] // (ss * ss)))
        out.append(line)
    return out

def png(size, px):
    raw = b"".join(b"\x00" + b"".join(bytes(p) for p in row) for row in px)
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

here = pathlib.Path(__file__).parent
sizes = [16, 32, 48, 64, 128, 256]
imgs = {s: render(s, 4 if s <= 64 else 2) for s in sizes}
pngs = [png(s, imgs[s]) for s in sizes]
head = struct.pack("<HHH", 0, 1, len(sizes))
off = 6 + 16 * len(sizes)
entries, body = b"", b""
for s, data in zip(sizes, pngs):
    entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), off + len(body))
    body += data
(here / "icon.ico").write_bytes(head + entries + body)
(here / "icon64.rgba").write_bytes(b"".join(bytes(p) for row in imgs[64] for p in row))
(here / "icon256.png").write_bytes(pngs[-1])
print("ok", len(head + entries + body), "bytes ico;", len(imgs[64]) * 64 * 4, "bytes rgba")

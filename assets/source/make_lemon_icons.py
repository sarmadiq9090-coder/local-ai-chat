# Old icon options (not used). The app icon (assets/icon.ico, icon.png) is FastKiwi brand v2: assets/source/lemon*.svg.
"""Generate 4 lemon-themed icon options for "Lemon — Local AI Chat".

Each option is drawn on a 1024px design grid, rendered at SS x supersampling,
then downscaled with LANCZOS to every ICO size.

Usage:  python assets/lemon-options/make_lemon_icons.py   (requires Pillow only)
Writes (next to this script):
  option{1..4}.png  (256 px)
  option{1..4}.ico  (16, 24, 32, 48, 64, 128, 256)
  preview.png       (all options at 256 + 48/32/16 on dark and light strips)

Options:
  1. Lemon chat bubble: whole lemon with a bubble tail + leaf, on a soft cream tile.
  2. Lemon slice: circular cross-section with segments and an AI spark in the center.
  3. Code lemon: bold yellow lemon outline with { } inside, on a dark tile.
  4. Lemon buddy: friendly lemon character with a "..." speech bubble, on a sky tile.
"""

import math
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
SIZE = 1024
SS = 4
W = SIZE * SS
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


# ---------------------------------------------------------------- helpers
def s(v):
    return int(round(v * SS))


def hexc(h, a=255):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), a)


def new_mask():
    return Image.new("L", (W, W), 0)


def poly_mask(pts):
    m = new_mask()
    ImageDraw.Draw(m).polygon([(s(x), s(y)) for x, y in pts], fill=255)
    return m


def ellipse_mask(box):
    m = new_mask()
    ImageDraw.Draw(m).ellipse([s(v) for v in box], fill=255)
    return m


def rrect_mask(box, r):
    m = new_mask()
    ImageDraw.Draw(m).rounded_rectangle([s(v) for v in box], radius=s(r), fill=255)
    return m


def union(*ms):
    out = ms[0].copy()
    for m in ms[1:]:
        out.paste(255, mask=m)
    return out


def subtract(a, b):
    out = a.copy()
    out.paste(0, mask=b)
    return out


def intersect(a, b):
    out = new_mask()
    out.paste(a, mask=b)
    return out


def dilate(m, r):
    """Approximate round dilation by r design px (blur + threshold)."""
    b = m.filter(ImageFilter.GaussianBlur(s(r) / 2))
    return b.point(lambda v: 255 if v > 6 else 0)


def erode(m, r):
    b = m.filter(ImageFilter.GaussianBlur(s(r) / 2))
    return b.point(lambda v: 255 if v > 249 else 0)


def gradient(c1, c2, angle=0):
    """Linear gradient image c1 -> c2; angle 0 = top->bottom, 90 = left->right... (ccw)."""
    g = Image.linear_gradient("L").resize((W * 2, W * 2), Image.BILINEAR)
    g = g.rotate(angle, resample=Image.BILINEAR)
    g = g.crop((W // 2, W // 2, W // 2 + W, W // 2 + W))
    a = Image.new("RGBA", (W, W), c1)
    b = Image.new("RGBA", (W, W), c2)
    return Image.composite(b, a, g)


def radial(c_in, c_out, cx, cy, r):
    g = Image.radial_gradient("L").resize((s(2 * r), s(2 * r)), Image.BILINEAR)
    full = Image.new("L", (W, W), 255)
    full.paste(g, (s(cx - r), s(cy - r)))
    a = Image.new("RGBA", (W, W), c_in)
    b = Image.new("RGBA", (W, W), c_out)
    return Image.composite(b, a, full)


def fill(img, mask, paint, opacity=1.0):
    if isinstance(paint, tuple):
        paint = Image.new("RGBA", (W, W), paint)
    if opacity < 1.0:
        mask = mask.point(lambda v: int(v * opacity))
    layer = paint.copy()
    layer.putalpha(ImageChops.multiply(paint.getchannel("A"), mask))
    img.alpha_composite(layer)


def rot(pts, cx, cy, deg):
    a = math.radians(deg)
    c, sn = math.cos(a), math.sin(a)
    return [(cx + x * c - y * sn, cy + x * sn + y * c) for x, y in pts]


def lemon_pts(cx, cy, a, b, deg, nub=0.13, steps=720):
    """Lemon outline: ellipse with pointed nubs at both ends of the long axis."""
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        d0 = min(abs(t), abs(t - 2 * math.pi))
        d1 = abs(t - math.pi)
        f = 1 + nub * (math.exp(-(d0 / 0.22) ** 2) + math.exp(-(d1 / 0.22) ** 2))
        # slightly fuller body
        x = a * math.cos(t) * f
        y = b * math.copysign(abs(math.sin(t)) ** 0.9, math.sin(t))
        pts.append((x, y))
    return rot(pts, cx, cy, deg)


def leaf_pts(bx, by, length, width, deg, steps=120):
    """Leaf from base (bx,by) along direction deg (0 = pointing right)."""
    top, bot = [], []
    for i in range(steps + 1):
        u = i / steps
        w = width * math.sin(math.pi * u) ** 0.85 * (1 - 0.25 * u)
        top.append((u * length, -w))
        bot.append((u * length, w * 0.8))
    pts = top + bot[::-1]
    return rot(pts, bx, by, deg)


def astroid(cx, cy, r, steps=360, k=3):
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, sn = math.cos(t), math.sin(t)
        pts.append((cx + r * math.copysign(abs(c) ** k, c),
                    cy + r * math.copysign(abs(sn) ** k, sn)))
    return pts


def thick_line_mask(pts, width):
    """Polyline with round joins/caps (design coords)."""
    m = new_mask()
    d = ImageDraw.Draw(m)
    sp = [(s(x), s(y)) for x, y in pts]
    d.line(sp, fill=255, width=s(width), joint="curve")
    r = s(width) / 2
    for x, y in (sp[0], sp[-1]):
        d.ellipse([x - r, y - r, x + r, y + r], fill=255)
    return m


def finish(img):
    return img.resize((SIZE, SIZE), Image.LANCZOS)


# ---------------------------------------------------------------- palette
YEL_L = hexc("#FFE45C")
YEL = hexc("#FFD43B")
YEL_D = hexc("#F4C430")
YEL_DD = hexc("#E0A100")
RIND = hexc("#C98A00")
GREEN = hexc("#4CAF50")
GREEN_L = hexc("#7BD67F")
GREEN_D = hexc("#2E7D32")
DARK = hexc("#1F1E1D")
WHITE = hexc("#FFFFFF")


# ---------------------------------------------------------------- option 1
def option1():
    """Lemon chat bubble with leaf on a soft cream tile."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    tile = rrect_mask((24, 24, 1000, 1000), 220)
    fill(img, tile, gradient(hexc("#FFF6D6"), hexc("#FFE49A"), 0))

    lemon = poly_mask(lemon_pts(520, 470, 360, 280, -18, nub=0.12))
    tail = poly_mask([(300, 560), (175, 865), (470, 690)])
    body = union(lemon, tail)

    # soft drop shadow
    sh = body.filter(ImageFilter.GaussianBlur(s(18)))
    sh = sh.transform(sh.size, Image.AFFINE, (1, 0, 0, 0, 1, -s(18)))
    fill(img, intersect(sh, tile), hexc("#B07A00"), 0.35)

    # rind outline then body
    fill(img, dilate(body, 26), RIND)
    fill(img, body, gradient(YEL_L, YEL_D, 35))
    # glossy highlight
    hl = poly_mask(lemon_pts(430, 370, 170, 60, -22, nub=0.0))
    fill(img, intersect(hl, body), WHITE, 0.55)

    # leaf + stem at top-right tip
    leaf = poly_mask(leaf_pts(800, 300, 260, 100, -50))
    fill(img, dilate(leaf, 18), GREEN_D)
    fill(img, leaf, gradient(GREEN_L, GREEN, 30))
    vein = thick_line_mask(rot([(30, 0), (200, 0)], 800, 300, -50), 22)
    fill(img, intersect(vein, leaf), GREEN_D, 0.8)
    return finish(img)


# ---------------------------------------------------------------- option 2
def option2():
    """Circular lemon slice with segments and an AI spark at the center."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    cx = cy = 512
    R = 488
    outer = ellipse_mask((cx - R, cy - R, cx + R, cy + R))
    fill(img, outer, gradient(hexc("#FFD43B"), hexc("#F2A900"), 30))
    pith_r = 430
    pith = ellipse_mask((cx - pith_r, cy - pith_r, cx + pith_r, cy + pith_r))
    fill(img, pith, hexc("#FFFBE8"))

    flesh_r = 395
    n = 8
    gap = 13  # half-width of white membranes
    core_r = 160
    for i in range(n):
        a0 = 2 * math.pi * i / n
        a1 = 2 * math.pi * (i + 1) / n
        pts = []
        steps = 60
        for j in range(steps + 1):
            a = a0 + (a1 - a0) * j / steps
            pts.append((cx + flesh_r * math.cos(a), cy + flesh_r * math.sin(a)))
        pts.append((cx, cy))
        seg = poly_mask(pts)
        # shrink each wedge away from its two borders -> white membranes
        for a in (a0, a1):
            ln = thick_line_mask([(cx, cy), (cx + 600 * math.cos(a), cy + 600 * math.sin(a))], gap * 2)
            seg = subtract(seg, ln)
        seg = erode(seg, 8)
        mid = (a0 + a1) / 2
        g = radial(hexc("#FFF08A"), hexc("#FFC21A"), cx, cy, flesh_r + 40)
        fill(img, seg, g)
        # juicy highlight streak in each segment
        hx, hy = cx + 270 * math.cos(mid), cy + 270 * math.sin(mid)
        streak = thick_line_mask([(cx + 205 * math.cos(mid), cy + 205 * math.sin(mid)), (hx, hy)], 26)
        fill(img, intersect(streak, seg), WHITE, 0.45)

    # center: white core with a bold amber spark
    core = ellipse_mask((cx - core_r, cy - core_r, cx + core_r, cy + core_r))
    fill(img, core, hexc("#FFFDF2"))
    star = poly_mask(astroid(cx, cy, 165, k=2.6))
    fill(img, star, gradient(hexc("#FF9F1C"), hexc("#E86A00"), 30))
    # rind sheen
    sheen = subtract(ellipse_mask((cx - R, cy - R, cx + R, cy + R)), pith)
    sheen = intersect(sheen, ellipse_mask((cx - R - 60, cy - R - 60, cx + R - 160, cy + R - 160)))
    fill(img, sheen, WHITE, 0.25)
    return finish(img)


# ---------------------------------------------------------------- option 3
def option3():
    """Bold geometric lemon outline with { } inside, on a dark tile."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    tile = rrect_mask((24, 24, 1000, 1000), 220)
    fill(img, tile, gradient(hexc("#2A2826"), DARK, 0))

    lem_pts = lemon_pts(512, 530, 395, 300, -20, nub=0.11)
    outer = poly_mask(lem_pts)
    inner = erode(outer, 78)
    ring = subtract(outer, inner)
    fill(img, ring, gradient(YEL_L, YEL_D, 35))
    # faint yellow glow inside
    fill(img, inner, YEL, 0.08)

    # leaf on top-right
    leaf = poly_mask(leaf_pts(820, 335, 210, 85, -60))
    fill(img, dilate(leaf, 22), DARK)
    fill(img, leaf, gradient(GREEN_L, GREEN, 30))

    # { } braces: heavy round strokes built from quadratic curves
    def qbez(p0, c, p1, n=24):
        return [((1 - u) ** 2 * p0[0] + 2 * (1 - u) * u * c[0] + u * u * p1[0],
                 (1 - u) ** 2 * p0[1] + 2 * (1 - u) * u * c[1] + u * u * p1[1])
                for u in (i / n for i in range(n + 1))]

    h = 150
    left_brace = (qbez((50, -h), (0, -h), (0, -h + 55)) + [(0, -50)]
                  + qbez((0, -50), (0, 0), (-50, 0))
                  + qbez((-50, 0), (0, 0), (0, 50)) + [(0, h - 55)]
                  + qbez((0, h - 55), (0, h), (50, h)))
    for sx, ox in ((1, 400), (-1, 624)):
        pts = [(ox + sx * x, 530 + y) for x, y in left_brace]
        fill(img, thick_line_mask(pts, 60), WHITE)
    return finish(img)


# ---------------------------------------------------------------- option 4
def option4():
    """Friendly lemon character with a speech bubble on a sky tile."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    tile = rrect_mask((24, 24, 1000, 1000), 220)
    fill(img, tile, gradient(hexc("#5EC8F2"), hexc("#2F8FD8"), 0))

    # speech bubble (top-right)
    bub = union(rrect_mask((520, 90, 950, 380), 140),
                poly_mask([(560, 320), (470, 440), (680, 360)]))
    sh = bub.filter(ImageFilter.GaussianBlur(s(14)))
    sh = sh.transform(sh.size, Image.AFFINE, (1, 0, 0, 0, 1, -s(12)))
    fill(img, intersect(sh, tile), hexc("#0B4F8A"), 0.35)
    fill(img, bub, WHITE)
    for i in range(3):
        x = 625 + i * 110
        fill(img, ellipse_mask((x - 44, 235 - 44, x + 44, 235 + 44)), hexc("#1F7BC8"))

    # lemon body (bottom-left), leaning
    lc = (430, 630)
    body = poly_mask(lemon_pts(lc[0], lc[1], 345, 285, -28, nub=0.12))
    sh = body.filter(ImageFilter.GaussianBlur(s(16)))
    sh = sh.transform(sh.size, Image.AFFINE, (1, 0, 0, 0, 1, -s(16)))
    fill(img, intersect(sh, tile), hexc("#0B4F8A"), 0.35)
    fill(img, body, radial(YEL_L, hexc("#F2B705"), 360, 540, 470))
    hl = poly_mask(lemon_pts(330, 500, 150, 48, -30, nub=0.0))
    fill(img, intersect(hl, body), WHITE, 0.5)

    # leaf at top tip
    leaf = poly_mask(leaf_pts(760, 455, 190, 80, -15))
    fill(img, leaf, gradient(GREEN_L, GREEN, 30))

    # face
    face = hexc("#3B2A12")
    for ex in (345, 505):
        ey = 615 - (ex - 425) * 0.32
        fill(img, ellipse_mask((ex - 38, ey - 52, ex + 38, ey + 52)), face)
        fill(img, ellipse_mask((ex - 12, ey - 34, ex + 12, ey - 10)), WHITE)
    smile = []
    for i in range(41):
        t = math.pi * (0.15 + 0.7 * i / 40)
        smile.append((425 + 85 * math.cos(t), 690 + 60 * math.sin(t)))
    smile = rot([(x - 425, y - 700) for x, y in smile], 425, 700, -18)
    fill(img, thick_line_mask(smile, 34), face)
    for ex, ey in ((275, 745), (580, 650)):
        fill(img, ellipse_mask((ex - 48, ey - 30, ex + 48, ey + 30)), hexc("#FF7A59"), 0.55)
    return finish(img)


# ---------------------------------------------------------------- output
def save(img, n):
    img.resize((256, 256), Image.LANCZOS).save(HERE / f"option{n}.png")
    img.save(HERE / f"option{n}.ico", sizes=[(z, z) for z in ICO_SIZES])


def preview(imgs):
    pad = 24
    cell = 256 + pad
    pw = pad + cell * len(imgs)
    small = [48, 32, 16]
    strip_h = 48 + 2 * 16
    ph = pad + 256 + 40 + 2 * strip_h + pad
    out = Image.new("RGBA", (pw, ph), (0x80, 0x80, 0x80, 255))
    d = ImageDraw.Draw(out)
    try:
        font = ImageFont.load_default(size=30)
    except TypeError:  # Pillow < 10.1
        font = ImageFont.load_default()
    d.rectangle([0, pad + 256 + 40, pw, pad + 256 + 40 + strip_h], fill=(0x1F, 0x1E, 0x1D, 255))
    d.rectangle([0, pad + 256 + 40 + strip_h, pw, ph], fill=(0xF3, 0xF3, 0xF3, 255))
    for i, im in enumerate(imgs):
        x = pad + i * cell
        out.alpha_composite(im.resize((256, 256), Image.LANCZOS), (x, pad))
        # number label
        d.text((x + 128, pad + 256 + 20), str(i + 1), fill=(255, 255, 255, 255),
               font=font, anchor="mm")
        for row in range(2):
            y0 = pad + 256 + 40 + row * strip_h + 16
            sx = x + 40
            for z in small:
                ico = im.resize((z, z), Image.LANCZOS)
                out.alpha_composite(ico, (sx, y0 + (48 - z) // 2))
                sx += z + 30
    return out


def main():
    imgs = [option1(), option2(), option3(), option4()]
    for n, im in enumerate(imgs, 1):
        save(im, n)
    preview(imgs).save(HERE / "preview.png")
    # zoomed small-size check (nearest-neighbour x6) for inspection
    z = Image.new("RGBA", (4 * (16 + 32 + 4) * 6, 32 * 6 * 2), (0x1F, 0x1E, 0x1D, 255))
    ImageDraw.Draw(z).rectangle([0, 32 * 6, z.width, z.height], fill=(0xF3, 0xF3, 0xF3, 255))
    for i, im in enumerate(imgs):
        for row in range(2):
            x = i * (16 + 32 + 4) * 6
            for sz in (32, 16):
                t = im.resize((sz, sz), Image.LANCZOS).resize((sz * 6, sz * 6), Image.NEAREST)
                z.alpha_composite(t, (x, row * 32 * 6))
                x += (sz + 2) * 6
    if len(sys.argv) > 1:  # optional: path for a pixel-zoomed 16/32 inspection sheet
        z.save(sys.argv[1])
    print("done")


if __name__ == "__main__":
    main()

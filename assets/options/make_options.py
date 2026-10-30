"""Generate 4 alternative icon options for Local AI Chat.

Each option is drawn at 1024px with 2x supersampling, then downscaled with
LANCZOS. Writes optionN.png (256px), optionN.ico (16..256) and preview.png.

Usage: python assets/options/make_options.py   (requires Pillow)
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
SIZE = 1024
SS = 2
W = SIZE * SS
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


def s(v):
    return int(round(v * SS))


def box(x0, y0, x1, y1):
    return [s(x0), s(y0), s(x1), s(y1)]


# ---------------------------------------------------------------- helpers
def linear(c0, c1, angle_deg=45):
    """Full-canvas linear gradient from c0 to c1 along angle (0 = left->right)."""
    a = math.radians(angle_deg)
    dx, dy = math.cos(a), math.sin(a)
    small = 256
    g = Image.new("RGBA", (small, small))
    px = g.load()
    for y in range(small):
        for x in range(small):
            t = ((x / (small - 1) - 0.5) * dx + (y / (small - 1) - 0.5) * dy) / (
                abs(dx) * 0.5 + abs(dy) * 0.5) * 0.5 + 0.5
            t = min(1, max(0, t))
            px[x, y] = tuple(int(c0[i] + (c1[i] - c0[i]) * t) for i in range(3)) + (255,)
    return g.resize((W, W), Image.BICUBIC)


def radial(c_in, c_out, cx=0.5, cy=0.4, r=0.8):
    small = 256
    g = Image.new("RGBA", (small, small))
    px = g.load()
    for y in range(small):
        for x in range(small):
            t = math.hypot(x / small - cx, y / small - cy) / r
            t = min(1, max(0, t))
            px[x, y] = tuple(int(c_in[i] + (c_out[i] - c_in[i]) * t) for i in range(3)) + (255,)
    return g.resize((W, W), Image.BICUBIC)


def new_mask():
    m = Image.new("L", (W, W), 0)
    return m, ImageDraw.Draw(m)


def fill(img, mask, paint):
    """Composite `paint` (colour tuple or RGBA image) onto img through mask."""
    if isinstance(paint, tuple):
        paint = Image.new("RGBA", (W, W), paint)
    layer = paint.copy()
    layer.putalpha(Image.composite(paint.getchannel("A"), Image.new("L", (W, W), 0), mask))
    img.alpha_composite(layer)


def shadow(img, mask, blur=24, offset=(0, 14), alpha=110):
    sh = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    m = mask.point(lambda v: v * alpha // 255).filter(ImageFilter.GaussianBlur(s(blur)))
    sh.putalpha(m)
    sh = sh.transform(sh.size, Image.AFFINE, (1, 0, -s(offset[0]), 0, 1, -s(offset[1])))
    img.alpha_composite(sh)


def glow(img, mask, color, blur=40, strength=1.0):
    m = mask.filter(ImageFilter.GaussianBlur(s(blur))).point(lambda v: min(255, int(v * strength)))
    fill(img, m, color)


def tile_mask(shape="rounded"):
    m, d = new_mask()
    if shape == "circle":
        d.ellipse(box(32, 32, 992, 992), fill=255)
    else:
        d.rounded_rectangle(box(32, 32, 992, 992), radius=s(224), fill=255)
    return m


def top_sheen(img, base_mask, strength=34):
    """Subtle light from the top for depth."""
    g = linear((255, 255, 255), (0, 0, 0), 90)
    a = g.getchannel("R").point(lambda v: max(0, v - 128) * strength // 128)
    a = Image.composite(a, Image.new("L", (W, W), 0), base_mask)
    layer = Image.new("RGBA", (W, W), (255, 255, 255, 0))
    layer.putalpha(a)
    img.alpha_composite(layer)


def finish(img):
    return img.resize((SIZE, SIZE), Image.LANCZOS)


# ---------------------------------------------------------------- option 1
def option1():
    """Neural ring: cyan/teal node network on deep navy."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    base = tile_mask()
    fill(img, base, radial((22, 44, 84), (8, 14, 34), 0.5, 0.35, 0.85))
    top_sheen(img, base, 20)
    grad = linear((90, 240, 255), (20, 190, 160), 60)

    cx, cy, R = 512, 512, 318
    nodes = [(cx + R * math.cos(math.radians(-90 + 60 * i)),
              cy + R * math.sin(math.radians(-90 + 60 * i))) for i in range(6)]
    m, d = new_mask()
    lw = s(78)
    # outer ring segments + spokes to the core
    for i in range(6):
        a, b = nodes[i], nodes[(i + 1) % 6]
        d.line([(s(a[0]), s(a[1])), (s(b[0]), s(b[1]))], fill=255, width=lw)
        if i % 2 == 0:
            d.line([(s(a[0]), s(a[1])), (s(cx), s(cy))], fill=255, width=lw)
    for (x, y) in nodes:
        d.ellipse(box(x - 100, y - 100, x + 100, y + 100), fill=255)
    d.ellipse(box(cx - 150, cy - 150, cx + 150, cy + 150), fill=255)
    glow(img, m, (40, 220, 230, 255), blur=46, strength=0.9)
    fill(img, m, grad)
    # dark pupils in outer nodes and a bright core
    m2, d2 = new_mask()
    for (x, y) in nodes:
        d2.ellipse(box(x - 40, y - 40, x + 40, y + 40), fill=255)
    fill(img, m2, (10, 22, 48, 255))
    m3, d3 = new_mask()
    d3.ellipse(box(cx - 72, cy - 72, cx + 72, cy + 72), fill=255)
    fill(img, m3, (235, 255, 255, 255))
    return finish(img)


# ---------------------------------------------------------------- option 2
def option2():
    """Friendly robot head with a speech bubble on a purple->blue circle."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    base = tile_mask("circle")
    fill(img, base, linear((150, 70, 240), (40, 110, 250), 60))
    top_sheen(img, base, 40)
    white = (255, 255, 255, 255)
    dark = (44, 30, 110, 255)

    head, d = new_mask()
    d.rounded_rectangle(box(170, 390, 770, 830), radius=s(170), fill=255)
    # antenna
    d.rounded_rectangle(box(444, 270, 496, 400), radius=s(26), fill=255)
    d.ellipse(box(400, 200, 540, 340), fill=255)
    shadow(img, head, 26, (0, 18), 120)
    fill(img, head, white)
    # dark visor with two glowing eyes
    visor, d = new_mask()
    d.rounded_rectangle(box(250, 500, 690, 720), radius=s(110), fill=255)
    fill(img, visor, dark)
    eyes, d = new_mask()
    d.rounded_rectangle(box(330, 555, 420, 665), radius=s(45), fill=255)
    d.rounded_rectangle(box(520, 555, 610, 665), radius=s(45), fill=255)
    glow(img, eyes, (90, 230, 255, 255), blur=22, strength=1.2)
    fill(img, eyes, (120, 240, 255, 255))
    # speech bubble top-right
    bub, d = new_mask()
    d.ellipse(box(650, 150, 900, 380), fill=255)
    d.polygon([(s(680), s(330)), (s(640), s(420)), (s(750), s(370))], fill=255)
    shadow(img, bub, 20, (0, 12), 110)
    fill(img, bub, (255, 214, 90, 255))
    dots, d = new_mask()
    for x in (712, 775, 838):
        d.ellipse(box(x - 24, 265 - 24, x + 24, 265 + 24), fill=255)
    fill(img, dots, dark)
    return finish(img)


# ---------------------------------------------------------------- option 3
def star8(cx, cy, r):
    pts = []
    for i in range(16):
        rr = r if i % 2 == 0 else r * 0.76  # khatam: two overlapping squares
        a = math.radians(-90 + 22.5 * i)
        pts.append((s(cx + rr * math.cos(a)), s(cy + rr * math.sin(a))))
    return pts


def option3():
    """Khatam eight-point star, gold on emerald, glowing circuit core."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    base = tile_mask()
    fill(img, base, radial((14, 84, 64), (4, 30, 26), 0.5, 0.45, 0.8))
    top_sheen(img, base, 18)
    gold = linear((255, 226, 130), (214, 150, 40), 70)

    cx, cy = 512, 512
    star, d = new_mask()
    d.polygon(star8(cx, cy, 430), fill=255)
    glow(img, star, (255, 200, 80, 255), blur=50, strength=0.55)
    shadow(img, star, 24, (0, 16), 140)
    fill(img, star, gold)
    # inner emerald star (outline effect, kept bold)
    inner, d = new_mask()
    d.polygon(star8(cx, cy, 280), fill=255)
    fill(img, inner, radial((22, 120, 90), (8, 52, 42), 0.5, 0.5, 0.25))
    # circuit traces from core to the 8 inner points, ending in pads
    circ, d = new_mask()
    for i in range(8):
        a = math.radians(-90 + 45 * i)
        x1, y1 = cx + 190 * math.cos(a), cy + 190 * math.sin(a)
        d.line([(s(cx), s(cy)), (s(x1), s(y1))], fill=255, width=s(40))
        d.ellipse(box(x1 - 36, y1 - 36, x1 + 36, y1 + 36), fill=255)
    fill(img, circ, (110, 240, 180, 255))
    # glowing core
    core, d = new_mask()
    d.ellipse(box(cx - 120, cy - 120, cx + 120, cy + 120), fill=255)
    glow(img, core, (120, 255, 200, 255), blur=50, strength=1.4)
    fill(img, core, radial((255, 255, 230), (255, 214, 100), 0.5, 0.5, 0.06))
    return finish(img)


# ---------------------------------------------------------------- option 4
def option4():
    """Processor chip with chat-bubble cutout, orange->pink gradient tile."""
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    base = tile_mask()
    grad = linear((255, 140, 40), (236, 50, 130), 50)
    fill(img, base, grad)
    top_sheen(img, base, 40)

    chip, d = new_mask()
    d.rounded_rectangle(box(232, 232, 792, 792), radius=s(100), fill=255)
    # pins: 3 per side
    for p in (400, 624):
        d.rounded_rectangle(box(p - 44, 140, p + 44, 260), radius=s(26), fill=255)
        d.rounded_rectangle(box(p - 44, 764, p + 44, 884), radius=s(26), fill=255)
        d.rounded_rectangle(box(140, p - 44, 260, p + 44), radius=s(26), fill=255)
        d.rounded_rectangle(box(764, p - 44, 884, p + 44), radius=s(26), fill=255)
    # chat bubble cutout
    cut, dc = new_mask()
    dc.rounded_rectangle(box(322, 330, 702, 610), radius=s(110), fill=255)
    dc.polygon([(s(380), s(560)), (s(350), s(712)), (s(510), s(600))], fill=255)
    chip = Image.composite(Image.new("L", (W, W), 0), chip, cut)
    shadow(img, chip, 26, (0, 18), 120)
    fill(img, chip, (255, 255, 255, 255))
    # three dots inside the bubble (white)
    dots, d = new_mask()
    for x in (418, 512, 606):
        d.ellipse(box(x - 36, 470 - 36, x + 36, 470 + 36), fill=255)
    fill(img, dots, (255, 255, 255, 255))
    return finish(img)


# ---------------------------------------------------------------- preview
def preview(icons):
    col, pad = 300, 30
    sheet_w = col * len(icons) + pad
    h = 30 + 256 + 20 + 80 + 80 + 60
    sheet = Image.new("RGBA", (sheet_w, h), (245, 245, 245, 255))
    d = ImageDraw.Draw(sheet)
    d.rectangle([0, 0, sheet_w, 30 + 256 + 20], fill=(60, 60, 64, 255))
    y_dark, y_light = 306, 386
    d.rectangle([0, y_dark, sheet_w, y_dark + 80], fill=(32, 32, 32, 255))
    d.rectangle([0, y_light, sheet_w, y_light + 80], fill=(250, 250, 250, 255))
    try:
        font = ImageFont.truetype("C:/Windows/Fonts/segoeuib.ttf", 34)
    except OSError:
        font = ImageFont.load_default()
    for i, ic in enumerate(icons):
        x = pad + i * col
        sheet.alpha_composite(ic.resize((256, 256), Image.LANCZOS), (x, 30))
        for y in (y_dark, y_light):
            xx = x + 20
            for n in (48, 32, 16):
                sheet.alpha_composite(ic.resize((n, n), Image.LANCZOS), (xx, y + 40 - n // 2))
                xx += n + 40
        d.text((x + 128, h - 32), str(i + 1), fill=(30, 30, 30), font=font, anchor="mm")
    sheet.save(HERE / "preview.png")


def main():
    icons = [option1(), option2(), option3(), option4()]
    for n, ic in enumerate(icons, 1):
        ic.resize((256, 256), Image.LANCZOS).save(HERE / f"option{n}.png")
        ic.save(HERE / f"option{n}.ico", format="ICO", sizes=[(k, k) for k in ICO_SIZES])
    preview(icons)
    print("wrote options 1-4 and preview.png to", HERE)


if __name__ == "__main__":
    main()

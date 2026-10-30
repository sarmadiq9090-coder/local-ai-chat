# NOTE: the current app icon is option 4 from options/make_options.py (chip + chat bubble).
# This script generates the previous (orange bubble) icon and is kept for reference.
"""Generate the Local AI Chat application icon.

Draws an orange chat bubble with a four-point spark on a dark rounded tile at
1024px (antialiased via supersampling), then downsamples to every ICO size.

Usage:  python assets/make_icon.py      (requires Pillow: pip install pillow)
Writes: assets/icon.ico (16..256) and assets/icon.png (256px preview).
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
BG = (0x1F, 0x1E, 0x1D, 255)      # dark tile
ORANGE = (0xD9, 0x77, 0x57, 255)  # bubble
SIZE = 1024
SS = 4                            # supersampling factor for antialiasing
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


def s(v):
    return int(round(v * SS))


def astroid(cx, cy, r, steps=360):
    """Four-point star with concave sides (astroid curve)."""
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, sn = math.cos(t), math.sin(t)
        pts.append((s(cx + r * math.copysign(abs(c) ** 3, c)),
                    s(cy + r * math.copysign(abs(sn) ** 3, sn))))
    return pts


def draw_master():
    W = SIZE * SS
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Dark rounded-square tile
    d.rounded_rectangle([s(24), s(24), s(1000), s(1000)], radius=s(220), fill=BG)

    # Chat bubble body + tail (bottom-left)
    d.rounded_rectangle([s(176), s(208), s(848), s(720)], radius=s(160), fill=ORANGE)
    d.polygon([(s(290), s(660)), (s(232), s(856)), (s(470), s(700))], fill=ORANGE)

    # Spark: big four-point star plus a small companion, cut out in tile colour
    d.polygon(astroid(500, 470, 190), fill=BG)
    d.polygon(astroid(680, 330, 70), fill=BG)
    return img.resize((SIZE, SIZE), Image.LANCZOS)


def main():
    master = draw_master()
    frames = [master.resize((n, n), Image.LANCZOS) for n in ICO_SIZES]
    frames[-1].save(HERE / "icon.png")
    master.save(HERE / "icon.ico", format="ICO",
                sizes=[(n, n) for n in ICO_SIZES])
    print("wrote", HERE / "icon.ico", "and", HERE / "icon.png")


if __name__ == "__main__":
    main()

"""Render the poses to 1-bit 128x46 sheets: python3 bits.py out.png"""
import sys
import numpy as np
from PIL import Image, ImageDraw
from face_render import Face, render
from poses import FRAME, POSES, TONGUE
from stylize import reduce, stylize, W, H, S
if __name__ == "__main__":
    f = Face()
    tiles = []
    for n in POSES:
        img, zb, pid = render(f, POSES[n], W=W * S, H=H * S, yaw=38, pitch=4, frame=FRAME, tongue=TONGUE.get(n, (0.5, 0.5)))
        cls, shade = reduce(pid, img)
        b = stylize(cls, shade)
        np.save(f"bits_{n}.npy", b)
        tiles.append((n, Image.fromarray(b * 235).resize((W * 4, H * 4), Image.NEAREST)))
    sh = Image.new("L", (2 * (W * 4 + 10), 3 * (H * 4 + 18)), 255)
    d = ImageDraw.Draw(sh)
    for i, (n, t) in enumerate(tiles):
        x = (i % 2) * (W * 4 + 10); y = (i // 2) * (H * 4 + 18)
        sh.paste(t, (x, y + 14)); d.text((x + 3, y + 1), n, fill=0)
    sh.save(sys.argv[1])

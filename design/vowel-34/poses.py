"""Render the vowel poses (grayscale) into a sheet: python3 poses.py out.png"""
import sys
import numpy as np
from PIL import Image, ImageDraw
from face_render import Face, render
FRAME = [-0.42, 0.78, -0.77, -0.338]
POSES = {
    "closed": {},
    "Ah": {"jawOpen": 0.55, "mouthLowerDown_L": 0.3, "mouthLowerDown_R": 0.3},
    "Ee": {"jawOpen": 0.12, "mouthStretch_L": 0.5, "mouthStretch_R": 0.5, "mouthSmile_L": 0.4, "mouthSmile_R": 0.4, "mouthUpperUp_L": 0.2, "mouthUpperUp_R": 0.2},
    "Eh": {"jawOpen": 0.3, "mouthStretch_L": 0.3, "mouthStretch_R": 0.3, "mouthLowerDown_L": 0.2, "mouthLowerDown_R": 0.2},
    "Oo": {"jawOpen": 0.08, "mouthPucker": 0.9, "mouthFunnel": 0.4},
    "Oh": {"jawOpen": 0.3, "mouthFunnel": 0.8, "mouthPucker": 0.3},
}
TONGUE = {"Ah": (0.27, 0.2), "Ee": (0.71, 0.67), "Eh": (0.63, 0.4), "Oo": (0.37, 0.6), "Oh": (0.14, 0.41)}
if __name__ == "__main__":
    f = Face()
    tiles = []
    for n, w in POSES.items():
        img, _, _ = render(f, w, W=512, H=184, yaw=38, pitch=4, frame=FRAME, tongue=TONGUE.get(n, (0.5, 0.5)))
        tiles.append((n, Image.fromarray((img * 255).astype("uint8"))))
    sh = Image.new("L", (2 * 520, 3 * 200), 255)
    d = ImageDraw.Draw(sh)
    for i, (n, t) in enumerate(tiles):
        x = (i % 2) * 520; y = (i // 2) * 200
        sh.paste(t, (x, y + 14)); d.text((x + 3, y + 1), n, fill=0)
    sh.save(sys.argv[1])

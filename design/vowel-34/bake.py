"""Render the 3/4 mouth poses to 1-bit 128x46 frames in the chosen style (tone C) and write
them as a JS module for the mockup: python3 bake.py out.js sheet.png"""
import sys, json
import numpy as np
from PIL import Image, ImageDraw
from face_render import Face, render
import face_render as fm
from poses import POSES, TONGUE
from stylize import reduce, BAYER4, W, H, S

fm.LIP_A, fm.LIP_UP, fm.LIP_LO = 0.068, 0.027, 0.032
PITCH, YAW = -4, 38


def frame_for(f):
    """centred on where the lips go across all the poses (closed to the jaw wide open)"""
    p, y = np.radians(PITCH), np.radians(YAW)
    Ry = np.array([[np.cos(y), 0, np.sin(y)], [0, 1, 0], [-np.sin(y), 0, np.cos(y)]])
    Rp = np.array([[1, 0, 0], [0, np.cos(p), -np.sin(p)], [0, np.sin(p), np.cos(p)]])
    lip = f.lipness < 1.0
    lo, hi = np.array([9.0, 9.0]), np.array([-9.0, -9.0])
    for w in list(POSES.values()):
        V, _ = f.verts("head", w)
        Vr = (V @ (Rp @ Ry).T)[lip]
        lo = np.minimum(lo, Vr[:, :2].min(0)); hi = np.maximum(hi, Vr[:, :2].max(0))
    cx, cy = (lo[0] + hi[0]) / 2 + 0.03, (lo[1] + hi[1]) / 2
    h = (hi[1] - lo[1]) + 0.03
    w = h * W / H
    return [cx - w / 2, cx + w / 2, cy - h / 2, cy + h / 2]


def tone(cls, shade):
    t = np.zeros((H, W))
    sk = cls == 0
    t[sk] = np.clip((shade[sk] - 0.86) * 3.0, 0, 1)
    lp = cls == 7
    t[lp] = 0.4 + 0.6 * shade[lp]
    tg = cls == 6
    t[tg] = 0.3 + 0.3 * shade[tg]
    t[(cls == 1) | (cls == 5)] = 1.0
    th = BAYER4[np.arange(H)[:, None] % 4, np.arange(W)[None, :] % 4]
    out = (t > th).astype(np.uint8)
    face = cls >= 0
    bg = np.pad(~face, 1, constant_values=False)
    edge = face & (bg[:-2, 1:-1] | bg[2:, 1:-1] | bg[1:-1, :-2] | bg[1:-1, 2:])
    out[edge & (cls != 7)] = 1   # the lips stand in front of the face's edge
    return out


if __name__ == "__main__":
    f = Face()
    FR = frame_for(f)
    frames = {}
    for n in POSES:
        img, zb, pid = render(f, POSES[n], W=W * S, H=H * S, yaw=YAW, pitch=PITCH, frame=FR, tongue=TONGUE.get(n, (0.5, 0.5)))
        cls, shade = reduce(pid, img)
        frames[n] = tone(cls, shade)
    rows = {n: ["".join("1" if v else "0" for v in r) for r in b] for n, b in frames.items()}
    open(sys.argv[1], "w").write("const FACE34_FRAMES = " + json.dumps(rows) + ";\n")
    sh = Image.new("L", (2 * (W * 4 + 10), 3 * (H * 4 + 18)), 255)
    d = ImageDraw.Draw(sh)
    for i, (n, b) in enumerate(frames.items()):
        x = (i % 2) * (W * 4 + 10); y = (i // 2) * (H * 4 + 18)
        sh.paste(Image.fromarray(b * 235).resize((W * 4, H * 4), Image.NEAREST), (x, y + 14)); d.text((x + 3, y + 1), n, fill=0)
    sh.save(sys.argv[2])

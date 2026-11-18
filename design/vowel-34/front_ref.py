"""Front-view references (reference only): the head face-on in the six vowel poses, framed so
the closed mouth spans the front view's width, with measurements per pose.
    python3 front_ref.py   -> front_ref.json, front-ref-grey.png, front-ref-classes.png
"""
import json
import numpy as np
from PIL import Image, ImageDraw, ImageFont
from face_render import Face, render
import face_render as fm
from poses import POSES, TONGUE
from stylize import W, H, S

fm.LIP_A, fm.LIP_UP, fm.LIP_LO = 0.068, 0.027, 0.032
YAW, PITCH = 0.0, -4.0

f = Face()
# frame: closed mouth corner to corner = 80 px (the front view's width), centred like it
lip = f.lipness < 1.0
V, _ = f.verts("head", {})
p = np.radians(PITCH)
Rp = np.array([[1, 0, 0], [0, np.cos(p), -np.sin(p)], [0, np.sin(p), np.cos(p)]])
Vr = V @ Rp.T
lx0, lx1 = Vr[lip, 0].min(), Vr[lip, 0].max()
ly = (Vr[lip, 1].min() + Vr[lip, 1].max()) / 2
scale = 60.0 / (lx1 - lx0)                 # px per unit
w_u, h_u = W / scale, H / scale
cy_px = 16                                  # the mouth line
FR = [(lx0 + lx1) / 2 - w_u / 2, (lx0 + lx1) / 2 + w_u / 2, ly - (H - cy_px) / scale, ly + cy_px / scale]

meas, greys, cls_ims = {}, [], []
for n in ("Ah", "Ee", "Eh", "Oo", "Oh", "closed"):
    img, zb, pid = render(f, POSES[n], W=W * S, H=H * S, yaw=YAW, pitch=PITCH, frame=FR, tongue=TONGUE.get(n, (0.5, 0.5)))
    from scipy.ndimage import binary_fill_holes
    lips = pid == 7
    filled = binary_fill_holes(lips)
    hole = filled & ~lips
    teeth = (pid == 1) | (pid == 5)
    ys, xs = np.nonzero(lips)
    cxp = int(round(xs.mean()))
    col = lips[:, cxp]
    rows = np.nonzero(col)[0]
    hrows = np.nonzero(hole[:, cxp])[0]
    m = {
        "width": (xs.max() - xs.min() + 1) / S,
        "upper_lip_top": rows.min() / S, "lower_lip_bottom": (rows.max() + 1) / S,
        "opening_top": (hrows.min() / S) if len(hrows) else None,
        "opening_bottom": ((hrows.max() + 1) / S) if len(hrows) else None,
        "opening_width": ((np.ptp(np.nonzero(hole.any(0))[0]) + 1) / S) if hole.any() else 0,
        "teeth_upper_rows": (np.nonzero(teeth[:, cxp] & (np.arange(H * S) < (hrows.mean() if len(hrows) else 0)))[0].size / S) if len(hrows) else 0,
        "teeth_lower_rows": (np.nonzero(teeth[:, cxp] & (np.arange(H * S) >= (hrows.mean() if len(hrows) else 1e9)))[0].size / S) if len(hrows) else 0,
    }
    meas[n] = m
    greys.append((n, Image.fromarray((img * 255).astype("uint8"))))
    c = np.zeros(pid.shape + (3,), np.uint8)
    for k, v in {-1: (0, 0, 0), 0: (150, 130, 115), 1: (80, 140, 255), 4: (20, 20, 30), 5: (80, 230, 80), 6: (230, 80, 200), 7: (220, 70, 70)}.items():
        c[pid == k] = v
    cls_ims.append((n, Image.fromarray(c)))
json.dump(meas, open("front_ref.json", "w"), indent=1)
fnt = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 16)
for name, ims in (("front-ref-grey.png", greys), ("front-ref-classes.png", cls_ims)):
    sh = Image.new("RGB", (2 * 526, 3 * 206), (245, 245, 242))
    d = ImageDraw.Draw(sh)
    for i, (n, t) in enumerate(ims):
        x = (i % 2) * 526; y = (i // 2) * 206
        sh.paste(t.convert("RGB"), (x, y + 20)); d.text((x + 3, y + 1), n, fill=(30, 30, 30), font=fnt)
    sh.save(name)
for n, m in meas.items():
    print(n, {k: (round(v, 1) if isinstance(v, float) else v) for k, v in m.items()})

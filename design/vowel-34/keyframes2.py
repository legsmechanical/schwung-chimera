"""Reference keyframes, v2 (reference only: nothing here ships).

For each pose of the 3D head, measures in screen pixels:
    outer   the lips' outline: 64 points by rays from its centre (round puckers stay round)
    hole_u  the opening's upper edge, 17 points between the opening's own corners
    hole_l  its lower edge; when closed both lie on the seam (the darkest line between the lips)
    edge    the face's outer edge (profile, jaw), x per row, -1 where none
Grid: jaw 0 / 0.5 / 1 x lips spread / neutral / round, plus pout. Writes keyframes2.min.json.
"""
import json
import numpy as np
from face_render import Face, render
import face_render as fm
from bake import frame_for, PITCH, YAW
from keyframes import pose
from stylize import W, H, S

fm.LIP_A, fm.LIP_UP, fm.LIP_LO = 0.068, 0.027, 0.032
NR, NE = 64, 17


def measure(pid, img):
    from scipy.ndimage import binary_fill_holes
    lips = pid == 7
    # the opening is everything the lips enclose: the cavity, the teeth, and the lips' own
    # inner surface (which renders as skin, and in 1-bit is simply dark)
    filled = binary_fill_holes(lips)
    hole_in = filled & ~lips
    ys, xs = np.nonzero(lips)
    bx0, bx1, by0, by1 = xs.min(), xs.max(), ys.min(), ys.max()
    region = filled
    ry, rx = np.nonzero(region)
    cx, cy = rx.mean(), ry.mean()
    outer = []
    for k in range(NR):
        a = 2 * np.pi * k / NR
        dx, dy = np.cos(a), np.sin(a)
        r, last = 0.0, 0.0
        while True:
            x, y = int(round(cx + dx * r)), int(round(cy + dy * r))
            if x < 0 or y < 0 or x >= region.shape[1] or y >= region.shape[0]:
                break
            if region[y, x]:
                last = r
            elif r > last + 6:
                break
            r += 0.5
        outer.append([(cx + dx * (last + 0.5)) / S, (cy + dy * (last + 0.5)) / S])
    hu, hl = [], []
    if hole_in.sum() > 4 * S:
        hy, hx = np.nonzero(hole_in)
        h0, h1 = hx.min(), hx.max()
        for i in range(NE):
            x = int(round(h0 + (h1 - h0) * i / (NE - 1)))
            c = np.nonzero(hole_in[:, x])[0]
            d = 1
            while len(c) == 0 and d < (h1 - h0):
                for xx in (x - d, x + d):
                    if h0 <= xx <= h1 and len(c) == 0:
                        c = np.nonzero(hole_in[:, xx])[0]
                d += 1
            hu.append([x / S, (c.min() - 0.5) / S])
            hl.append([x / S, (c.max() + 0.5) / S])
        # the corners of the opening: the two ends meet
        hu[0][1] = hl[0][1] = (hu[0][1] + hl[0][1]) / 2
        hu[-1][1] = hl[-1][1] = (hu[-1][1] + hl[-1][1]) / 2
    else:
        # closed: the seam, the darkest row in the lips per column, corner to corner
        x0, x1 = bx0, bx1
        for i in range(NE):
            x = int(round(x0 + (x1 - x0) * i / (NE - 1)))
            c = np.nonzero(lips[:, x])[0]
            if len(c) == 0:
                c = np.nonzero(lips[:, min(x1, x + 1)])[0]
            sh = img[c, x]
            yy = c[np.argmin(sh)] if len(c) > 2 else c.mean()
            hu.append([x / S, yy / S]); hl.append([x / S, yy / S])
    face = pid >= 0
    edge = []
    for y in range(0, H * S, S):
        xs_ = np.nonzero(face[y])[0]
        edge.append((xs_.max() + 1) / S if len(xs_) and xs_.max() < W * S - 1 else -1)
    return {"outer": outer, "hole_u": hu, "hole_l": hl, "edge": edge}


if __name__ == "__main__":
    f = Face()
    FR = frame_for(f)
    keys = {}
    for j in (0, 0.5, 1):
        for w in (-1, 0, 1):
            keys[f"j{j}_w{w}"] = pose(j, w)
    keys["pout"] = pose(0, 0, pout=1.0)
    out = {}
    for name, weights in keys.items():
        img, zb, pid = render(f, weights, W=W * S, H=H * S, yaw=YAW, pitch=PITCH, frame=FR, tongue=None)
        np.save(f"ref2_pid_{name}.npy", pid)
        m = measure(pid, img)
        q = lambda pts: [round(c * 4) / 4 for p in pts for c in p]
        out[name] = {"outer": q(m["outer"]), "hole_u": q(m["hole_u"]), "hole_l": q(m["hole_l"]), "edge": [round(e * 4) / 4 for e in m["edge"]]}
        print(name, "hole corners", m["hole_u"][0], m["hole_u"][-1])
    open("keyframes2.min.json", "w").write(json.dumps(out, separators=(",", ":")))

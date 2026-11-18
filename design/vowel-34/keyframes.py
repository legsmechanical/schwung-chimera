"""Reference keyframes from the 3D head (reference only: nothing here ships).

Renders a grid of poses (jaw closed/open x lips spread/neutral/round, plus pout) at 4x the
screen, and extracts the contours the drawing is made from, as points running from the near
mouth corner (t=0) to the far corner (t=1), in screen pixels:
    upper_out, upper_in, lower_in, lower_out   (lip edges)
    face_edge                                   (the profile / jaw line, top to bottom)
Writes keyframes.json.
"""
import json
import numpy as np
from face_render import Face, render
import face_render as fm
from bake import frame_for, PITCH, YAW
from stylize import W, H, S

fm.LIP_A, fm.LIP_UP, fm.LIP_LO = 0.068, 0.027, 0.032
N = 17  # points per lip edge

# the grid: jaw x width, and pout. Weights in the head's own shape vocabulary.
def pose(jaw, width, pout=0.0):
    w = {"jawOpen": 0.62 * jaw, "mouthLowerDown_L": 0.3 * jaw, "mouthLowerDown_R": 0.3 * jaw}
    if width < 0:      # spread (Ee)
        s = -width
        w.update({"mouthStretch_L": 0.5 * s, "mouthStretch_R": 0.5 * s, "mouthSmile_L": 0.35 * s, "mouthSmile_R": 0.35 * s,
                  "mouthUpperUp_L": 0.15 * s, "mouthUpperUp_R": 0.15 * s})
    elif width > 0:    # rounded (Oo)
        w.update({"mouthPucker": 0.9 * width, "mouthFunnel": 0.45 * width})
    if pout:
        w.update({"mouthFunnel": w.get("mouthFunnel", 0) + 0.5 * pout, "mouthPucker": w.get("mouthPucker", 0) + 0.3 * pout})
    return w

GRID = {f"j{j}_w{w}": (j, w) for j in (0, 1) for w in (-1, 0, 1)}


def contours(pid):
    """lip edges per column between the corners, from the 4x class map"""
    lips = pid == 7
    inner = (pid == 4) | (pid == 1) | (pid == 5) | (pid == 6)  # anything seen through the opening
    ys, xs = np.nonzero(lips)
    x0, x1 = xs.min(), xs.max()
    # corner heights: the lips' mid height at their extreme columns
    def col(x):
        c = np.nonzero(lips[:, x])[0]
        return c
    out = {k: [] for k in ("upper_out", "upper_in", "lower_in", "lower_out")}
    for i in range(N):
        t = i / (N - 1)
        x = int(round(x0 + (x1 - x0) * t))
        x = min(max(x, x0), x1)
        c = col(x)
        if len(c) == 0:
            c = col(min(x1, x + 1))
        top, bot = c.min(), c.max()
        # the opening in this column: rows between the lips that are not lip
        rows = np.arange(top, bot + 1)
        gap = rows[~lips[top:bot + 1, x]]
        if len(gap):
            ui, li = gap.min() - 0.5, gap.max() + 0.5
        else:
            # closed: the seam is where the upper and lower lip meet (the darkest row)
            mid = (top + bot) / 2
            ui = li = mid
        out["upper_out"].append([x / S, top / S])
        out["upper_in"].append([x / S, ui / S])
        out["lower_in"].append([x / S, li / S])
        out["lower_out"].append([x / S, (bot + 1) / S])
    return out


def face_edge(pid):
    """the face's outer edge: for each row, the rightmost face pixel (the profile), and the
    bottom-left jaw where the face meets the background below"""
    face = pid >= 0
    pts = []
    for y in range(0, H * S, S):
        xs = np.nonzero(face[y])[0]
        if len(xs) and xs.max() < W * S - 1:
            pts.append([(xs.max() + 1) / S, y / S])
    return pts


if __name__ == "__main__":
    f = Face()
    FR = frame_for(f)
    out = {"frame": FR, "keys": {}}
    for name, (j, w) in list(GRID.items()) + [("pout", (0, 0))]:
        weights = pose(j, w, pout=1.0 if name == "pout" else 0.0)
        img, zb, pid = render(f, weights, W=W * S, H=H * S, yaw=YAW, pitch=PITCH, frame=FR, tongue=None)
        k = contours(pid)
        k["face_edge"] = face_edge(pid)
        out["keys"][name] = k
        np.save(f"ref_pid_{name}.npy", pid)
        print(name, "corners", k["upper_out"][0], k["upper_out"][-1])
    json.dump(out, open("keyframes.json", "w"), indent=0)

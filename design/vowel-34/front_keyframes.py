"""Front-view keyframes (reference only), measured like keyframes2 but face-on, framed so the
tallest shape fits: python3 front_keyframes.py -> keyframes_front.min.json"""
import json
import numpy as np
from face_render import Face, render
import face_render as fm
from keyframes import pose
from keyframes2 import measure
from stylize import W, H, S

fm.LIP_A, fm.LIP_UP, fm.LIP_LO = 0.068, 0.027, 0.032
YAW, PITCH = 0.0, -4.0
CLOSED_W, MOUTH_Y = 46.0, 15.0
JAW_MAX = 0.8   # the widest vowel (Ah) opens to ~80% of the model's jaw

if __name__ == "__main__":
    f = Face()
    lip = f.lipness < 1.0
    V, _ = f.verts("head", {})
    p = np.radians(PITCH)
    Rp = np.array([[1, 0, 0], [0, np.cos(p), -np.sin(p)], [0, np.sin(p), np.cos(p)]])
    Vr = V @ Rp.T
    lx0, lx1 = Vr[lip, 0].min(), Vr[lip, 0].max()
    ly = (Vr[lip, 1].min() + Vr[lip, 1].max()) / 2
    scale = CLOSED_W / (lx1 - lx0)
    FR = [(lx0 + lx1) / 2 - W / scale / 2, (lx0 + lx1) / 2 + W / scale / 2, ly - (H - MOUTH_Y) / scale, ly + MOUTH_Y / scale]
    keys = {}
    for j in (0, 0.5, 1):
        for w in (-1, 0, 1):
            keys[f"j{j}_w{w}"] = pose(j * JAW_MAX, w)
    keys["pout"] = pose(0, 0, pout=1.0)
    out = {}
    for name, weights in keys.items():
        img, zb, pid = render(f, weights, W=W * S, H=H * S, yaw=YAW, pitch=PITCH, frame=FR, tongue=None)
        np.save(f"fref_pid_{name}.npy", pid)
        m = measure(pid, img)
        q = lambda pts: [round(c * 4) / 4 for p in pts for c in p]
        out[name] = {"outer": q(m["outer"]), "hole_u": q(m["hole_u"]), "hole_l": q(m["hole_l"]), "edge": [-1] * H}
        ys = [m["outer"][i][1] for i in range(len(m["outer"]))]
        print(name, "lips y", round(min(ys), 1), round(max(ys), 1), "hole", m["hole_u"][0], m["hole_u"][-1])
    open("keyframes_front.min.json", "w").write(json.dumps(out, separators=(",", ":")))

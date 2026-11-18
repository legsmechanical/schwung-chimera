"""Render three.js's facecap head (ARKit blendshapes) at three-quarter view, offline.

    python3 face_render.py out.png [weights json]

Loads the GLB with gltf_meshopt, applies the node transforms, mixes blendshapes, and rasterizes
with a z-buffer and Lambert shading (numpy). Used to find the look of the Vowel page's 3/4 mouth
before deciding how the device draws it.
"""
import json
import sys
import numpy as np
from gltf_meshopt import GLB

LIP_A, LIP_UP, LIP_LO = 0.072, 0.033, 0.04
BACK_TEETH_Z = 0.34   # teeth behind the canines sit in the mouth's shadow
MODEL = "/private/tmp/claude-501/-Users-josh-schwung-repos/491e8e9a-b3b5-4c4f-8e65-3b27d0b9da44/scratchpad/models/facecap.glb"


def quat_mat(q):
    x, y, z, w = q
    return np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ])


def node_mat(n):
    m = np.eye(4)
    s = np.diag(n.get("scale", [1, 1, 1]) + [1])
    r = np.eye(4)
    if "rotation" in n:
        r[:3, :3] = quat_mat(n["rotation"])
    t = np.eye(4)
    if "translation" in n:
        t[:3, 3] = n["translation"]
    return t @ r @ s


class Face:
    def __init__(self, path=MODEL):
        g = GLB(path)
        j = g.j
        parent = {}
        for i, n in enumerate(j["nodes"]):
            for c in n.get("children", []):
                parent[c] = i
        def world(i):
            m = node_mat(j["nodes"][i])
            while i in parent:
                i = parent[i]
                m = node_mat(j["nodes"][i]) @ m
            return m
        self.parts = {}
        self.names = j["meshes"][2]["extras"]["targetNames"]
        for ni, n in enumerate(j["nodes"]):
            if "mesh" not in n:
                continue
            mi = n["mesh"]
            p = j["meshes"][mi]["primitives"][0]
            M = world(ni)
            P = g.accessor(p["attributes"]["POSITION"])
            I = g.accessor(p["indices"]).astype(int).reshape(-1, 3)
            T = [g.accessor(t["POSITION"]) for t in p.get("targets", [])]
            name = {0: "eyeL", 1: "eyeR", 2: "head", 3: "teeth"}[mi]
            self.parts[name] = dict(M=M, P=P, I=I, T=T)
            if mi == 2:
                UV = g.accessor(p["attributes"]["TEXCOORD_0"])
                self.uv = (UV - UV.min(0)) / (UV.max(0) - UV.min(0))
        # the head mesh holds the skin, the lower teeth (small UV islands on the left) and the
        # mouth bag (the large island beside them); classify its faces
        V0, I0 = self.verts("head", {})
        uv = self.uv
        lower_teeth = uv[:, 0] < 0.17
        inside = (np.abs(V0[:, 0]) < 0.36) & (V0[:, 1] > -0.62) & (V0[:, 1] < -0.2) & (V0[:, 2] < 0.58)
        bag = (uv[:, 0] >= 0.17) & (uv[:, 0] < 0.5) & (uv[:, 1] < 0.56) & inside
        cls = np.zeros(len(V0), int)
        # the lips: a band round the mouth hole in UV (hole centre 0.5938, 0.8324)
        du = (uv[:, 0] - 0.5938) / LIP_A
        dv = (uv[:, 1] - 0.8324)
        dv = np.where(dv < 0, dv / LIP_UP, dv / LIP_LO)
        okreg = (uv[:, 1] > 0.6) & (uv[:, 0] > 0.4)
        self.lipness = np.where(okreg, du * du + dv * dv, 9.0)   # < 1 inside the lips
        cls[bag] = 4
        cls[lower_teeth & inside] = 5
        fc = cls[I0]
        self.head_face_class = np.where((fc == 5).sum(1) >= 2, 5, np.where((fc == 4).sum(1) >= 2, 4, 0))
        self.lower_teeth = lower_teeth & inside
        back_lt = self.lower_teeth & (V0[:, 2] < BACK_TEETH_Z)
        bf = back_lt[I0].sum(1) >= 2
        self.head_face_class[(self.head_face_class == 5) & bf] = 4
        Vt, It = self.verts("teeth", {})
        self.upper_back = (Vt[:, 2] < BACK_TEETH_Z)[It].sum(1) >= 2

    def verts(self, name, weights):
        part = self.parts[name]
        P = part["P"].copy()
        lt = getattr(self, "lower_teeth", None)
        for k, w in weights.items():
            if w and name == "head":
                d = w * part["T"][self.names.index(k)]
                # the lower teeth ride the jaw rigidly: the lip shapes must not drag them
                if lt is not None and k != "jawOpen":
                    d = d.copy()
                    d[lt] = 0
                P = P + d
        h = np.c_[P, np.ones(len(P))] @ part["M"].T
        return h[:, :3], part["I"]


def tongue_mesh(face, weights, front, high, nu=14, nv=9):
    """a tongue mound on the floor of the mouth, carried by the lower jaw: its crest forward
    (front=1, just behind the lower front teeth) or back, low or high"""
    V, _ = face.verts("head", weights)
    V0, _ = face.verts("head", {})
    lt = face.lower_teeth
    jaw_d = (V[lt] - V0[lt]).mean(0)            # how far the lower jaw carried the teeth
    tip_z, back_z, floor_y = 0.38, 0.1, -0.47
    crest_u = 0.18 + (1 - front) * 0.6
    crest_h = 0.02 + high * 0.09
    pts, idx = [], []
    for i in range(nu + 1):
        u = i / nu
        z = tip_z + (back_z - tip_z) * u
        half = 0.05 + 0.08 * np.sin(np.pi * min(1.0, 0.25 + u))
        for j in range(nv + 1):
            v = -1 + 2 * j / nv
            h = 0.03 + crest_h * np.exp(-(((u - crest_u) / 0.32) ** 2))
            y = floor_y + h * np.sqrt(max(0.0, 1 - v * v)) * (1 - 0.6 * (1 - u) ** 6)
            pts.append([v * half, y, z])
    for i in range(nu):
        for j in range(nv):
            a = i * (nv + 1) + j
            idx += [[a, a + 1, a + nv + 2], [a, a + nv + 2, a + nv + 1]]
    return np.array(pts) + jaw_d, np.array(idx)


def render(face, weights, W=512, H=184, yaw=38.0, pitch=4.0, frame=None, tongue=None):
    """grayscale render (0..1) plus a depth and part-id buffer"""
    ry, rp = np.radians(yaw), np.radians(pitch)
    Ry = np.array([[np.cos(ry), 0, np.sin(ry)], [0, 1, 0], [-np.sin(ry), 0, np.cos(ry)]])
    Rp = np.array([[1, 0, 0], [0, np.cos(rp), -np.sin(rp)], [0, np.sin(rp), np.cos(rp)]])
    R = Rp @ Ry
    tris = []
    for pid, name in enumerate(["head", "teeth", "eyeL", "eyeR"]):
        V, I = face.verts(name, weights)
        V = V @ R.T
        if name == "head":
            for c in (0, 4, 5):
                tris.append((c if c else 0, V, I[face.head_face_class == c]))
        elif name == "teeth":
            tris.append((1, V, I[~face.upper_back]))
            tris.append((4, V, I[face.upper_back]))
        elif name != "teeth":
            tris.append((pid, V, I))
    if tongue is not None:
        TV, TI = tongue_mesh(face, weights, *tongue)
        tris.append((6, TV @ R.T, TI))
    # frame: world box (x0, x1, y0, y1) of the view after rotation
    x0, x1, y0, y1 = frame
    sx = W / (x1 - x0)
    img = np.zeros((H, W))
    zb = np.full((H, W), -1e9)
    pidb = np.full((H, W), -1)
    L = np.array([-0.4, 0.5, 0.75]); L /= np.linalg.norm(L)
    for pid, V, I in tris:
        LIPN = face.lipness if pid == 0 else None
        VN = np.zeros_like(V)
        fn = np.cross(V[I[:, 1]] - V[I[:, 0]], V[I[:, 2]] - V[I[:, 0]])
        for k in range(3):
            np.add.at(VN, I[:, k], fn)
        VN /= np.maximum(1e-12, np.linalg.norm(VN, axis=1))[:, None]
        X = (V[:, 0] - x0) * sx
        Y = (y1 - V[:, 1]) * sx
        Z = V[:, 2]
        for a, b, c in I:
            xa, xb, xc = X[a], X[b], X[c]
            ya, yb, yc = Y[a], Y[b], Y[c]
            minx, maxx = int(max(0, np.floor(min(xa, xb, xc)))), int(min(W - 1, np.ceil(max(xa, xb, xc))))
            miny, maxy = int(max(0, np.floor(min(ya, yb, yc)))), int(min(H - 1, np.ceil(max(ya, yb, yc))))
            if minx > maxx or miny > maxy:
                continue
            n = np.cross(V[b] - V[a], V[c] - V[a])
            nl = np.linalg.norm(n)
            if nl == 0:
                continue
            n /= nl
            back = n[2] < 0
            den = (yb - yc) * (xa - xc) + (xc - xb) * (ya - yc)
            if abs(den) < 1e-9:
                continue
            gx, gy = np.meshgrid(np.arange(minx, maxx + 1) + 0.5, np.arange(miny, maxy + 1) + 0.5)
            w1 = ((yb - yc) * (gx - xc) + (xc - xb) * (gy - yc)) / den
            w2 = ((yc - ya) * (gx - xc) + (xa - xc) * (gy - yc)) / den
            w3 = 1 - w1 - w2
            inside = (w1 >= 0) & (w2 >= 0) & (w3 >= 0)
            if not inside.any():
                continue
            z = w1 * Z[a] + w2 * Z[b] + w3 * Z[c]
            sub = zb[miny:maxy + 1, minx:maxx + 1]
            upd = inside & (z > sub)
            sub[upd] = z[upd]
            if back or pid == 4:
                shade = np.zeros_like(z)          # the inside of the mouth: a dark cavity
            else:
                nrm = (w1[..., None] * VN[a] + w2[..., None] * VN[b] + w3[..., None] * VN[c])
                nrm /= np.maximum(1e-9, np.linalg.norm(nrm, axis=-1))[..., None]
                if back:
                    nrm = -nrm
                lam = np.clip(nrm @ L, 0, 1)
                if pid in (1, 5):
                    shade = 0.7 + 0.3 * lam        # teeth: bright
                elif pid == 6:
                    shade = 0.25 + 0.4 * lam       # tongue: a mid tone
                else:
                    shade = 0.12 + 0.88 * lam
            img[miny:maxy + 1, minx:maxx + 1][upd] = shade[upd]
            pp = np.full(z.shape, pid)
            if pid == 0:
                lp = w1 * LIPN[a] + w2 * LIPN[b] + w3 * LIPN[c]
                pp = np.where(lp < 1.0, 7, 0)
            pidb[miny:maxy + 1, minx:maxx + 1][upd] = pp[upd]
    return img, zb, pidb


if __name__ == "__main__":
    from PIL import Image
    f = Face()
    w = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    # first look: the whole head, to find the frame
    allv = np.vstack([f.verts(n, {})[0] for n in ("head",)])
    print("head bounds", allv.min(0).round(2), allv.max(0).round(2))
    fr = json.loads(sys.argv[3]) if len(sys.argv) > 3 else None
    if fr is None:
        mn, mx = allv.min(0), allv.max(0)
        fr = [mn[0] - 1, mx[0] + 1, mn[1] - 1, mx[1] + 1]
        img, _, _ = render(f, w, W=400, H=int(400 * (fr[3] - fr[2]) / (fr[1] - fr[0])), yaw=38, frame=fr)
    else:
        img, _, _ = render(f, w, frame=fr)
    Image.fromarray((img * 255).astype("uint8")).save(sys.argv[1])

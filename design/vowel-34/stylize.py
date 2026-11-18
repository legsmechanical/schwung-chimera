"""The 3/4 mouth render reduced to the Move's 1-bit 128 x 46, in the house style (white on black).

    render at 4x -> per-pixel part ids and shading -> one class and one shade per screen pixel
    background, skin      black; the face's outer edge (profile, jaw) a white line
    lips                  white; their shaded underside a light dither, for volume
    teeth                 white, with a dark outline where they meet the lips
    mouth cavity          black
    tongue                a stipple
"""
import numpy as np

W, H, S = 128, 46, 4
BAYER4 = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]) / 16.0


def reduce(pid4, img4):
    """per screen pixel: the majority class of its 4x4 block, and the mean shade"""
    p = pid4.reshape(H, S, W, S).transpose(0, 2, 1, 3).reshape(H, W, S * S)
    cls = np.zeros((H, W), int)
    for y in range(H):
        for x in range(W):
            vals, cnt = np.unique(p[y, x], return_counts=True)
            cls[y, x] = vals[np.argmax(cnt)]
    shade = img4.reshape(H, S, W, S).mean(axis=(1, 3))
    return cls, shade


def stylize(cls, shade):
    out = np.zeros((H, W), np.uint8)
    face = (cls >= 0)
    lips = cls == 7
    teeth = (cls == 1) | (cls == 5)
    tongue = cls == 6
    # lips white, their darker underside thinned by an ordered dither
    lip_on = lips & ((shade > 0.3) | (BAYER4[np.arange(H)[:, None] % 4, np.arange(W)[None, :] % 4] < 0.5))
    out[lip_on] = 1
    out[teeth] = 1
    # tongue: a stipple
    yy, xx = np.mgrid[0:H, 0:W]
    out[tongue & (((xx + yy) % 2) == 0)] = 1
    # a black moat round the teeth where they meet the lips, so the two whites read apart
    pt = np.pad(teeth, 1)
    ring = (pt[:-2, 1:-1] | pt[2:, 1:-1] | pt[1:-1, :-2] | pt[1:-1, 2:]) & ~teeth
    out[ring & lips] = 0
    # the face's outer edge: a skin pixel next to the background
    bg = np.pad(~face, 1, constant_values=False)
    edge = face & (bg[:-2, 1:-1] | bg[2:, 1:-1] | bg[1:-1, :-2] | bg[1:-1, 2:])
    out[edge & ~lips] = 1
    return out


BAYER8 = (np.array([
    [0, 32, 8, 40, 2, 34, 10, 42], [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38], [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41], [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37], [63, 31, 55, 23, 61, 29, 53, 21]]) + 0.5) / 64.0


def stylize_tone(cls, shade, skin=(0.0, 0.45), lip=(0.3, 0.75), tng=(0.25, 0.5), edge_line=True):
    """the photo look: each part toned, then an ordered dither (stable while it animates)"""
    t = np.zeros((H, W))
    sk = cls == 0
    t[sk] = skin[0] + (skin[1] - skin[0]) * shade[sk]
    lp = cls == 7
    t[lp] = lip[0] + (lip[1] - lip[0]) * shade[lp]
    tg = cls == 6
    t[tg] = tng[0] + (tng[1] - tng[0]) * shade[tg]
    teeth = (cls == 1) | (cls == 5)
    t[teeth] = 1.0
    th = BAYER8[np.arange(H)[:, None] % 8, np.arange(W)[None, :] % 8]
    out = (t > th).astype(np.uint8)
    if edge_line:
        face = cls >= 0
        bg = np.pad(~face, 1, constant_values=False)
        edge = face & (bg[:-2, 1:-1] | bg[2:, 1:-1] | bg[1:-1, :-2] | bg[1:-1, 2:])
        out[edge] = 1
    return out


def ink(pid4, img4, crease=0.22):
    """line art at 4x: class boundaries (lips, teeth, cavity, face edge) plus creases where the
    skin's shading falls off sharply (folds, the nose's underside), reduced to 1px at 1x"""
    Hh, Ww = pid4.shape
    e = np.zeros((Hh, Ww), bool)
    for dy, dx in ((0, 1), (1, 0)):
        a = pid4[: Hh - dy, : Ww - dx]
        b = pid4[dy:, dx:]
        diff = a != b
        e[: Hh - dy, : Ww - dx] |= diff
    sk = pid4 == 0
    gy, gx = np.gradient(img4)
    g = np.hypot(gx, gy)
    e |= sk & (g > crease)
    # one screen pixel per 4x4 block that holds enough edge
    blk = e.reshape(H, S, W, S).sum(axis=(1, 3))
    return (blk >= 3).astype(np.uint8)

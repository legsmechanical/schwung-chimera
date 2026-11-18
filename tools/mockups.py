"""Pixel-accurate mockups of the proposed home pages: the Move's own font (host/font.png, fetched
into build/font/), the host's chrome geometry (list_geometry.mjs, page_controller.mjs), 128x64.

    python3 tools/mockups.py      -> build/mockups/*.png and build/mockups/sheet.png
"""
import math
import os
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.join(os.path.dirname(__file__), "..")
OUT = os.path.join(ROOT, "build", "mockups")
W, H = 128, 64

# ---- the Move's bitmap font, loaded as js_display_load_font does
atlas = Image.open(os.path.join(ROOT, "build", "font", "font.png")).convert("RGBA")
chars = open(os.path.join(ROOT, "build", "font", "font.png.dat"), encoding="utf-8").readline().rstrip("\n")
cw = atlas.width // len(chars)
GLYPHS = {}
for i, ch in enumerate(chars):
    cols = []
    for x in range(cw):
        col = [atlas.getpixel((i * cw + x, y))[3] > 0 for y in range(atlas.height)]
        cols.append(col)
    ink = [x for x in range(cw) if any(cols[x])]
    if not ink:
        GLYPHS[ch] = [[False] * atlas.height for _ in range(cw)]
    else:
        GLYPHS[ch] = cols[ink[0] : ink[-1] + 1]
FONT_H = atlas.height


class Screen:
    def __init__(self):
        self.px = [[0] * W for _ in range(H)]

    def set(self, x, y, c=1):
        if 0 <= x < W and 0 <= y < H:
            self.px[y][x] = c

    def rect(self, x, y, w, h, c=1):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                self.set(xx, yy, c)

    def frame(self, x, y, w, h, c=1):
        self.rect(x, y, w, 1, c)
        self.rect(x, y + h - 1, w, 1, c)
        self.rect(x, y, 1, h, c)
        self.rect(x + w - 1, y, 1, h, c)

    def width(self, s):
        return sum(len(GLYPHS.get(ch, GLYPHS[" "])) + 1 for ch in s) - (1 if s else 0)

    def text(self, x, y, s, c=1):
        for ch in s:
            g = GLYPHS.get(ch, GLYPHS[" "])
            for gx, col in enumerate(g):
                for gy, on in enumerate(col):
                    if on:
                        self.set(x + gx, y + gy, c)
            x += len(g) + 1
        return x

    def text_r(self, xr, y, s, c=1):
        self.text(xr - self.width(s), y, s, c)

    def inv_text(self, x, y, s):
        """a word on an inverted box"""
        w = self.width(s)
        self.rect(x - 1, y - 1, w + 2, FONT_H + 2, 1)
        self.text(x, y, s, 0)
        return x + w + 1

    def line(self, x0, y0, x1, y1, c=1):
        n = max(abs(x1 - x0), abs(y1 - y0), 1)
        for i in range(n + 1):
            self.set(round(x0 + (x1 - x0) * i / n), round(y0 + (y1 - y0) * i / n), c)

    def dotted(self, x0, x1, y):
        for x in range(x0, x1, 2):
            self.set(x, y)


# ---- host chrome
def chrome(s, title, footer, browser=False):
    s.text(1, 0, title)  # 7 rows above the title rule
    s.rect(0, 7, W, 1)  # TITLE_RULE_Y
    s.rect(0, 55, W, 1)  # RULE_Y; below it the host's own footer (not drawn here)
    if browser:  # the preset page's brackets: frame 4,9,120x45, arm 4
        x, y, w, h, a = 4, 9, 120, 45, 4
        for cx, cy, dx, dy in [(x, y, 1, 1), (x + w - 1, y, -1, 1), (x, y + h - 1, 1, -1), (x + w - 1, y + h - 1, -1, -1)]:
            for i in range(a):
                s.set(cx + dx * i, cy)
                s.set(cx, cy + dy * i)


def preset_line(s, name, idx, count):
    s.text(7, 11, name)
    s.text_r(121, 11, f"{idx}/{count}")
    s.dotted(7, 121, 19)


def words_window(s, before, current, after, y0):
    """two lines of a text window, the current word/syllable inverted"""
    x, y = 7, y0
    for i, wd in enumerate(before + [current] + after):
        wdt = s.width(wd)
        if x + wdt > 121:
            x, y = 7, y + 10
            if y > y0 + 10:
                break
        if wd is current and i == len(before):
            x = s.inv_text(x, y, wd) + 3
        else:
            x = s.text(x, y, wd) + 3


# ---- the pages
def speak():
    s = Screen()
    chrome(s, "Speak", "Home   Tone", browser=True)
    preset_line(s, "Spoken Word", 22, 24)
    words_window(s, ["Hello", "there.", "How"], "are", ["you", "doing", "today?"], 22)
    s.frame(7, 41, 114, 3)
    s.rect(8, 42, 64, 1)
    s.text(7, 45, "Rate 1.0x")
    s.text_r(121, 45, "Expr 35%")
    return s


def sing():
    s = Screen()
    chrome(s, "Sing", "Home   Tone   Pitch", browser=True)
    preset_line(s, "Made of Air", 1, 24)
    words_window(s, ["breathe", "in,", "breathe"], "out,", ["I", "am", "made", "of", "air."], 22)
    s.text(7, 45, "8 of 22")
    s.text_r(121, 45, "Vib 35c")
    return s


def tract(s, x0, x1, ymid, half, nose=True):
    """an airway profile: throat on the left, lips on the right (an /a/: narrow pharynx,
    wide front)"""
    n = 40
    for i in range(n + 1):
        t = i / n
        # area profile of an "ah": a pharyngeal constriction, then open toward the lips
        a = 0.35 + 0.5 * (1 / (1 + math.exp(-12 * (t - 0.45)))) - 0.18 * math.exp(-((t - 0.3) / 0.08) ** 2)
        r = max(1, round(a * half))
        x = round(x0 + (x1 - x0) * t)
        s.set(x, ymid - r)
        s.set(x, ymid + r)
        if i:
            px, pr = prev
            s.line(px, ymid - pr, x, ymid - r)
            s.line(px, ymid + pr, x, ymid + r)
        prev = (x, r)
    # the folds at the left end, the lips' opening at the right
    s.rect(x0, ymid - 1, 1, 3)
    if nose:
        vx = round(x0 + (x1 - x0) * 0.55)
        s.line(vx, ymid - round(0.6 * half), vx, ymid - half - 2)
        s.dotted(vx, x1 - 6, ymid - half - 2)


def vowel():
    s = Screen()
    chrome(s, "Vowel", "Home  Tone  Pitch  Mouth  Throat", browser=True)
    preset_line(s, "Drone Ah", 7, 24)
    tract(s, 10, 116, 31, 8)
    s.text(7, 45, "Ah")
    s.text(26, 45, "A3 220 Hz")
    s.inv_text(121 - s.width("Hold"), 45, "Hold")
    return s


def mouth():
    s = Screen()
    chrome(s, "Mouth", "Home  Tone  Pitch  Mouth  Throat")
    tract(s, 6, 121, 27, 12, nose=False)
    s.text(1, 46, "throat")
    s.text_r(127, 46, "lips")
    s.text_c = None
    s.inv_text(64 - s.width("Jaw 62%") // 2, 46, "Jaw 62%")
    return s


def beatbox():
    s = Screen()
    chrome(s, "Beatbox", "Home   Tone", browser=True)
    preset_line(s, "Boots & Cats", 1, 8)
    rows = [("kick", [0, 8, 10]), ("snr", [4, 12]), ("hat", [2, 6, 10, 14])]
    gx, gw = 30, 5
    play = 6
    s.rect(gx + play * gw - 1, 20, gw, 24)  # the playhead column
    for r, (name, hits) in enumerate(rows):
        y = 21 + r * 8
        s.text(7, y, name)
        for st in range(16):
            x = gx + st * gw
            on = st in hits
            c = 0 if st == play else 1
            if on:
                s.rect(x, y + 1, 4, 4, c)
            elif st % 4 == 0:
                s.set(x + 1, y + 3, c)
    s.text(7, 46, "96 bpm")
    s.inv_text(64 - s.width("snare") // 2, 46, "snare")
    s.text_r(121, 46, "Sw 10%")
    return s


PAGES = [
    ("Voice - Speak (home)", speak),
    ("Voice - Sing (home)", sing),
    ("Voice - Vowel (home)", vowel),
    ("Voice - Vowel, Mouth page", mouth),
    ("Beatbox (home)", beatbox),
]


def to_image(s, scale):
    img = Image.new("RGB", (W * scale, H * scale), (0, 0, 0))
    d = ImageDraw.Draw(img)
    for y in range(H):
        for x in range(W):
            if s.px[y][x]:
                d.rectangle([x * scale, y * scale, x * scale + scale - 1, y * scale + scale - 1], fill=(235, 235, 235))
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    scale, pad, cap = 5, 24, 30
    shots = []
    for name, fn in PAGES:
        img = to_image(fn(), scale)
        img.save(os.path.join(OUT, name.split(" ")[0].lower() + "_" + fn.__name__ + ".png"))
        shots.append((name, img))
    cols = 2
    rows = (len(shots) + cols - 1) // cols
    sw, sh = W * scale, H * scale
    sheet = Image.new("RGB", (cols * sw + (cols + 1) * pad, rows * (sh + cap) + (rows + 1) * pad), (245, 245, 242))
    d = ImageDraw.Draw(sheet)
    try:
        f = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 20)
    except OSError:
        f = ImageFont.load_default()
    for i, (name, img) in enumerate(shots):
        cx = pad + (i % cols) * (sw + pad)
        cy = pad + (i // cols) * (sh + cap + pad)
        d.text((cx, cy), name, fill=(40, 40, 40), font=f)
        sheet.paste(img, (cx, cy + cap))
    sheet.save(os.path.join(OUT, "sheet.png"))
    print(os.path.join(OUT, "sheet.png"))


main()

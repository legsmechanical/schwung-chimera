"""The README's pictures, through the host's own page renderer (build/preview, as build/render_voice.py):
docs/mouth.gif (the Vowel page sweeping Oo..Ee and back), docs/lyrics.png, docs/line.png.
Run build/render_voice.py once first (it writes build/voice-fixture.json and the widgets)."""
import json, os, subprocess
from PIL import Image

repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
build, docs = f"{repo}/build", f"{repo}/docs"
os.makedirs(docs, exist_ok=True)
tmp = f"{build}/readme_tmp"
os.makedirs(tmp, exist_ok=True)


def page(vals, pg=1):
    env = dict(os.environ, PREVIEW_FIXTURE=f"{build}/voice-fixture.json", PREVIEW_VALUES=json.dumps(vals))
    subprocess.run(["node", "preview.mjs", "chimera", "--page", str(pg), "--layout", "movy", "--fixture", f"{build}/voice-fixture.json",
                    "--widgets", f"{build}/voice_widgets", "--png", tmp, "--scale", "3"], env=env, check=True, capture_output=True, cwd=f"{build}/preview")
    f = f"{tmp}/chimera-{pg:02d}.png"
    im = Image.open(f).convert("L")
    os.remove(f)
    return im


# the mouth: a sweep along the Vowel knob's path and back, eased at the ends
n = 28
xs = [0.5 - 0.5 * __import__("math").cos(__import__("math").pi * i / (n - 1)) for i in range(n)]
frames = [page({"mode": "2", "sweep": f"{x:.3f}"}) for x in xs + xs[-2:0:-1]]
frames[0].save(f"{docs}/mouth.gif", save_all=True, append_images=frames[1:], duration=70, loop=0, optimize=True)

page({"mode": "1", "position": "8", "chm_view": "S|7|breathe in, breathe out, I am made of air. tongue and lips and throat, sing·ing an·y·where. "}).save(f"{docs}/lyrics.png")
page({"mode": "0", "chm_view": "P|900|300,520,800,950,1050,1400,1600,1750,2050|Hello there, I am a voice made of air."}).save(f"{docs}/line.png")
os.rmdir(tmp)
print("docs/mouth.gif", len(frames), "frames; docs/lyrics.png; docs/line.png")

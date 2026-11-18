"""Run our JS drawer (via node) at given knob values and the reference class map for a keyframe,
side by side at 4x: python3 drawer_check.py out.png"""
import json, subprocess, sys
import numpy as np
from PIL import Image, ImageDraw

def drawer(values):
    js = open("../../build/canvas34/canvas.js").read()
    prog = js + """
;const W=128,H=46,px=new Uint8Array(W*H);
const ctx={width:W,height:H,textWidth:s=>String(s).length*5,print(){},line(){},
  fillRect(x,y,w,h,c){for(let j=y;j<y+h;j++)for(let i=x;i<x+w;i++)if(j>=0&&j<H&&i>=0&&i<W)px[j*W+i]=c?1:0}};
globalThis.canvas_overlay.drawPage(ctx,{key:"mouthview",values:%s});
process.stdout.write(Buffer.from(px).toString("base64"));""" % json.dumps(values)
    out = subprocess.run(["node", "-e", "const globalThis_=globalThis;" + prog], capture_output=True, text=True)
    if out.returncode:
        raise SystemExit(out.stderr)
    import base64
    return np.frombuffer(base64.b64decode(out.stdout), np.uint8).reshape(46, 128)

def ref(name):
    pid = np.load(f"ref_pid_{name}.npy")
    col = np.zeros(pid.shape + (3,), np.uint8)
    pal = {-1: (0, 0, 0), 0: (90, 80, 70), 1: (255, 255, 255), 4: (10, 10, 40), 5: (255, 255, 255), 6: (90, 160, 90), 7: (220, 60, 60)}
    for k, c in pal.items():
        col[pid == k] = c
    return Image.fromarray(col)

if __name__ == "__main__":
    cases = [("j0_w1", {"jaw": "0", "lips": "0", "round": "1", "protrude": "0", "tongue_x": "0.4", "tongue_y": "0.5"}),
             ("j1_w1", {"jaw": "0.87", "lips": "0", "round": "1", "protrude": "0", "tongue_x": "0.4", "tongue_y": "0.5"}),
             ("pout", {"jaw": "0", "lips": "0", "round": "0", "protrude": "1", "tongue_x": "0.4", "tongue_y": "0.5"})]
    rows = []
    for name, v in cases:
        b = drawer(v)
        rows.append((name, ref(name), Image.fromarray(b * 235).resize((512, 184), Image.NEAREST)))
    sh = Image.new("RGB", (2 * 520, len(rows) * 200), (245, 245, 242))
    d = ImageDraw.Draw(sh)
    for i, (n, r, b) in enumerate(rows):
        y = i * 200
        d.text((3, y + 1), f"{n}: reference class map", fill=(0, 0, 0)); d.text((523, y + 1), f"{n}: our drawer at that keyframe", fill=(0, 0, 0))
        sh.paste(r, (0, y + 14)); sh.paste(b.convert("RGB"), (520, y + 14))
    sh.save(sys.argv[1])

#!/usr/bin/env python3
"""CHIMERA inside the real host, on a Move running stock Schwung: load it into chain slot 0
(track 1) through upstream's test bus, check it answers, play pads, capture what it rendered,
then put slot 0 back the way it was.

  python3 tests/device_check.py [--keep]

Uses schwung-monomodule/tools/devicetest/move.py (screen + schwung-testd) and
schwung-current/tools/pytest-schwung (the bus client). Refuses if slot 0 is not empty, so a set
is never changed under the user. Captures the module's own output with the shim's align dump
(slot0_synth_src.pcm) to out/device_slot0.wav.
"""
from __future__ import annotations

import struct
import subprocess
import sys
import time
import wave
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO.parent / "schwung-monomodule/tools/devicetest"))
import move  # noqa: E402

SCHW = "/data/UserData/schwung"


def rms(pcm: bytes) -> float:
    n = len(pcm) // 4
    if n == 0:
        return 0.0
    s = struct.unpack(f"<{n * 2}h", pcm[: n * 4])
    left = s[0::2]
    return (sum(x * x for x in left) / n) ** 0.5 / 32767


def main() -> int:
    keep = "--keep" in sys.argv
    with move.Device() as d:
        def get(k):
            # an empty slot answers a synth read with an error, not ""
            try:
                return d.bus.get_param(k, overtake=False)
            except Exception as e:  # noqa: BLE001
                return f"<error: {e}>" if k != "synth:module" else None

        put = lambda k, v: d.bus.set_param(k, v, overtake=False)  # noqa: E731
        # an overtake tool owns the pads: notes would never reach track 1
        view = subprocess.run(["ssh", "ableton@move.local", f"tail -n 50 {SCHW}/debug.log | grep -a -c 'ui_view: overtakemodule'"], capture_output=True, text=True).stdout.strip()
        if view not in ("", "0"):
            print("an overtake tool is open on the Move (it owns the pads): close it first")
            return 1
        before = get("synth:module")
        print(f"slot 0 synth before: {before!r}")
        if before not in (None, "", "none"):
            print("slot 0 is in use: not touching it")
            return 1
        put("synth:module", "chimera")
        for _ in range(50):
            d.settle(10)
            if get("synth:module") == "chimera":
                break
        print("synth:module ->", get("synth:module"))
        print("build        ->", get("synth:chm_build"))
        for _ in range(30):
            st = get("synth:chm_status") or ""
            if "ready=1" in st:
                break
            d.settle(10)
        print("status       ->", st)
        print("preset       ->", get("synth:preset_name"))
        print("mode         ->", get("synth:mode"))
        h = get("synth:ui_hierarchy") or ""
        print(f"ui_hierarchy -> {len(h)} bytes")

        # capture the module's own output while pads play (shim align dump, 6 s)
        subprocess.run(["ssh", "ableton@move.local", f"echo 6 > {SCHW}/align_dump_trigger"], check=True)
        syl = [get("synth:next_syllable")]
        for note in (68, 70, 72, 73, 75, 77):
            d.pad(note, 350)
            d.settle(8)
            syl.append(get("synth:next_syllable"))
        print("next syllable as pads play:", " -> ".join(repr(s) for s in syl))
        time.sleep(4)
        print("status after ->", get("synth:chm_status"))

        if not keep:
            put("synth:module", "")
            d.settle(30)
            print("slot 0 synth after:", repr(get("synth:module")))

    out = REPO / "out" / "device_slot0.wav"
    pcm = subprocess.run(["ssh", "ableton@move.local", f"cat {SCHW}/slot0_synth_src.pcm"], capture_output=True).stdout
    with wave.open(str(out), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(44100)
        w.writeframes(pcm)
    print(f"captured {len(pcm) / 4 / 44100:.1f} s of slot 0's synth, rms {rms(pcm):.4f} -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

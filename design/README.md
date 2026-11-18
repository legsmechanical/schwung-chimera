# Design working files (not shipped)

How the Vowel page's mouth was drawn. Nothing here goes into the module except what
`tools/gen.mjs` takes from `src/` (`src/mouth_keys.json` is a copy of
`vowel-34/keyframes_front.min.json`).

- `vowel-front/` — the first front-view lips drawer, kept as a fallback.
- `vowel-34/` — a 3D head with ARKit blendshapes (three.js's `facecap.glb` example model, read
  from outside the repo; the path is in `face_render.py`) rendered offline as a **reference
  only**: `gltf_meshopt.py` decodes it, `face_render.py` rasterizes it, `front_ref.py` /
  `front_keyframes.py` measure the lips' outline and opening at the keyframe poses into
  `keyframes_front.min.json` (coordinates only). `mouth_front.js` is our own drawer, now in
  `src/canvas.src.js`. The three-quarter attempt (`keyframes2.py`, `mouth34.js`, …) was set
  aside. Renders of the model and web photos are git-ignored.
- `vowel-34/front-compare.png`, `mouth-front.gif` — the drawing as approved.

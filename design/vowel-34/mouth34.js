/*
 * CHIMERA Vowel page: the mouth in three-quarter view. Our own drawing: polygons blended from
 * keyframes whose proportions were measured off a 3D head used only as a reference
 * (design/vowel-34/keyframes2.py); no model data ships.
 *
 * MOUTH34_KEYS (generated into this file), per keyframe:
 *   outer          the lips' outline, 64 points by rays from its centre
 *   hole_u, hole_l the opening's upper and lower edges, 17 points between its own corners;
 *                  closed, both lie on the seam between the lips
 *   edge           the face's outer edge (profile, jaw): x per row, -1 where none
 * Keyframes: jaw 0 / 0.5 / 1 x lips spread (w-1) / neutral (w0) / rounded (w1), and pout.
 *
 * From the Mouth knobs: Jaw (and a little of Lips) opens; Round runs neutral -> rounded; a
 * spread comes in for front, high-tongue vowels with the jaw nearly closed (Ee); Pout pushes the
 * lips forward. Front and Height place the tongue; the nearest vowel is named in the corner.
 */
function drawMouth34(ctx, values, VOWELS, FONT_H) {
    const K = MOUTH34_KEYS;
    const g = (k, d) => {
        const x = parseFloat(values[k]);
        return isFinite(x) ? Math.max(0, Math.min(1, x)) : d;
    };
    const front = g("tongue_x", 0.5), high = g("tongue_y", 0.5), jaw = g("jaw", 0.3);
    const lips = g("lips", 0.5), round = g("round", 0), pout = g("protrude", 0);
    const W = ctx.width, H = ctx.height;

    /* ---- where in the keyframe grid */
    const open = Math.min(1, jaw * 1.15 + lips * 0.12);
    const spread = (1 - round) * Math.max(0, Math.min(1, (front - 0.45) * 3)) * Math.max(0, 1 - jaw * 1.4);
    const width = Math.max(-1, Math.min(1, round - spread));
    const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
    const rowAt = (j, field) => {
        const w0 = K[`j${j}_w0`][field];
        return width < 0 ? mix(w0, K[`j${j}_w-1`][field], -width) : mix(w0, K[`j${j}_w1`][field], width);
    };
    const at = (field) => {
        const v = open <= 0.5 ? mix(rowAt(0, field), rowAt(0.5, field), open * 2)
                              : mix(rowAt(0.5, field), rowAt(1, field), (open - 0.5) * 2);
        const pw = pout * (1 - 0.75 * Math.max(0, width));
        return pw > 0 ? v.map((x, i) => x + (K.pout[field][i] - K.j0_w0[field][i]) * pw) : v;
    };
    const pts = (flat) => { const p = []; for (let i = 0; i < flat.length; i += 2) p.push([flat[i], flat[i + 1]]); return p; };
    const outer = pts(at("outer")), hu = pts(at("hole_u")), hl = pts(at("hole_l"));

    /* ---- regions, by polygon fill */
    const reg = new Uint8Array(W * H);   /* 1 lips, 3 opening */
    const fill = (poly, c) => {
        let y0 = Infinity, y1 = -Infinity;
        for (const p of poly) { y0 = Math.min(y0, p[1]); y1 = Math.max(y1, p[1]); }
        for (let y = Math.max(0, Math.floor(y0)); y <= Math.min(H - 1, Math.ceil(y1)); y++) {
            const yc = y + 0.5, xs = [];
            for (let i = 0; i < poly.length; i++) {
                const a = poly[i], b = poly[(i + 1) % poly.length];
                if ((a[1] <= yc && b[1] > yc) || (b[1] <= yc && a[1] > yc)) xs.push(a[0] + ((yc - a[1]) * (b[0] - a[0])) / (b[1] - a[1]));
            }
            xs.sort((p, q) => p - q);
            for (let k = 0; k + 1 < xs.length; k += 2)
                for (let x = Math.max(0, Math.round(xs[k])); x < Math.min(W, Math.round(xs[k + 1])); x++) reg[y * W + x] = c;
        }
    };
    fill(outer, 1);
    let holeH = 0;
    for (let i = 0; i < hu.length; i++) holeH = Math.max(holeH, hl[i][1] - hu[i][1]);
    if (holeH > 0.6) fill([...hu, ...hl.slice().reverse()], 3);

    /* the seam: through the opening's middle, out to the lips' two corners */
    let lc = outer[0], rc = outer[0];
    for (const p of outer) { if (p[0] < lc[0]) lc = p; if (p[0] > rc[0]) rc = p; }
    const lineAt = (poly, x) => {
        if (x <= poly[0][0]) return poly[0][1];
        if (x >= poly[poly.length - 1][0]) return poly[poly.length - 1][1];
        for (let i = 1; i < poly.length; i++) {
            const a = poly[i - 1], b = poly[i];
            if (x <= b[0]) return b[0] === a[0] ? a[1] : a[1] + ((b[1] - a[1]) * (x - a[0])) / (b[0] - a[0]);
        }
        return poly[poly.length - 1][1];
    };
    const mid = hu.map((p, i) => [p[0], (p[1] + hl[i][1]) / 2]);
    const seam = [lc, ...mid, rc].sort((a, b) => a[0] - b[0]);

    /* ---- the lips' shading: a 4x4 ordered dither. Upper lip mid-dark (it faces down),
     * lower lip bright at the seam and darker toward its underside; the side turning away
     * (beyond the opening's far corner) falls into shadow */
    const B = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    const px = new Uint8Array(W * H);
    const farX = hu[hu.length - 1][0], farW = Math.max(2, rc[0] - farX);
    for (let x = 0; x < W; x++) {
        let top = -1, bot = -1;
        for (let y = 0; y < H; y++) if (reg[y * W + x]) { if (top < 0) top = y; bot = y; }
        if (top < 0) continue;
        const s = lineAt(seam, x + 0.5);
        const far = x > farX ? Math.min(1, (x - farX) / farW) : 0;
        for (let y = top; y <= bot; y++) {
            if (reg[y * W + x] !== 1) continue;
            let tone;
            if (y + 0.5 < s) {
                const v = (y + 0.5 - top) / Math.max(1, s - top);
                tone = 0.38 + 0.22 * v;
            } else {
                const v = (y + 0.5 - s) / Math.max(1, bot + 1 - s);
                tone = 0.95 - 0.5 * v * v;
            }
            tone *= 1 - 0.3 * far;
            if (tone * 16 > B[(y & 3) * 4 + (x & 3)]) px[y * W + x] = 1;
        }
    }

    /* ---- inside the opening: upper teeth below the lip, lower teeth when well open, the
     * tongue on the floor. Positions run across the opening between its own corners. */
    const h0 = hu[0][0], h1 = hu[hu.length - 1][0];
    for (let x = Math.ceil(h0); x <= Math.floor(h1); x++) {
        const t = (x + 0.5 - h0) / Math.max(1, h1 - h0);
        const top = lineAt(hu, x + 0.5), bot = lineAt(hl, x + 0.5), gap = bot - top;
        if (gap < 1.2) continue;
        if (round < 0.6 && t > 0.08 && t < 0.95) {
            const th = Math.min(4, gap * 0.38) * (1 - 0.6 * round);
            for (let y = Math.ceil(top) + 1; y < top + 1 + th; y++) if (reg[y * W + x] === 3) px[y * W + x] = 1;
        }
        if (open > 0.55 && round < 0.6 && t > 0.2 && t < 0.85) {
            const th = Math.min(2.5, gap * 0.16);
            for (let y = Math.floor(bot) - 1; y > bot - 1 - th; y--) if (reg[y * W + x] === 3) px[y * W + x] = 1;
        }
        if (gap > 6) {
            /* forward is toward the front of the mouth (the far side, by the profile) */
            const crest = 0.3 + front * 0.5;
            const h = gap * (0.08 + high * 0.55) * Math.exp(-(((t - crest) / 0.24) ** 2));
            for (let y = Math.floor(bot) - 1; y > bot - 1 - h; y--)
                if (y >= 0 && reg[y * W + x] === 3 && !px[y * W + x] && ((x + y) & 1) === 0) px[y * W + x] = 1;
        }
    }

    /* ---- the face's edge (profile, jaw), behind the lips */
    const edgeRow = (j) => {
        const a = K[`j${j}_w0`].edge, b = width < 0 ? K[`j${j}_w-1`].edge : K[`j${j}_w1`].edge, m = Math.abs(width);
        return a.map((v, i) => (v < 0 || b[i] < 0 ? v : v + (b[i] - v) * m));
    };
    const ea = open <= 0.5 ? edgeRow(0) : edgeRow(0.5), eb = open <= 0.5 ? edgeRow(0.5) : edgeRow(1);
    const et = open <= 0.5 ? open * 2 : (open - 0.5) * 2;
    let prev = -1;
    for (let y = 0; y < H; y++) {
        const va = ea[y], vb = eb[y];
        const x = va < 0 ? vb : vb < 0 ? va : va + (vb - va) * et;
        if (x < 0) { prev = -1; continue; }
        const xr = Math.round(x);
        const xa = prev < 0 ? xr : Math.min(prev, xr), xb = prev < 0 ? xr : Math.max(prev, xr);
        for (let xx = xa; xx <= xb; xx++) if (xx >= 0 && xx < W && reg[y * W + xx] !== 1) px[y * W + xx] = 1;
        prev = xr;
    }

    for (let y = 0; y < H; y++) {
        let x = 0;
        while (x < W) {
            if (!px[y * W + x]) { x++; continue; }
            let e = x;
            while (e < W && px[y * W + e]) e++;
            ctx.fillRect(x, y, e - x, 1, 1);
            x = e;
        }
    }
    let nm = "", best = 1e9;
    for (const [n, f, h] of VOWELS) {
        const d = (f - front) ** 2 + (h - high) ** 2;
        if (d < best) { best = d; nm = n; }
    }
    ctx.print(1, H - FONT_H - 1, nm, 1);
}

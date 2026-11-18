/*
 * CHIMERA Voice's drawn pages. Each is an `as_page` canvas with `page_first`, so it comes before
 * its level's knob grid and carries the same knobs; the host draws the header, page bar and
 * footer.
 *
 *   line    Speak  the line, the word being said inverted
 *   sung    Sing   the lyrics in syllables, the one the next key sings inverted (Position, live)
 *   mouth   Vowel  the mouth face-on, at the Vowel knob's place on its path (live: it follows
 *                  pad pressure)
 *   shape   Mouth  the mouth face-on, from the Mouth page's own knobs
 *
 * From `chm_view` (served by the plugin):
 *   P|<ms since the line started, -1: silent>|<when each word ends, ms, comma separated>|<line>
 *   S|<the syllables: each one's text, then ' ' at a word's end or SYL '·' within a word>
 *
 * The mouth is our own drawing: its shapes are blended from keyframes measured off a 3D head
 * used only as a reference (design/vowel-34/front_keyframes.py; no model data ships).
 *
 * MOUTH_KEYS and SWEEP_KNOBS come first in the built file (tools/gen.mjs writes it from
 * src/canvas.src.js, src/mouth_keys.json and src/params.json).
 */
(function () {
    const FONT_H = 7, LINE = 9, SYL = "\u00b7";

    /* vowel landmarks in Mouth-knob space (front 0..1, height 0..1), from the engine's vowel
     * table (male body), named as the Pick Vowel knob names them */
    const VOWELS = [
        ["Ah", 0.27, 0.20], ["Ee", 0.71, 0.67], ["Ih", 0.69, 0.60], ["Ay", 0.68, 0.55],
        ["Eh", 0.63, 0.40], ["Ae", 0.62, 0.28], ["Aw", 0.13, 0.30], ["Oh", 0.14, 0.41],
        ["Uh", 0.25, 0.33], ["Oo", 0.37, 0.60], ["Book", 0.35, 0.53], ["Er", 0.45, 0.56],
        ["Schwa", 0.51, 0.43],
    ];

    /*
     * Flowed text: tokens on lines, token `cur` inverted, and the view scrolled so its line is
     * the second one shown (what is coming matters more than what has gone).
     * tokens: [{ t, gap }], gap = a space before it.
     */
    function flow(ctx, tokens, cur) {
        const w = ctx.width - 2, lines = [];
        let line = [], x = 0;
        for (let i = 0; i < tokens.length; i++) {
            const tw = ctx.textWidth(tokens[i].t);
            let g = line.length && tokens[i].gap ? 4 : 0;
            if (line.length && x + g + tw > w) {
                lines.push(line);
                line = [];
                x = 0;
                g = 0;
            }
            line.push({ i, x: x + g, t: tokens[i].t });
            x += g + tw;
        }
        if (line.length) lines.push(line);
        const shown = Math.floor((ctx.height + 2) / LINE);
        const at = lines.findIndex((l) => l.some((s) => s.i === cur));
        const first = Math.max(0, Math.min((at < 0 ? 0 : at) - 1, lines.length - shown));
        for (let r = 0; r < shown && first + r < lines.length; r++) {
            const y = r * LINE + 1;
            for (const s of lines[first + r]) {
                if (s.i === cur) {
                    const tw = ctx.textWidth(s.t);
                    ctx.fillRect(s.x, y - 1, tw + 2, FONT_H + 2, 1);
                    ctx.print(1 + s.x, y, s.t, 0);
                } else ctx.print(1 + s.x, y, s.t, 1);
            }
        }
    }

    /* ---- Speak: the line, the word being said inverted. The view says how far into the line
     * the voice was when it was read (a few times a second); between reads the page counts on
     * from there with its own clock, so the highlight keeps time with the words. */
    const line = { raw: null, at: 0, el: -1, ends: [], words: [] };
    function drawLine(ctx, v, nowMs) {
        if (typeof v !== "string" || v[0] !== "P") return;
        if (v !== line.raw) {
            const a = v.indexOf("|", 2), b = v.indexOf("|", a + 1);
            line.raw = v;
            line.at = nowMs;
            line.el = parseInt(v.slice(2, a), 10);
            line.ends = v.slice(a + 1, b).split(",").filter(Boolean).map(Number);
            line.words = v.slice(b + 1).split(/\s+/).filter(Boolean);
        }
        let cur = -1;
        if (line.el >= 0) {
            const t = line.el + (nowMs - line.at);
            cur = line.ends.findIndex((e) => e > t);
            if (cur < 0) cur = line.words.length - 1;
        }
        flow(ctx, line.words.map((t) => ({ t, gap: true })), cur);
    }

    /* ---- Sing: the syllables, the next one inverted (Position) */
    function drawLyrics(ctx, v, pos) {
        if (typeof v !== "string" || v[0] !== "S") return;
        const tokens = [];
        for (const word of v.slice(2).split(" ").filter(Boolean)) {
            const syl = word.split(SYL);
            syl.forEach((s, k) => tokens.push({ t: k < syl.length - 1 ? s + "-" : s, gap: k === 0 }));
        }
        const p = parseInt(pos, 10);
        flow(ctx, tokens, isFinite(p) ? Math.min(p, tokens.length - 1) : 0);
    }

    /* ---- the mouth. Eased toward what the values say, frame by frame, so a knob's steps (and
     * a jump between vowels) glide rather than snap; the voice glides too. */
    const MOUTH = ["tongue_x", "tongue_y", "jaw", "lips", "round", "protrude"];
    const eased = {};
    function ease(target, nowMs) {
        const dt = eased.at === undefined ? 1e9 : Math.max(0, nowMs - eased.at);
        eased.at = nowMs;
        const a = 1 - Math.exp(-dt / 45);
        const out = {};
        for (const k of MOUTH) {
            const t = parseFloat(target[k]);
            if (!isFinite(t)) continue;
            eased[k] = eased[k] === undefined ? t : eased[k] + (t - eased[k]) * a;
            out[k] = eased[k];
        }
        return out;
    }

    /* the Vowel knob's place on its path, as Mouth-knob values */
    function onPath(x) {
        const rows = SWEEP_KNOBS.rows, n = rows.length;
        const f = Math.max(0, Math.min(1, x)) * (n - 1);
        const i = Math.min(n - 2, Math.floor(f)), t = f - i;
        const out = {};
        SWEEP_KNOBS.keys.forEach((k, j) => (out[k] = rows[i][j] + (rows[i + 1][j] - rows[i][j]) * t));
        return out;
    }

    function drawMouthFront(ctx, values, VOWELS, FONT_H) {
        const K = MOUTH_KEYS;
        const g = (k, d) => {
            const x = parseFloat(values[k]);
            return isFinite(x) ? Math.max(0, Math.min(1, x)) : d;
        };
        const front = g("tongue_x", 0.5), high = g("tongue_y", 0.5), jaw = g("jaw", 0.3);
        const lips = g("lips", 0.5), round = g("round", 0), pout = g("protrude", 0);
        const W = ctx.width, H = ctx.height;

        /* ---- where in the keyframe grid (open = 1 is the widest vowel, Ah) */
        const open = Math.min(1, jaw * 1.25 + lips * 0.12);
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
        /* drawn wider than measured (about 30%): the page is short, so the measured scale is set by
         * the tallest shape; widening keeps the heights and gives the mouth the page's width */
        const XS = 1.3, XC = W / 2;
        const sx = (p) => p.map(([x, y]) => [XC + (x - XC) * XS, y]);
        const outer = sx(pts(at("outer"))), hu = sx(pts(at("hole_u"))), hl = sx(pts(at("hole_l")));
        /* framed the same for every shape: the measured head keeps its upper lip still and drops the
         * jaw, so a closed mouth sat high and an open one low; centre the lips on the page instead */
        {
            let y0 = Infinity, y1 = -Infinity;
            for (const p of outer) { y0 = Math.min(y0, p[1]); y1 = Math.max(y1, p[1]); }
            const dy = H / 2 - (y0 + y1) / 2;
            for (const q of [outer, hu, hl]) for (const p of q) p[1] += dy;
        }
        /* a cupid's bow on the upper lip's top edge: two small peaks either side of a dip at the
         * middle (the reference head's lips are flat there; the bow is ours) */
        {
            let minX = Infinity, maxX = -Infinity, topY = Infinity;
            for (const [x, y] of outer) { minX = Math.min(minX, x); maxX = Math.max(maxX, x); topY = Math.min(topY, y); }
            const cxm = (minX + maxX) / 2, hw = (maxX - minX) / 2;
            const seamY = (hu[0][1] + hu[hu.length - 1][1]) / 2;
            for (const p of outer) {
                if (p[1] > seamY) continue;
                const u = (p[0] - cxm) / hw;
                p[1] += 1.4 * Math.exp(-((u / 0.07) ** 2)) - 1.2 * Math.exp(-(((Math.abs(u) - 0.2) / 0.1) ** 2));
            }
        }

        /* ---- regions */
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
        const isOpen = holeH > 1.0;
        if (isOpen) fill([...hu, ...hl.slice().reverse()], 3);

        const lineAt = (poly, x) => {
            if (x <= poly[0][0]) return poly[0][1];
            if (x >= poly[poly.length - 1][0]) return poly[poly.length - 1][1];
            for (let i = 1; i < poly.length; i++) {
                const a = poly[i - 1], b = poly[i];
                if (x <= b[0]) return b[0] === a[0] ? a[1] : a[1] + ((b[1] - a[1]) * (x - a[0])) / (b[0] - a[0]);
            }
            return poly[poly.length - 1][1];
        };

        /* ---- the lips: solid */
        const px = new Uint8Array(W * H);
        for (let i = 0; i < px.length; i++) if (reg[i] === 1) px[i] = 1;
        /* the seam, when closed: a dark line between the lips, corner to corner */
        if (!isOpen) {
            for (let x = Math.ceil(hu[0][0]); x <= Math.floor(hu[hu.length - 1][0]); x++) {
                const y = Math.round(lineAt(hu, x + 0.5) - 0.5);
                if (y >= 0 && y < H) px[y * W + x] = 0;
            }
        }
        /* a shine across the lower lip's fullest part: the lip's middle third, a little below its
         * centre line */
        let lc = outer[0], rc = outer[0];
        for (const p of outer) { if (p[0] < lc[0]) lc = p; if (p[0] > rc[0]) rc = p; }
        const cx = (lc[0] + rc[0]) / 2, half = (rc[0] - lc[0]) / 2;
        for (let x = Math.round(cx - half * 0.32); x <= Math.round(cx + half * 0.32); x++) {
            let bot = -1;
            for (let y = H - 1; y >= 0; y--) if (reg[y * W + x] === 1) { bot = y; break; }
            const inner = lineAt(hl, x + 0.5);
            if (bot < 0 || bot - inner < 4) continue;
            const y = Math.round(inner + (bot - inner) * 0.55);
            px[y * W + x] = 0;
        }

        /* ---- inside. Upper teeth hang from the upper lip as one even row of blocks (5px teeth, a
         * 1px gap, the front pair either side of the middle). A narrow opening on a spread mouth (Ee)
         * shows both rows meeting: teeth fill it, split by the bite line. Rounded mouths show fewer
         * teeth. The tongue is a stippled mound on the floor, only where there is room for it. */
        if (isOpen) {
            const h0 = hu[0][0], h1 = hu[hu.length - 1][0], hc = (h0 + h1) / 2, hw = Math.max(1, (h1 - h0) / 2);
            const reach = 0.85 - 0.3 * round;
            const toothH = Math.min(4, Math.max(2, holeH * 0.3));
            const clenched = holeH < 7 && round < 0.5;
            for (let x = Math.ceil(h0); x <= Math.floor(h1); x++) {
                const t = (x + 0.5 - hc) / hw;
                const top = lineAt(hu, x + 0.5), bot = lineAt(hl, x + 0.5), gap = bot - top;
                if (gap < 1.5 || Math.abs(t) > reach) continue;
                const gapCol = (((Math.round(x - hc) % 6) + 6) % 6) === 0;
                const set = (y) => { if (y >= 0 && y < H && reg[y * W + x] === 3) px[y * W + x] = 1; };
                if (clenched) {
                    /* a dark rim against each lip, the teeth between */
                    if (!gapCol) for (let y = Math.ceil(top) + 1; y < Math.floor(bot) - 1; y++) set(y);
                    continue;
                }
                if (round > 0.75 && holeH < 9) continue;     /* a small rounded opening: dark inside */
                if (!gapCol) for (let y = Math.ceil(top) + 1; y < top + 1 + toothH; y++) set(y);
                const room = gap - toothH - 4;
                if (room > 3 && Math.abs(t) < 0.8) {
                    const wdt = 0.5 + front * 0.3;
                    const h = Math.min(room, gap * (0.15 + high * 0.35)) * Math.max(0, 1 - (t / wdt) ** 2);
                    for (let y = Math.floor(bot) - 1; y > bot - 1 - h; y--)
                        if (y >= 0 && reg[y * W + x] === 3 && ((x + y) & 1) === 0) px[y * W + x] = 1;
                }
            }
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
        ctx.print(W - ctx.textWidth(nm) - 1, H - FONT_H - 1, nm, 1);
    }

    globalThis.canvas_overlay = {
        drawPage(ctx, o) {
            const values = o.values || {};
            const now = typeof o.nowMs === "number" ? o.nowMs : Date.now();
            if (o.key === "line") return drawLine(ctx, values.chm_view, now);
            if (o.key === "sung") return drawLyrics(ctx, values.chm_view, values.position);
            if (o.key === "mouth") {
                const s = parseFloat(values.sweep);
                return drawMouthFront(ctx, ease(onPath(isFinite(s) ? s : 0.5), now), VOWELS, FONT_H);
            }
            if (o.key === "shape") return drawMouthFront(ctx, ease(values, now), VOWELS, FONT_H);
        },
    };
})();

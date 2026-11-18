/*
 * CHIMERA Vowel page: the mouth in THREE-QUARTER view (design draft).
 *
 * Drawn as a close crop of a lower face turned to the viewer's right, after the 3/4 column of
 * Meta's viseme reference (photos in this folder): the top edge cuts the nose, the bottom the
 * chin; the profile runs down the right side (nose, philtrum, lips, chin) and the near half of
 * the mouth fills the middle. That context is what makes it read as a turned head.
 *
 * Articulated from the Mouth knobs:
 *   Jaw      the lower jaw swings down (lower lip, chin), more at the front than the corner
 *   Lips     parts the lips
 *   Round    draws the corner forward and thickens the lips into a pucker
 *   Pout     pushes the lips out past the profile
 *   Front    where the tongue's crest sits: near the teeth (front) or deeper in
 *   Height   how high the tongue rises
 *
 * Band coordinates (the page body, 128 x ~46), y down. Shapes are filled polygons (lips,
 * teeth) and 1px lines (profile, folds), built from control points smoothed by Catmull-Rom.
 */
function drawFace34(ctx, values, VOWELS, FONT_H) {
    const g = (k, d) => {
        const x = parseFloat(values[k]);
        return isFinite(x) ? Math.max(0, Math.min(1, x)) : d;
    };
    const front = g("tongue_x", 0.5), high = g("tongue_y", 0.5), jaw = g("jaw", 0.3);
    const lips = g("lips", 0.5), round = g("round", 0), pout = g("protrude", 0);
    const W = ctx.width, H = ctx.height;
    const px = new Uint8Array(W * H);
    const put = (x, y, c) => {
        x = Math.round(x); y = Math.round(y);
        if (x >= 0 && y >= 0 && x < W && y < H) px[y * W + x] = c;
    };

    /* ---- smoothing and drawing helpers */
    const spline = (pts, per = 6) => {
        if (pts.length < 3) return pts.slice();
        const out = [];
        for (let i = 0; i < pts.length - 1; i++) {
            const p0 = pts[Math.max(0, i - 1)], p1 = pts[i], p2 = pts[i + 1], p3 = pts[Math.min(pts.length - 1, i + 2)];
            for (let k = 0; k < per; k++) {
                const t = k / per, t2 = t * t, t3 = t2 * t;
                const f = (a, b, c, d) => 0.5 * (2 * b + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t2 + (-a + 3 * b - 3 * c + d) * t3);
                out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
            }
        }
        out.push(pts[pts.length - 1]);
        return out;
    };
    const line = (x0, y0, x1, y1, c = 1) => {
        const n = Math.max(1, Math.ceil(Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0))));
        for (let i = 0; i <= n; i++) put(x0 + ((x1 - x0) * i) / n, y0 + ((y1 - y0) * i) / n, c);
    };
    const poly = (pts, c = 1) => { for (let i = 1; i < pts.length; i++) line(pts[i - 1][0], pts[i - 1][1], pts[i][0], pts[i][1], c); };
    const dotted = (pts, every = 2) => {
        let k = 0;
        for (let i = 1; i < pts.length; i++) {
            const [x0, y0] = pts[i - 1], [x1, y1] = pts[i];
            const n = Math.max(1, Math.ceil(Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0))));
            for (let j = 0; j < n; j++, k++) if (k % every === 0) put(x0 + ((x1 - x0) * j) / n, y0 + ((y1 - y0) * j) / n, 1);
        }
    };
    /* even-odd scanline fill; `mask` (optional) limits where it may paint */
    const fill = (pts, c = 1, mask) => {
        let y0 = Infinity, y1 = -Infinity;
        for (const p of pts) { y0 = Math.min(y0, p[1]); y1 = Math.max(y1, p[1]); }
        for (let y = Math.max(0, Math.ceil(y0)); y <= Math.min(H - 1, Math.floor(y1)); y++) {
            const xs = [];
            for (let i = 0; i < pts.length; i++) {
                const a = pts[i], b = pts[(i + 1) % pts.length];
                if ((a[1] <= y && b[1] > y) || (b[1] <= y && a[1] > y)) xs.push(a[0] + ((y - a[1]) * (b[0] - a[0])) / (b[1] - a[1]));
            }
            xs.sort((p, q) => p - q);
            for (let k = 0; k + 1 < xs.length; k += 2)
                for (let x = Math.max(0, Math.ceil(xs[k])); x <= Math.min(W - 1, Math.floor(xs[k + 1])); x++)
                    if (!mask || mask[y * W + x]) px[y * W + x] = c;
        }
    };

    /* ---- the pose */
    const J = jaw * 13;                       /* jaw drop at the front of the chin (px) */
    const P = Math.max(pout, round * 0.7) * 6; /* how far the lips stand out past the profile */
    const lipGap = lips * 2.5;                /* the lips' own parting, added to the jaw's */
    /* the mandible swings from far off to the left: a point drops by more the further forward */
    const drop = (x) => J * Math.max(0, Math.min(1, (x - 34) / 74));

    /* the mouth's front (where the lips meet at the profile) and its near corner */
    const fx = 108 + P * 0.6;
    const cornerX = 50 + round * 30 + pout * 6;
    const cornerY = 21 + drop(cornerX) * 0.45;
    const thick = 1 + round * 0.45;
    /* the mouth line, from the corner to the front; the opening is split about it */
    const mouthY = (t) => cornerY + (19 - cornerY) * t - 0.6 * Math.sin(Math.PI * t);
    const X = (t) => cornerX + (fx - cornerX) * t;
    /* how open the mouth is at t (0 corner .. 1 front): a wedge from the corner, rounder when
     * puckered */
    const openAt = (t) => {
        const h = J * 1.0 + lipGap;
        const wedge = Math.pow(t, 0.55 + 0.6 * (1 - round));
        return h * (wedge * (1 - round) + Math.sin(Math.PI * Math.min(1, t * 1.05)) ** 0.6 * round);
    };
    const upperIn = (t) => mouthY(t) - openAt(t) * 0.35;
    const lowerIn = (t) => mouthY(t) + openAt(t) * 0.7;

    const T = 24, ts = [];
    for (let i = 0; i <= T; i++) ts.push(i / T);

    /* the upper lip: from its lower edge up to the vermilion border, which peaks in the bow
     * just behind the front */
    /* the lips run out to the profile line at the front */
    const reach = (t) => (t > 0.9 ? (t - 0.9) * 20 : 0);
    const upperTop = ts.map((t) => {
        const bow = 1.1 * Math.exp(-(((t - 0.86) / 0.07) ** 2)) - 0.5 * Math.exp(-(((t - 0.95) / 0.04) ** 2));
        return [X(t) + P * 0.3 * t + reach(t), upperIn(t) - (4.2 * thick) * Math.pow(Math.sin(Math.PI * Math.min(1, t * 0.98 + 0.02)), 0.5) - bow * thick];
    });
    const upperBot = ts.map((t) => [X(t) + P * 0.3 * t + reach(t) * 0.5, upperIn(t)]);
    /* the lower lip: fuller, and carried down with the jaw */
    const lowerTop = ts.map((t) => [X(t) + P * 0.25 * t + reach(t) * 0.5, lowerIn(t)]);
    const lowerBot = ts.map((t) => [X(t) - 2 * t + P * 0.2 * t + reach(t) * 0.6, lowerIn(t) + (6 * thick) * Math.pow(Math.sin(Math.PI * Math.min(1, t * 0.96 + 0.04)), 0.55)]);

    /* ---- the face around it: the profile down the right side, the nose above */
    const prof = spline([
        [127, -2], [121, 2], [112, 5], [107, 7],                 /* nose tip underside to the base */
        [107, 10], [109 + P * 0.5, 12.5 - 0], [fx + 2, upperIn(1) - 3], [fx + 1, upperIn(1)],  /* philtrum, upper lip */
    ], 6);
    const lowLip = spline([
        [fx + 1, lowerIn(1)], [fx + 3 + P * 0.2, lowerIn(1) + 4 * thick], [fx - 1, lowerIn(1) + 8 * thick],
        [102 - P * 0.2, lowerIn(1) + 10 * thick], [100, lowerIn(1) + 13 * thick], [104, 39 + drop(104)], [105, 44 + drop(105)], [103, 50 + drop(103)],
    ], 6);
    /* the nose's near wing and nostril */
    const ala = spline([[88, -1], [89, 3], [94, 6], [101, 6.5]], 5);
    const nostril = spline([[96, 4], [100, 4.6], [104, 4.4]], 4);
    /* the fold from the nose wing down past the mouth corner, and the chin's near edge */
    const fold = spline([[86, 4], [76, 11], [cornerX - 6, cornerY + 1], [cornerX - 9, cornerY + 9]], 6);
    const chin = spline([[101, 34 + drop(101)], [92, 39 + drop(92)], [82, 44 + drop(82)], [74, 50 + drop(74)]], 6);

    /* ---- inside the mouth: dark, with the upper teeth at the front and the tongue behind */
    const hole = [...upperBot, ...lowerTop.slice().reverse()];
    const holeMask = new Uint8Array(W * H);
    {
        const keep = px.slice();
        fill(hole, 1);
        for (let i = 0; i < px.length; i++) { holeMask[i] = px[i]; px[i] = keep[i]; }
    }
    const opening = openAt(0.8);
    if (opening > 1.6) {
        /* upper teeth: a row hung from the upper lip, the front ones broad, narrowing as they
         * go back toward the corner; hidden behind a pucker */
        if (round < 0.55) {
            let x = fx - 2;
            for (let k = 0; k < 7; k++) {
                const w = Math.max(1.5, 4.2 - k * 0.45);
                const t0 = (x - w - cornerX) / (fx - cornerX), t1 = (x - cornerX) / (fx - cornerX);
                if (t0 < 0.15) break;
                const top = Math.min(upperIn(t0), upperIn(t1)) - 1;
                const h = Math.min(3.6, opening * 0.42) * (1 - k * 0.06);
                fill([[x - w + 0.6, top], [x - 0.6, top], [x - 0.6, upperIn(t1) + h], [x - w + 0.6, upperIn(t0) + h]], 1, holeMask);
                x -= w;
            }
        }
        /* the tongue: a mound on the floor of the mouth; its crest near the teeth when it is
         * forward, deeper in when it is back; higher or lower */
        const crestT = 0.32 + front * 0.5;
        for (let x = Math.floor(cornerX); x <= Math.ceil(fx); x++) {
            const t = (x - cornerX) / (fx - cornerX);
            const room = lowerIn(t) - upperIn(t);
            if (room < 2) continue;
            const hgt = room * (0.12 + high * 0.6) * Math.exp(-(((t - crestT) / 0.24) ** 2));
            const top = Math.round(lowerIn(t) - hgt);
            for (let y = top; y <= lowerIn(t) + 1; y++) {
                if (y < 0 || y >= H || !holeMask[y * W + x]) continue;
                if (y === top || ((x + y) & 1) === 0) put(x, y, 1);
            }
        }
    } else {
        /* closed: the seam between the lips is the darkest line */
    }

    /* ---- the lips, over the inside */
    fill([...upperTop, ...upperBot.slice().reverse()], 1);
    fill([...lowerTop, ...lowerBot.slice().reverse()], 1);
    /* the seam where they meet, when closed */
    if (opening <= 1.6) poly(ts.map((t) => [X(t) + P * 0.28 * t, mouthY(t)]), 0);
    /* light on the lower lip: a short dark shine line across its fullest part */
    poly(ts.filter((t) => t > 0.45 && t < 0.85).map((t) => [X(t) - 1.5 * t + P * 0.2 * t, lowerIn(t) + 2.6 * thick]), 0);

    /* ---- the face lines over everything */
    poly(prof);
    poly(lowLip);
    poly(ala);
    poly(nostril);
    dotted(fold, 2);
    dotted(chin, 2);

    /* to the screen in runs */
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
    /* the vowel it is closest to, top left, out of the drawing's way */
    let nm = "", best = 1e9;
    for (const [n, f, h] of VOWELS) {
        const d = (f - front) ** 2 + (h - high) ** 2;
        if (d < best) { best = d; nm = n; }
    }
    ctx.print(1, 1, nm, 1);
}

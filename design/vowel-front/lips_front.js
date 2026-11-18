/*
 * CHIMERA Vowel page: the FRONT-VIEW lips close-up (Josh: "MUCH better", 2026-11-18).
 * Kept as the fallback while a three-quarter view is designed. Self-contained: needs VOWELS
 * (below) and FONT_H; drawn with ctx.fillRect/print/textWidth, as an as_page drawPage body.
 */
const FONT_H = 7;
const VOWELS = [
    ["Ah", 0.27, 0.20], ["Ee", 0.71, 0.67], ["Ih", 0.69, 0.60], ["Ay", 0.68, 0.55],
    ["Eh", 0.63, 0.40], ["Ae", 0.62, 0.28], ["Aw", 0.13, 0.30], ["Oh", 0.14, 0.41],
    ["Uh", 0.25, 0.33], ["Oo", 0.37, 0.60], ["Book", 0.35, 0.53], ["Er", 0.45, 0.56],
    ["Schwa", 0.51, 0.43],
];
    /*
     * Vowel: a close-up of the mouth, drawn from the Mouth knobs (no engine data, no lag).
     *   Jaw, Lips      open the mouth; a wide-open one shows the upper teeth, a very wide one
     *                  the lower teeth too
     *   Front, Height  the tongue, seen through the opening: it rises behind the teeth for Ee,
     *                  lies low for Ah
     *   Round, Pout    draw the lips in and thicken them into a pucker for Oo
     * The vowel the shape is closest to is named in the corner.
     *
     * Drawn into a bitmap first (the shapes overlap and cut into each other), then sent to the
     * screen as horizontal runs.
     */
    function drawLips(ctx, values) {
        const g = (k, d) => {
            const x = parseFloat(values[k]);
            return isFinite(x) ? Math.max(0, Math.min(1, x)) : d;
        };
        const front = g("tongue_x", 0.5), high = g("tongue_y", 0.5), jaw = g("jaw", 0.3);
        const lips = g("lips", 0.5), round = Math.max(g("round", 0), g("protrude", 0) * 0.85);
        const W = ctx.width, H = ctx.height;
        const px = new Uint8Array(W * H);
        const set = (x, y, c) => {
            x = Math.round(x); y = Math.round(y);
            if (x >= 0 && y >= 0 && x < W && y < H) px[y * W + x] = c;
        };

        const cx = W >> 1, cy = 20;
        /* half-width of the mouth: wide and spread, or drawn in by rounding */
        const hw = 40 - round * 24;
        /* half-height of the opening */
        let oh = 0.4 + jaw * 12 + lips * 2.5;
        /* a pucker leaves a small round hole */
        oh += (hw * 0.36 + jaw * 4 - oh) * round;
        /* lip thickness, upper and lower: a pucker is fuller */
        const tu = 4 + round * 2, tl = 5 + round * 3;

        /* the opening's edges and the lips' outer edges, as functions of u in [-1, 1] */
        const open = (u) => Math.pow(Math.max(0, 1 - u * u), 0.6 + round * 0.3);
        const bow = (u) => 1.6 * Math.exp(-(((Math.abs(u) - 0.2) / 0.11) ** 2)) - 0.6 * Math.exp(-((u / 0.07) ** 2));
        const innerTop = (u) => cy - oh * open(u);
        const innerBot = (u) => cy + oh * open(u) * 1.08;
        const lipShape = (u) => Math.pow(Math.max(0, 1 - u * u), 0.5);
        const outerTop = (u) => innerTop(u) - tu * lipShape(u) - bow(u) * lipShape(u);
        const outerBot = (u) => innerBot(u) + tl * lipShape(u);

        for (let x = Math.floor(cx - hw - 2); x <= Math.ceil(cx + hw + 2); x++) {
            const u = (x - cx) / hw;
            if (Math.abs(u) > 1) continue;
            const it = innerTop(u), ib = innerBot(u), ot = outerTop(u), ob = outerBot(u);
            /* the lips, solid */
            for (let y = Math.floor(ot); y <= Math.ceil(ob); y++) if (y < it || y > ib) set(x, y, 1);
            /* the opening: dark, with teeth and tongue in it */
            const span = ib - it;
            if (span >= 2) {
                /* upper teeth hang from just inside the upper lip, a gap between each */
                const teethH = Math.min(4, span * 0.3);
                /* 6px teeth with a 1px gap; the gap column is the one at 0 mod 6 from the middle */
                const gap = ((Math.round(x - cx) % 6) + 6) % 6 === 3;
                const showTeeth = round < 0.5 && Math.abs(u) < 0.78 && span > 4;
                if (showTeeth)
                    for (let y = Math.ceil(it) + 1; y <= it + 1 + teethH; y++) if (!gap) set(x, y, 1);
                /* the tongue: a mound from the bottom of the opening, higher for a high tongue,
                 * fuller across the front for a front tongue; drawn in a light stipple */
                const tw = 0.55 + front * 0.35;
                const th = span * (0.15 + high * 0.6) * Math.max(0, 1 - (u / tw) ** 2);
                for (let y = Math.floor(ib) - 1; y > ib - th; y--)
                    if (((x + y) & 1) === 0 && y > it + 1) set(x, y, 1);
            } else {
                /* closed: the seam between the lips */
                set(x, cy, 0);
            }
        }
        /* a shine across the lower lip */
        for (let x = Math.floor(cx - hw * 0.35); x <= cx + hw * 0.35; x++) {
            const u = (x - cx) / hw;
            set(x, Math.round(outerBot(u) - tl * 0.45), 0);
        }
        /* to the screen, row by row in runs */
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

        let near = "", best = 1e9;
        for (const [nm, f, h] of VOWELS) {
            const d = (f - front) ** 2 + (h - high) ** 2;
            if (d < best) { best = d; near = nm; }
        }
        ctx.print(W - ctx.textWidth(near) - 1, H - FONT_H - 1, near, 1);
    }


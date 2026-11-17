/* What each knob cell DRAWS on the device, per mode, using a host's own param_pages code:
 * the page plan (visible_if evaluated for each Mode), each cell's label as the grid shortens it,
 * and its widget. Also runs the host's contract validator.
 *
 *   node tools/cells.mjs [param_pages dir]     (default build/stock_shared/param_pages)
 *   The device's files: scp -r ableton@move.local:/data/UserData/schwung/shared/param_pages build/stock_shared/
 * Reads build/contract.json, written by tools/gen.mjs.
 */
import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

const S = resolve(process.argv[2] || "build/stock_shared/param_pages") + "/";
const { planPages } = await import(S + "page_plan.mjs");
const { buildMetaIndex } = await import(S + "param_meta.mjs");
const R = await import(S + "render_page_movy.mjs");
const { ui_hierarchy: hierarchy, chain_params: chainParams } = JSON.parse(readFileSync("build/contract.json", "utf8"));
const P = JSON.parse(readFileSync("src/params.json", "utf8"));
const metaIndex = buildMetaIndex({ hierarchy, chainParams });

if (existsSync(S + "validate_contract.mjs")) {
    const { validateContract } = await import(S + "validate_contract.mjs");
    const { findings } = validateContract({ id: "chimera", hierarchy, chainParams, capabilities: {} });
    for (const f of findings) console.log(`[${f.level}] ${f.rule}: ${f.message}`);
    if (!findings.length) console.log("validate_contract: no findings");
}

let bad = 0;
P.modes.forEach((mode, m) => {
    const visible = (c) => {
        if (!c || c.param !== "mode") return true;
        if (c.equals !== undefined) return m === c.equals;
        if (c.not_equals !== undefined) return m !== c.not_equals;
        if (c.lt !== undefined) return m < c.lt;
        if (c.gt !== undefined) return m > c.gt;
        return true;
    };
    const plan = planPages({ hierarchy, chainParams, visible });
    console.log(`\n== Mode ${mode}`);
    for (const p of plan.pages || plan) {
        if (p.kind !== "knobs") {
            console.log(`   ${String(p.name).padEnd(12)} (${p.kind})`);
            continue;
        }
        const cells = (p.keys || []).map((k) => {
            if (!k) return "--";
            const meta = metaIndex.get(k);
            // as render_page_movy draws it: a declared short_name verbatim, else the name squeezed
            const label = meta?.short_name ? R.labelVerbatim(meta.short_name) : R.labelForCell(meta?.label || meta?.name || k);
            const w = R.widgetKindFor ? R.widgetKindFor(meta) : "?";
            if (!meta) bad++;
            return `${label}[${w}]`;
        });
        console.log(`   ${String(p.name).padEnd(12)} ${cells.join("  ")}`);
    }
});
if (bad) { console.error(`${bad} cells have no metadata`); process.exit(1); }

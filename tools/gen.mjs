/* src/params.json -> everything that must agree with it:
 *
 *   src/module.json                              base + chain_params (the host's metadata table)
 *   src/dsp/params_gen.h                         the plugin's param table, presets, words,
 *                                                and the served ui_hierarchy / chain_params
 *   engine/crates/chimera-move/src/params_gen.rs ids + how each param drives the engine
 *
 *   node tools/gen.mjs        (scripts/build.sh runs it on the host side; Docker has no node)
 *
 * WHY module.json carries chain_params: the host builds a synth slot's parameter table
 * (modulation targets, patch apply, smoothing) from module.json ON DISK at load, not from the
 * plugin. Its reader finds each entry by its first '}', so entries are flat (no visible_if,
 * no nested objects), and a "ui_hierarchy" string anywhere in the file would be taken as an
 * inline hierarchy. A string param has no C type there and is left out (it rides in the state).
 */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..");
const P = JSON.parse(readFileSync(join(repo, "src/params.json"), "utf8"));
const base = JSON.parse(readFileSync(join(repo, "src/module.base.json"), "utf8"));
const fail = (m) => { console.error("gen: " + m); process.exit(1); };

const byKey = new Map();
P.params.forEach((p, i) => {
  if (byKey.has(p.key)) fail(`duplicate key ${p.key}`);
  if (p.key.length > 31 || p.name.length > 31) fail(`${p.key}: over 31 chars`);
  if (/(^|_)(bpm|tempo|steps?|pulses?|rotation|count|voices|polyphony|divisions?)($|_)/.test(p.key)) fail(`${p.key}: the host draws this key as a big number`);
  if (p.type === "enum" && p.short_options && p.short_options.length !== p.options.length) fail(`${p.key}: short_options length`);
  byKey.set(p.key, { ...p, id: i });
});
const keyOf = (e) => (typeof e === "string" ? e : e.key);
const seen = new Set();
for (const pg of P.pages) {
  if (pg.keys.length > 8) fail(`page ${pg.level}: more than 8 knobs`);
  for (const e of pg.keys) {
    const k = keyOf(e);
    if (!byKey.has(k)) fail(`page ${pg.level}: unknown key ${k}`);
    if (seen.has(k)) fail(`${k} on two pages (a repeated hierarchy key drops the slot's metadata)`);
    seen.add(k);
  }
}
for (const p of P.params) if (!seen.has(p.key)) fail(`${p.key} is on no page`);

// ---- chain_params entries
function entry(p, flat) {
  const e = { key: p.key, name: p.name };
  if (p.short) e.short_name = p.short;
  if (p.type === "enum") {
    e.type = "enum";
    e.options = p.options;
    if (p.short_options) e.short_options = p.short_options;
  } else if (p.type === "string") {
    e.type = "string";
  } else {
    e.type = "float";
    e.min = p.min;
    e.max = p.max;
    e.step = p.step;
    if (p.unit) e.unit = p.unit;
  }
  if (p.access) e.access = p.access;
  if (p.type !== "string") e.default = p.default;
  if (!flat && p.type === "string") e.default = p.default;
  return e;
}
const moduleChain = P.params.filter((p) => p.type !== "string").map((p) => entry(p, true));
const servedChain = P.params.map((p) => entry(p, false));

// ---- ui_hierarchy: root is the preset browser, every page a child level
const levels = {
  root: {
    name: base.name, label: base.name,
    list_param: "preset", count_param: "preset_count", name_param: "preset_name",
    params: P.pages.map((pg) => ({ level: pg.level, label: pg.label })),
  },
};
for (const pg of P.pages) {
  const l = {
    name: pg.label, label: pg.label,
    knobs: pg.keys.map(keyOf),
    params: pg.keys.map((e) => (typeof e === "string" ? e : { key: e.key, visible_if: e.visible_if })),
  };
  if (pg.visible_if) l.visible_if = pg.visible_if;
  levels[pg.level] = l;
}
const hierarchy = { modes: null, levels };

// ---- presets: values by option NAME in the table, indices on the wire
const words = P.words;
const optIndex = (p, v) => {
  if (typeof v === "number") return v;
  const i = p.options.indexOf(v);
  if (i < 0) fail(`preset value ${v} is not an option of ${p.key}`);
  return i;
};
const presets = P.presets.map((pr) => {
  const vals = P.params.map((p) => (p.type === "string" ? null : p.default));
  let lyrics = byKey.get("lyrics").default;
  for (const [k, v] of Object.entries(pr.values)) {
    const p = byKey.get(k);
    if (!p) fail(`preset ${pr.name}: unknown key ${k}`);
    if (p.type === "string") { lyrics = v; continue; }
    vals[p.id] = p.type === "enum" ? optIndex(p, v) : v;
  }
  const w = byKey.get("words");
  const wname = w.options[vals[w.id]];
  if (wname !== "Custom") {
    if (!(wname in words)) fail(`no words for ${wname}`);
    lyrics = words[wname];
  }
  return { name: pr.name, vals, lyrics };
});
for (const o of byKey.get("words").options) if (o !== "Custom" && !(o in words)) fail(`no words for ${o}`);

// ---- C header
const cs = (s) => JSON.stringify(s); // a JSON string is a valid C string literal for our ASCII/UTF-8 text
const num = (x) => (Number.isInteger(x) ? x.toFixed(1) : String(x));
const TYPE = { float: "CP_FLOAT", enum: "CP_ENUM", string: "CP_STRING" };
let h = `/* GENERATED by tools/gen.mjs from src/params.json. Do not edit. */
#pragma once
#include <stddef.h>

typedef enum { CP_FLOAT, CP_ENUM, CP_STRING } cp_type_t;
typedef struct {
    const char *key;
    cp_type_t type;
    float min, max, def;
    int n_options;
    const char *const *options;
    int trigger; /* access: write */
} cp_param_t;

`;
P.params.forEach((p) => {
  if (p.type === "enum") h += `static const char *const cp_opts_${p.key}[] = {${p.options.map(cs).join(", ")}};\n`;
});
h += `\nenum {\n${P.params.map((p, i) => `    CP_${p.key.toUpperCase()} = ${i},`).join("\n")}\n    CP_COUNT = ${P.params.length}\n};\n\n`;
h += `static const cp_param_t cp_params[CP_COUNT] = {\n`;
for (const p of P.params) {
  const isEnum = p.type === "enum";
  const min = isEnum ? 0 : p.type === "string" ? 0 : p.min;
  const max = isEnum ? p.options.length - 1 : p.type === "string" ? 0 : p.max;
  const def = p.type === "string" ? 0 : p.default;
  h += `    {${cs(p.key)}, ${TYPE[p.type]}, ${num(min)}f, ${num(max)}f, ${num(def)}f, ${isEnum ? p.options.length : 0}, ${isEnum ? `cp_opts_${p.key}` : "NULL"}, ${p.access === "write" ? 1 : 0}},\n`;
}
h += `};\n\n`;
h += `#define CP_LYRICS_DEFAULT ${cs(byKey.get("lyrics").default)}\n`;
h += `/* the lyrics a Words option stands for (NULL: Custom, keep the typed lyrics) */\n`;
h += `static const char *const cp_words_text[] = {${byKey.get("words").options.map((o) => (o === "Custom" ? "NULL" : cs(words[o]))).join(", ")}};\n\n`;
h += `typedef struct { const char *name; float vals[CP_COUNT]; const char *lyrics; } cp_preset_t;\n`;
h += `#define CP_PRESET_COUNT ${presets.length}\n`;
h += `static const cp_preset_t cp_presets[CP_PRESET_COUNT] = {\n`;
for (const pr of presets) h += `    {${cs(pr.name)}, {${pr.vals.map((v) => num(v ?? 0) + "f").join(", ")}}, ${cs(pr.lyrics)}},\n`;
h += `};\n\n`;
h += `static const char cp_ui_hierarchy[] = ${cs(JSON.stringify(hierarchy))};\n`;
h += `static const char cp_chain_params[] = ${cs(JSON.stringify(servedChain))};\n`;
writeFileSync(join(repo, "src/dsp/params_gen.h"), h);

// ---- Rust: ids and how each drives the engine
const ARTIC = ["jaw", "tbX", "tbY", "ttX", "ttY", "ttCurl", "root", "lipAp", "lipProt", "lipWidth", "labiodental", "velum", "larynx", "epilarynx", "ventricular", "lateral", "dorsum"];
const SOURCE = { pressure: "Pressure", tension: "Tension", vibrato_rate: "VibratoRate", glide: "Glide", brightness: "Brightness", breathiness: "Breathiness", roughness: "Roughness", fry: "Fry", drift: "Drift" };
let r = `// GENERATED by tools/gen.mjs from src/params.json. Do not edit.\n#![allow(dead_code)]\n\n`;
r += `pub const N_PARAMS: usize = ${P.params.length};\n`;
P.params.forEach((p, i) => (r += `pub const P_${p.key.toUpperCase()}: usize = ${i};\n`));
r += `\n#[derive(Clone, Copy, Debug, PartialEq)]\npub enum Src { ${Object.values(SOURCE).join(", ")} }\n\n`;
r += `/// What a param drives in the engine.\n#[derive(Clone, Copy, Debug, PartialEq)]\npub enum Drive {\n    /// handled by name in the controller\n    Own,\n    /// a voice-source field, in the param's own units (ms for glide)\n    Source(Src),\n    /// an articulator (ArticKey index): engine value = lo + v * (hi - lo)\n    Artic(usize, f64, f64),\n    /// a creature's parameter: (mode index, creature id, param key)\n    Creature(usize, &'static str, &'static str),\n}\n\n`;
r += `pub const DEFAULTS: [f64; N_PARAMS] = [${P.params.map((p) => num(p.type === "string" ? 0 : p.default)).join(", ")}];\n`;
r += `pub const DRIVES: [Drive; N_PARAMS] = [\n`;
for (const p of P.params) {
  let d = "Drive::Own";
  if (p.source) {
    if (!SOURCE[p.source]) fail(`${p.key}: unknown source ${p.source}`);
    d = `Drive::Source(Src::${SOURCE[p.source]})`;
  } else if (p.artic) {
    const ai = ARTIC.indexOf(p.artic[0]);
    if (ai < 0) fail(`${p.key}: unknown articulator ${p.artic[0]}`);
    d = `Drive::Artic(${ai}, ${num(p.artic[1])}, ${num(p.artic[2])})`;
  } else if (p.creature) {
    const mode = { songbird: 3, seal: 4, alligator: 5, elephant: 6 }[p.creature[0]];
    if (mode === undefined) fail(`${p.key}: unknown creature ${p.creature[0]}`);
    d = `Drive::Creature(${mode}, ${cs(p.creature[0])}, ${cs(p.creature[1])})`;
  }
  r += `    ${d}, // ${p.key}\n`;
}
r += `];\n\n`;
const vowel = byKey.get("vowel");
if (vowel.ipa.length !== vowel.options.length) fail("vowel ipa length");
r += `pub const VOWEL_IPA: [&str; ${vowel.ipa.length}] = [${vowel.ipa.map(cs).join(", ")}];\n`;
r += `/// per creature mode (3..=6): the Keys and Hold param ids\n`;
r += `pub const CREATURE_KEYS: [(usize, usize); 4] = [(P_SB_KEYS, P_SB_HOLD), (P_SE_KEYS, P_SE_HOLD), (P_GA_KEYS, P_GA_HOLD), (P_EL_KEYS, P_EL_HOLD)];\n`;
r += `pub const CREATURE_IDS: [&str; 4] = ["songbird", "seal", "alligator", "elephant"];\n`;
writeFileSync(join(repo, "engine/crates/chimera-move/src/params_gen.rs"), r);

// ---- what the plugin serves, for tools/cells.mjs (the host's own page planner)
mkdirSync(join(repo, "build"), { recursive: true });
writeFileSync(join(repo, "build/contract.json"), JSON.stringify({ ui_hierarchy: hierarchy, chain_params: servedChain }, null, 1));

// ---- module.json
base.capabilities.chain_params = moduleChain;
const text = JSON.stringify(base, null, 1) + "\n";
if (text.includes("ui_hierarchy")) fail("module.json must not mention ui_hierarchy");
if (text.length > 65536) fail(`module.json is ${text.length} bytes (host limit 65536)`);
for (const e of moduleChain) for (const v of Object.values(e)) if (v && typeof v === "object" && !Array.isArray(v)) fail(`${e.key}: nested object`);
writeFileSync(join(repo, "src/module.json"), text);
console.log(`gen: ${P.params.length} params, ${P.pages.length} pages, ${presets.length} presets; module.json ${text.length} bytes; ui_hierarchy ${JSON.stringify(hierarchy).length} bytes`);

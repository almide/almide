#!/usr/bin/env python3
"""The CLOSED port-vs-rebuild matrix: every self-host registry impl,
classified LINKED (which tier) / REJECTED (with the recorded reason) /
UNREACHED (no admission decision yet — honest walls). Generated, never
hand-sampled; regenerating after any whitelist change keeps it closed
by construction."""
import re, sys, collections

reg = open('crates/almide-types/src/self_host_registry.rs').read()
tbl = open('crates/almide-types/src/self_host_fn_tables.rs').read()
wl = open('crates/almide-wasm/src/whitelist.rs').read()
calls = open('crates/almide-wasm/src/calls.rs').read()

pairs = re.findall(r'\("([a-z0-9_]+)",\s*"([a-z0-9_.]+)"\)', reg) + \
        re.findall(r'\("([a-z0-9_]+)",\s*"([a-z0-9_.]+)"\)', tbl)
impls = {}
for a, b in pairs:
    impls.setdefault(a, b)

def const_names(src, name):
    m = re.search(name + r'[^=]*=\s*&\[(.*?)\];', src, re.S)
    return set(re.findall(r'"([a-z0-9_]+)"', m.group(1))) if m else set()

tiers = {
    'linked (calls.rs VERIFIED)': const_names(calls, 'const VERIFIED'),
    'linked (calls.rs SUM tier)': const_names(calls, 'const VERIFIED_SUM_BUILDERS'),
    'linked (sized-convert)': const_names(wl, 'SIZED_CONVERT_VERIFIED'),
    'linked (sized-convert SUM)': const_names(wl, 'SIZED_CONVERT_SUM_BUILDERS'),
    'linked (scalar/text)': const_names(wl, 'SCALAR_TEXT_VERIFIED'),
    'linked (scalar/text SUM)': const_names(wl, 'SCALAR_TEXT_SUM_BUILDERS'),
    'linked (libm)': const_names(wl, 'MATH_VERIFIED'),
    'linked (codec)': const_names(wl, 'CODEC_ENCODE_VERIFIED'),
    'linked (bytes family)': const_names(wl, 'BYTES_FAMILY_VERIFIED'),
    'linked (bytes family SUM)': const_names(wl, 'BYTES_FAMILY_SUM'),
}
# The rejections recorded in code comments (the executable decisions).
rejected = {
    'string_from_bytes': 'raw list-header read (len=count vs bytes) — from_bytes composes from_list + linked lossy instead',
    'value_eq': 'incumbent len-as-tag Value layout — native helper $value_eq instead',
    'value_merge': 'incumbent len-as-tag Value layout — native helper $value_merge instead',
    'value_pick': 'raw Value internals',
    'result_partition': 'raw list/tuple internals (load_str/store_str + list header reads)',
    'json_path_set': 'incumbent inline-pairs Value layout (tag@h+4, count@h+8) — native helper $jp_set instead',
    'json_path_remove': 'incumbent inline-pairs Value layout — native helper $jp_remove instead',
    'json_stringify_pretty': 'incumbent len-as-tag Value layout — native helper $vjson_pretty instead',
    'bytes_read_length_prefixed_strings_le': '8-byte List[String] slot stores (store_str at i*8) — native decoder instead',
}

linked = {}
for tier, names in tiers.items():
    for n in names:
        linked[n] = tier

rows = []
for a in sorted(impls):
    if a in linked:
        rows.append((a, impls[a], linked[a]))
    elif a in rejected:
        rows.append((a, impls[a], 'REJECTED: ' + rejected[a]))
    else:
        rows.append((a, impls[a], 'unreached (honest wall / native arm covers the surface)'))

counts = collections.Counter('linked' if r[2].startswith('linked') else ('rejected' if r[2].startswith('REJECTED') else 'unreached') for r in rows)
out = ["# The Port Matrix (generated — scripts/gen-port-matrix.py)", "",
       f"Registry impls: {len(rows)} — linked {counts['linked']}, rejected {counts['rejected']}, unreached {counts['unreached']}.",
       "",
       "An `unreached` row is an impl no admission decision has touched:",
       "its surface either has a NATIVE arm in the emitter or stays an",
       "honest wall in the burn-up histogram. Nothing links silently.",
       "", "| impl | surface | decision |", "|---|---|---|"]
for a, b, d in rows:
    out.append(f"| {a} | {b} | {d} |")
print("\n".join(out))

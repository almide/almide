#!/usr/bin/env python3
"""Type x operation conformance-matrix generator (test-surface-25x tier 4).

Emits one minimal .almd per (shape, op) cell into --out, then the driver
(run.sh) ladders every cell and classifies CLEAN / WALLED. CLEAN cells are
committed into spec/wasm_cross as tm_<shape>_<op>.almd under the matrix
contract; walls stay an inventory (representative specimens are pinned in
proofs/wall-corpus). Regenerate after a wall graduates: the grid regrows.

Each shape provides code fragments; each op composes them into a complete
program with deterministic output. A cell whose composition is not
expressible for the shape is skipped explicitly (SKIP table), never
silently.
"""
import os
import sys

# ── shapes ──────────────────────────────────────────────────────────
# id -> dict with:
#   decl      : type declarations (or "")
#   val       : an expression building a sample value
#   val2      : a second, DIFFERENT value (for eq)
#   show(v)   : expression rendering v deterministically to a String
#   ann       : type annotation
SHAPES = {
    "int_list": dict(
        decl="", ann="List[Int]",
        val="[1, 2, 3]", val2="[1, 2]",
        show="list.join(list.map({v}, (x) => int.to_string(x)), \",\")",
    ),
    "str_list": dict(
        decl="", ann="List[String]",
        val='["a", "bb", "ccc"]', val2='["a"]',
        show='list.join({v}, "|")',
    ),
    "list_list_int": dict(
        decl="", ann="List[List[Int]]",
        val="[[1, 2], [], [3]]", val2="[[1]]",
        show='list.join(list.map({v}, (xs) => int.to_string(list.len(xs))), ",")',
    ),
    "opt_int": dict(
        decl="", ann="Int?",
        val="some(7)", val2="none",
        show='int.to_string({v} ?? (0 - 1))',
    ),
    "res_int": dict(
        decl="", ann="Result[Int, String]",
        val="ok(5)", val2='err("e")',
        show='match {v} { ok(n) => int.to_string(n), err(e) => e }',
    ),
    "list_opt": dict(
        decl="", ann="List[Int?]",
        val="[some(1), none, some(3)]", val2="[none]",
        show='int.to_string(list.fold({v}, 0, (a, o) => a + (o ?? 0)))',
    ),
    "map_int": dict(
        decl="", ann="Map[String, Int]",
        val='map.from_list([("a", 1), ("b", 2)])', val2='map.from_list([("a", 1)])',
        show='int.to_string(map.get_or({v}, "a", 0 - 1) * 10 + map.len({v}))',
    ),
    "tuple2": dict(
        decl="", ann="(Int, String)",
        val='(4, "x")', val2='(5, "y")',
        show='match {v} { (n, s) => int.to_string(n) + s }',
    ),
    "list_tuple": dict(
        decl="", ann="List[(Int, Int)]",
        val="[(1, 2), (3, 4)]", val2="[(1, 2)]",
        show='int.to_string(list.fold({v}, 0, (a, p) => a + match p { (x, y) => x + y }))',
    ),
    "record": dict(
        decl="type R = { x: Int, s: String }", ann="R",
        val='R { x: 3, s: "r" }', val2='R { x: 4, s: "q" }',
        show='int.to_string({v}.x) + {v}.s',
    ),
    "variant": dict(
        decl="type V = | A(Int) | B(String)", ann="V",
        val="A(9)", val2='B("z")',
        show='match {v} { A(n) => int.to_string(n), B(s) => s }',
    ),
    "list_variant": dict(
        decl="type V = | A(Int) | B(String)", ann="List[V]",
        val='[A(1), B("t"), A(2)]', val2="[A(1)]",
        show='list.join(list.map({v}, (e) => match e { A(n) => int.to_string(n), B(s) => s }), "/")',
    ),
    "tree": dict(
        decl="type T = | Leaf(Int) | Node(List[T])",
        ann="T",
        val="Node([Leaf(1), Node([Leaf(2)]), Leaf(3)])", val2="Leaf(0)",
        show="int.to_string(tsum({v}))",
        helpers=(
            "fn tsum(t: T) -> Int = match t {\n"
            "  Leaf(n) => n,\n"
            "  Node(kids) => list.fold(kids, 0, (a, k) => a + tsum(k)),\n"
            "}\n"
        ),
    ),
}

# ── ops ─────────────────────────────────────────────────────────────
def op_lit_print(s):
    return "  let v: {ann} = {val}\n  println({show_v})\n".format(
        ann=s["ann"], val=s["val"], show_v=s["show"].replace("{v}", "v"))

def op_fn_pass(s):
    return None  # composed at file level (needs a fn decl); see build_cell

def op_eq(s):
    return (
        "  let a: {ann} = {val}\n  let b: {ann} = {val}\n  let c: {ann} = {val2}\n"
        '  println(if a == b then "eq" else "ne")\n'
        '  println(if a == c then "eq" else "ne")\n'
    ).format(**s)

def op_in_list(s):
    return (
        "  let xs: List[{ann}] = [{val}, {val2}]\n"
        "  println(int.to_string(list.len(xs)))\n"
        "  let h = list.get(xs, 0)\n"
        "  println(match h {{ some(v) => {show_v}, none => \"-\" }})\n"
    ).format(ann=s["ann"], val=s["val"], val2=s["val2"],
             show_v=s["show"].replace("{v}", "v"))

def op_shadow_drop(s):
    return (
        "  var v: {ann} = {val}\n"
        "  v = {val2}\n"
        "  println({show_v})\n"
    ).format(ann=s["ann"], val=s["val"], val2=s["val2"],
             show_v=s["show"].replace("{v}", "v"))

def op_interp(s):
    return "  let v: {ann} = {val}\n  let msg = \"got=${{{show_v}}}\"\n  println(msg)\n".format(
        ann=s["ann"], val=s["val"], show_v=s["show"].replace("{v}", "v"))

OPS = {
    "lit_print": op_lit_print,
    "fn_pass": op_fn_pass,
    "eq": op_eq,
    "in_list": op_in_list,
    "shadow_drop": op_shadow_drop,
    "interp": op_interp,
}

# Cells that are not expressible / deliberately excluded, with reasons.
SKIP = {
    ("map_int", "eq"): "map equality is order-independent by contract; covered by edge_map_order",
    ("map_int", "in_list"): "List[Map] is a pinned wall (list_of_maps_loop_build)",
}

HEADER = (
    "// @contract: C-296\n"
    "// GENERATED by tools/typematrix/gen.py — the type x operation conformance\n"
    "// matrix (cell: {shape} x {op}). Regenerate rather than hand-edit; a cell\n"
    "// that stops lowering moves to the wall inventory in the same PR.\n"
)

def build_cell(shape_id, op_id):
    s = dict(SHAPES[shape_id])
    body_fn = OPS[op_id]
    decl = s.get("decl", "")
    helpers = s.get("helpers", "")
    if op_id == "fn_pass":
        prog = (
            "fn roundtrip(v: {ann}) -> {ann} = v\n\n"
            "fn main() -> Unit = {{\n"
            "  let v: {ann} = {val}\n"
            "  let w = roundtrip(v)\n"
            "  println({show_w})\n"
            "}}\n"
        ).format(ann=s["ann"], val=s["val"], show_w=s["show"].replace("{v}", "w"))
    else:
        body = body_fn(s)
        prog = "fn main() -> Unit = {\n" + body + "}\n"
    parts = [HEADER.format(shape=shape_id, op=op_id)]
    if decl:
        parts.append(decl + "\n\n")
    if helpers:
        parts.append(helpers + "\n")
    parts.append(prog)
    return "".join(parts)

def main():
    out = sys.argv[sys.argv.index("--out") + 1]
    os.makedirs(out, exist_ok=True)
    n, skipped = 0, 0
    for shape_id in SHAPES:
        for op_id in OPS:
            if (shape_id, op_id) in SKIP:
                skipped += 1
                continue
            src = build_cell(shape_id, op_id)
            with open(os.path.join(out, f"tm_{shape_id}_{op_id}.almd"), "w") as f:
                f.write(src)
            n += 1
    print(f"typematrix: {n} cell(s) emitted, {skipped} skipped (see SKIP table)")

if __name__ == "__main__":
    main()

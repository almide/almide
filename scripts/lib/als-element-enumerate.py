# Shared enumerator for the ALS syntax-element coverage gate (Stage 3's
# element→section direction). The syntax-element universe is the surface AST:
# every variant of ExprKind / Stmt / Decl in crates/almide-syntax/src/ast.rs.
# Enumerating from the AST (not from prose) is what makes the deficit
# unarguable — a new syntax element lands as a new variant here and the gate
# demands its ledger row in the same PR (the #1176 one-instrument rule: the
# ledger and the gate share THIS file).
import re


def enumerate_elements(root):
    src = open(f"{root}/crates/almide-syntax/src/ast.rs", encoding="utf-8").read()
    out = []
    for enum_name in ("ExprKind", "Stmt", "Decl"):
        m = re.search(rf"pub enum {enum_name}\b.*?\{{(.*?)\n\}}", src, re.S)
        if not m:
            continue
        for line in m.group(1).splitlines():
            mm = re.match(r"\s{4}([A-Z][A-Za-z0-9]*)\s*[\{(,]", line)
            if mm:
                out.append(f"{enum_name}::{mm.group(1)}")
    return out

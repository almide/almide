#!/usr/bin/env bash
# Negative controls for the walker-reads-annotations gate (#2186): prove
# scripts/check-walker-reads-annotations.sh FIRES on each way an ownership
# decision can creep back into the renderer. The committed walker is the
# positive control.
set -euo pipefail
cd "$(dirname "$0")/.."

GATE="bash scripts/check-walker-reads-annotations.sh"
WALKER=crates/almide-codegen/src/walker

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

expect_pass() { $GATE "$1" >/dev/null 2>&1 || { echo "FAIL: $2" >&2; exit 1; }; }
expect_fail() { $GATE "$1" >/dev/null 2>&1 && { echo "FAIL: $2" >&2; exit 1; }; return 0; }

expect_pass "$WALKER" \
  "the committed walker did not pass — harness broken, the negatives below would be meaningless"

fresh() { rm -rf "$tmp/w"; mkdir -p "$tmp/w"; cp "$WALKER"/*.rs "$tmp/w/"; }

# 1. An analysis walk inside the renderer.
fresh
cat >>"$tmp/w/expressions.rs" <<'EOF'
struct Probe; impl almide_ir::visit::IrVisitor for Probe {}
EOF
expect_fail "$tmp/w" \
  "gate passed a walker that implements IrVisitor — an analysis walk went unnoticed"

# 2. A per-function ownership set recomputed from the params.
fresh
cat >>"$tmp/w/expressions_control.rs" <<'EOF'
fn probe(ctx: &RenderContext, id: &VarId) -> bool { ctx.ref_params.contains(id) }
EOF
expect_fail "$tmp/w" \
  "gate passed a walker reading a fn-local ref_params set — the #2194 shape went unnoticed"

# 3. Sniffing rendered text to decide ownership.
fresh
cat >>"$tmp/w/expressions_control.rs" <<'EOF'
fn probe(rendered: &str) -> bool { rendered.starts_with("match ") }
EOF
expect_fail "$tmp/w" \
  "gate passed a walker deciding on rendered text (starts_with(\"match \")) — the #1210 shape went unnoticed"

# 4. A param's borrow mode read where only the spelling sites may.
fresh
cat >>"$tmp/w/expressions_control.rs" <<'EOF'
fn probe(p: &IrParam) -> bool { matches!(p.borrow, ParamBorrow::Ref) }
EOF
expect_fail "$tmp/w" \
  "gate passed a walker matching ParamBorrow outside mod.rs / statements.rs — the borrow mode leaked into expression rendering"

# 5. A walker directory with no Rust sources is a hard FAIL, not a green.
mkdir -p "$tmp/empty"
expect_fail "$tmp/empty" \
  "gate passed an empty directory — a moved walker would turn it decorative"

echo "walker-reads-annotations negative controls: 1 positive + 5 negatives all behaved"

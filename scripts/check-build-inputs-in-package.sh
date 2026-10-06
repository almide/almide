#!/usr/bin/env bash
# EVERY CRATE BUILDS FROM ITS OWN PACKAGE (#3361).
#
# A project that depends on almide by git and vendors it (`cargo vendor`,
# Flatpak's flatpak-cargo-generator, Nix) lays each crate out ON ITS OWN:
# `vendor/almide-types/`, `vendor/almide-codegen/`, … — and `../..` from there
# is the consumer's directory, not the almide checkout. Three build scripts
# read `../../stdlib` / `../../runtime/rs/src`, one `include!`d
# `$CARGO_MANIFEST_DIR/../../runtime/rs/src/libm.rs`, one `include_str!`d
# `../../../codegen/templates/rust.toml`, and the vendored build failed at the
# first of them (almide 0.66.0, the issue's repro).
#
# The mechanism (see crates/almide-types/buildscript/in_checkout.rs): a table
# derived from files outside a crate is COMMITTED under that crate's
# `src/generated/`, the crate compiles from it, and its build script
# regenerates it only when it is provably inside an almide checkout. So the
# in-repo loop is unchanged — edit stdlib/, build, the table follows — and
# the one new obligation is committing the regenerated table with the edit.
#
# What this checks:
#   static (default, no cargo — CI `checks` job):
#     1. the shared build-script files are byte-identical to their canonical
#        copy (each crate carries its own; a `#[path]` into a sibling crate is
#        what vendoring breaks);
#     2. no shipped source names a path outside its own crate: every
#        include_str!/include_bytes!/include!/#[path] literal and every
#        `concat!(env!("CARGO_MANIFEST_DIR"), "…")` in build.rs, buildscript/
#        and src/ resolves inside the crate, and no build script `join`s a
#        `..` path except through `in_checkout::checkout_root`. Test-only
#        files are listed below with the reason they cannot ship;
#   --regen (CI `almide-gates` job, needs cargo):
#     3. after forcing every generating build script to rerun, the committed
#        `src/generated/` trees are unchanged (a stdlib/ or runtime/rs/src
#        edit whose table was not committed, or a hand-edited table).
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

MODE="${1:-static}"
fail=0

# ── 1. shared build-script files: one text, several in-package copies ──────
# canonical → copies
COPIES=(
  "crates/almide-types/buildscript/in_checkout.rs crates/almide-codegen/buildscript/in_checkout.rs"
  "crates/almide-types/buildscript/in_checkout.rs crates/almide-egg-lab/buildscript/in_checkout.rs"
  "crates/almide-types/buildscript/in_checkout.rs crates/almide-interp/buildscript/in_checkout.rs"
  "crates/almide-codegen/buildscript/fusion_parse.rs crates/almide-egg-lab/buildscript/fusion_parse.rs"
)
for pair in "${COPIES[@]}"; do
  read -r canon copy <<<"$pair"
  if ! cmp -s "$canon" "$copy"; then
    echo "::error::$copy differs from its canonical $canon."
    echo "  Edit the canonical file and copy it: cp $canon $copy"
    fail=1
  fi
done

# ── 2. no shipped source reaches outside its crate ─────────────────────────
python3 - <<'PY' || fail=1
import os, re, sys

# Files that only ever compile under `cargo test` (or are not compiled at all),
# so a vendored build never reads what they name. Each entry says why.
TEST_ONLY = {
    # `#[cfg(test)] #[path = "witness_tests.rs"] mod tests;` in witness.rs.
    "crates/almide-wasm/src/witness_tests.rs",
    # `include!`d only from the `#[cfg(test)] mod tests` of certificate_c.rs.
    "crates/almide-mir/src/certificate_c_gen.rs",
}
# The one build-script file allowed to name the checkout root (`../..`).
ROOT_GUARD = "buildscript/in_checkout.rs"

LIT = r'"((?:[^"\\]|\\.)*)"'
INCLUDE = re.compile(r'\b(?:include_str|include_bytes|include)!\s*\(\s*' + LIT)
PATH_ATTR = re.compile(r'#\[path\s*=\s*' + LIT + r'\s*\]')
MANIFEST_CONCAT = re.compile(r'concat!\s*\(\s*env!\s*\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*' + LIT)
JOIN_UP = re.compile(r'\.join\(\s*"(\.\.(?:/[^"]*)?)"\s*\)')

def shipped_files(crate):
    for top in ("build.rs",):
        p = os.path.join(crate, top)
        if os.path.isfile(p):
            yield p
    for sub in ("buildscript", "src"):
        for dirpath, dirs, files in os.walk(os.path.join(crate, sub)):
            dirs[:] = [d for d in dirs if d != "tests"]
            for f in files:
                if f.endswith(".rs"):
                    yield os.path.join(dirpath, f)

# Every package directory in the tree. A package's files stop where another
# package begins: `cargo package` / `cargo vendor` leave a nested package's
# directory out, so the ROOT `almide` package ships docs/ and proofs/ but not
# crates/* or runtime/rs/.
PACKAGES = sorted(os.path.realpath(os.path.dirname(p) or ".")
                  for p in os.popen("git ls-files '*Cargo.toml' 'Cargo.toml'").read().split()
                  if os.path.basename(p) == "Cargo.toml")

def inside(crate, path):
    crate = os.path.realpath(crate)
    target = os.path.realpath(os.path.normpath(path))
    if os.path.commonpath([crate, target]) != crate:
        return False
    return not any(pkg != crate and os.path.commonpath([crate, pkg]) == crate
                   and os.path.commonpath([pkg, target]) == pkg for pkg in PACKAGES)

bad = []
crates = sorted(os.path.join("crates", d) for d in os.listdir("crates")
                if os.path.isfile(os.path.join("crates", d, "Cargo.toml")))
if len(crates) < 20:
    print(f"::error::build-inputs: only {len(crates)} crates discovered under crates/ — the scan went blind")
    sys.exit(1)
crates.append(".")  # the root `almide` package: build.rs + src/
scanned = 0
for crate in crates:
    for f in shipped_files(crate):
        f = os.path.normpath(f)
        if f in TEST_ONLY:
            continue
        scanned += 1
        text = open(f, encoding="utf-8", errors="replace").read()
        base = os.path.dirname(f)
        is_build = f == os.path.normpath(os.path.join(crate, "build.rs")) or "/buildscript/" in f
        for n, line in enumerate(text.split("\n"), 1):
            s = line.lstrip()
            if s.startswith("//"):
                continue
            for m in INCLUDE.finditer(line):
                lit = m.group(1)
                if not inside(crate, os.path.join(base, lit)):
                    bad.append(f"{f}:{n}: {m.group(0)} resolves outside {crate}")
            for m in PATH_ATTR.finditer(line):
                lit = m.group(1)
                if not inside(crate, os.path.join(base, lit)):
                    bad.append(f"{f}:{n}: #[path = \"{lit}\"] resolves outside {crate}")
            for m in MANIFEST_CONCAT.finditer(line):
                lit = m.group(1).lstrip("/")
                if not inside(crate, os.path.join(crate, lit)):
                    bad.append(f"{f}:{n}: CARGO_MANIFEST_DIR + \"{m.group(1)}\" resolves outside {crate}")
            if is_build and not f.endswith(ROOT_GUARD):
                for m in JOIN_UP.finditer(line):
                    bad.append(f"{f}:{n}: build script joins \"{m.group(1)}\" — reach the checkout only "
                               "through in_checkout::checkout_root")

if scanned < 500:
    print(f"::error::build-inputs: only {scanned} source files scanned — the scan went blind")
    sys.exit(1)
if bad:
    print("::error::shipped crate sources read outside their package (#3361) — a vendored build breaks:")
    for b in bad:
        print("  " + b)
    print("Commit the input (or a table generated from it) under the crate, regenerated by its")
    print("build script through in_checkout::checkout_root; see scripts/check-build-inputs-in-package.sh.")
    sys.exit(1)
print(f"build-inputs: {len(crates)} crates, {scanned} shipped sources, nothing reads outside its package")
PY

# ── 3. committed tables equal what the sources regenerate ──────────────────
GENERATED=(
  crates/almide-types/src/generated
  crates/almide-codegen/src/generated
  crates/almide-egg-lab/src/generated
  crates/almide-interp/src/generated
)
if [ "$MODE" = "--regen" ]; then
  # A recompiled build script reruns; touching the shared module recompiles
  # all four, whatever their rerun-if-changed lists say.
  touch crates/almide-{types,codegen,egg-lab,interp}/buildscript/in_checkout.rs
  # almide-codegen pulls almide-types and almide-egg-lab; almide-interp is apart.
  cargo build -p almide-codegen -p almide-interp
  drift=$(git status --porcelain -- "${GENERATED[@]}")
  if [ -n "$drift" ]; then
    echo "$drift"
    git diff --stat -- "${GENERATED[@]}" || true
    echo "::error::a committed src/generated table differs from what its sources regenerate —"
    echo "an edit to stdlib/ or runtime/rs/src (or crates/almide-kernel/src) was committed without"
    echo "its table, or a table was hand-edited. Regenerate: cargo build -p almide-codegen -p almide-interp,"
    echo "then commit ${GENERATED[*]}."
    fail=1
  else
    echo "build-inputs: committed generated tables match their sources"
  fi
elif [ "$MODE" != "static" ]; then
  echo "usage: $0 [static|--regen]" >&2
  exit 2
fi

exit "$fail"

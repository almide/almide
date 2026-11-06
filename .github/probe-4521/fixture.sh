#!/usr/bin/env bash
# Usage: fixture.sh <almide-binary> <grant: 1|0>
# Builds the #4521 project (package nd with [native-deps] + native/ndshim.rs,
# root app depending on nd with no [native-deps]) natively; exit = build exit.
set -u
ALMIDE="$1"; GRANT="$2"
ROOT="$(mktemp -d)"
mkdir -p "$ROOT/probe/src" "$ROOT/nd/src" "$ROOT/nd/native" "$ROOT/app/src"
printf '[package]\nname = "almide_issue4521_probe"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n' > "$ROOT/probe/Cargo.toml"
printf 'pub fn value() -> i64 { 4521 }\n' > "$ROOT/probe/src/lib.rs"
printf '[package]\nname = "nd"\nversion = "0.1.0"\n\n[native-deps]\nalmide_issue4521_probe = { path = "%s" }\n' "$ROOT/probe" > "$ROOT/nd/almide.toml"
printf 'pub fn number() -> i64 { almide_issue4521_probe::value() }\n' > "$ROOT/nd/native/ndshim.rs"
printf '@extern(rust, "crate::ndshim", "number")\nfn number() -> Int = _\n' > "$ROOT/nd/src/mod.almd"
printf '[package]\nname = "app"\nversion = "0.1.0"\n\n[dependencies]\nnd = { path = "../nd" }\n' > "$ROOT/app/almide.toml"
if [ "$GRANT" = 1 ]; then printf '\n[permissions]\nnative = ["nd"]\n' >> "$ROOT/app/almide.toml"; fi
printf 'import nd\n\neffect fn main() -> Unit = {\n  println(int.to_string(nd.number()))\n}\n' > "$ROOT/app/src/main.almd"
cd "$ROOT/app"
"$ALMIDE" --version
echo "--- almide.toml"; hew almide.toml
echo "--- almide build"
ALMIDE_RUN_PROJECT_DIR="$ROOT/.run" "$ALMIDE" build src/main.almd -o "$ROOT/app-bin" 2>&1 | tail -40
st=${PIPESTATUS[0]}
echo "build exit: $st"
[ "$st" = 0 ] && { echo "--- binary output"; "$ROOT/app-bin"; }
exit "$st"

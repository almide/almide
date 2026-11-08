set +e
export ALMIDE_BIN=$(command -v almide) CI=true
for c in "bash scripts/check-fmt-gate.sh" "bash scripts/check-doc-fences.sh" "bash scripts/check-env-switches.sh" "almide run tools/almide-gates/src/main.almd -- env-switches-doc . $ALMIDE_BIN" "bash scripts/check-regex-reference.sh" "bash scripts/check-wasi-pins.sh" "bash scripts/check-retired-spellings.sh" "bash scripts/check-retired-spellings-negative.sh" "bash scripts/check-pass-isolated.sh" "scripts/check-semantics-manifest.sh" "bash scripts/check-stdlib-docs.sh" "bash scripts/gen-readme-stats.sh --check" "bash scripts/check-readme-numbers.sh" "almide docs-gen --check" "python3 -I scripts/check-readme-example.py $ALMIDE_BIN" "bash scripts/check-llm-surface.sh" "bash scripts/check-spec-test-files.sh" "bash scripts/check-spec-test-files-negative.sh"; do
  out=$($c 2>&1 </dev/null); rc=$?; echo "[$rc] $c :: $(echo "$out" | tail -4 | tr '\n' ' ' | cut -c1-400)"
done

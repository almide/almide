set +e
echo "=== clippy almide-str"
cargo clippy --release -p almide-str --all-targets 2>&1 | tail -40
for f in spec/lang/stream_fusion_test.almd spec/stdlib/value_test.almd spec/wasm_cross/negative_float_literal_sized.almd spec/wasm_cross/string_reuse_pipeline.almd; do
  echo "=== fmt $f"; cp $f /tmp/f.almd; almide fmt /tmp/f.almd >/dev/null 2>&1; diff -u $f /tmp/f.almd
done
f=spec/wasm_cross/negative_float_literal_sized.almd
echo "=== native"; almide run $f; echo "exit $?"
echo "=== wasm"; almide run $f --target wasm; echo "exit $?"
echo "=== rust emit"; almide $f --target rust | grep -n "maybe32\|result32\|7\.0\|f32" | head -60

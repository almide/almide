#!/usr/bin/env bash
# Build every implementation into $OUT (default ./out):
#   $OUT/native/<impl>__<variant>   native executables (timed)
#   $OUT/wasm/<impl>__<variant>.wasm wasm32-wasip1 modules (timed; .cwasm = AOT)
#   $OUT/count/native|wasm/...      allocation-counting builds (run once)
# A failed build is logged as BUILD-FAILED and skipped; the run reports what built.
# Needs: ALMIDE_HEAD, ALMIDE_REBORN (compiler binaries), JW_DEPS (setup.sh), rustc + wasm32-wasip1.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${OUT:-$PWD/out}"
DEPS="${JW_DEPS:-$PWD/deps}"
WASI="$DEPS/wasi-sdk"
mkdir -p "$OUT/native" "$OUT/wasm" "$OUT/count/native" "$OUT/count/wasm" "$OUT/log"
FAILED=()
step() { # <name> <cmd...>
  local name="$1"; shift
  echo "::group::build $name"
  echo "+ $*"
  if "$@" > "$OUT/log/$name.log" 2>&1; then tail -n 5 "$OUT/log/$name.log"; echo "built $name"
  else cat "$OUT/log/$name.log"; echo "BUILD-FAILED $name"; FAILED+=("$name"); fi
  echo "::endgroup::"
}

# ---- Almide ---------------------------------------------------------------
A="$HERE/almide"
for tag in head reborn; do
  var="ALMIDE_${tag^^}"; almide="${!var:-}"
  [ -x "$almide" ] || { echo "no almide for $tag"; FAILED+=("almide-$tag"); continue; }
  "$almide" --version || true
  # Native --release defaults to -C target-cpu=x86-64-v3 (AVX2) on x86-64.
  step "almide-${tag}__v3"       "$almide" build "$A/jsonround_phased.almd" -o "$OUT/native/almide-${tag}__v3" --release
  step "almide-${tag}__portable" "$almide" build "$A/jsonround_phased.almd" -o "$OUT/native/almide-${tag}__portable" --release --target-cpu portable
  step "almide-${tag}__native"   "$almide" build "$A/jsonround_phased.almd" -o "$OUT/native/almide-${tag}__native" --release --target-cpu native
  step "almide-$tag-exact__v3" "$almide" build "$A/jsonround.almd" -o "$OUT/native/almide-$tag-exact__v3" --release
  step "almide-${tag}__wasm"     "$almide" build "$A/jsonround_phased.almd" --target wasm -o "$OUT/wasm/almide-${tag}__plain.wasm"
  for p in jsonround_phased jr_build jr_stringify; do
    step "count-almide-$tag-$p" env ALMIDE_ALLOC_COUNT=1 "$almide" build "$A/$p.almd" -o "$OUT/count/native/almide-$tag-$p" --release
  done
done

# ---- Rust -----------------------------------------------------------------
R="$HERE/rust"
BINS="serde sonic simd_owned simd_borrowed"
rust_build() { # <name> <target-or-empty> <rustflags> <features> <dest>
  local name="$1" target="$2" flags="$3" feat="$4" dest="$5"
  local tdir="$OUT/cargo-target/$name${target:+-wasm}"
  local targs=(); [ -n "$target" ] && targs=(--target "$target")
  local fargs=(); [ -n "$feat" ] && fargs=(--features "$feat")
  RUSTFLAGS="$flags" cargo build --release --manifest-path "$R/Cargo.toml" --target-dir "$tdir" "${targs[@]}" "${fargs[@]}" --bins || return 1
  local dir="$tdir/${target:+$target/}release" b
  for b in $BINS; do
    if [ -n "$target" ]; then cp "$dir/$b.wasm" "$dest/${b}__$name.wasm"; else cp "$dir/$b" "$dest/${b}__$name"; fi
  done
}
step rust-portable     rust_build portable ""            ""                           ""    "$OUT/native"
step rust-native       rust_build native   ""            "-C target-cpu=native"       ""    "$OUT/native"
step rust-count        rust_build count    ""            ""                           count "$OUT/count/native"
step rust-wasm         rust_build plain    wasm32-wasip1 ""                           ""    "$OUT/wasm"
step rust-wasm-simd128 rust_build simd128  wasm32-wasip1 "-C target-feature=+simd128" ""    "$OUT/wasm"
step rust-wasm-count   rust_build count    wasm32-wasip1 ""                           count "$OUT/count/wasm"

# ---- C / C++ --------------------------------------------------------------
C="$HERE/c"
YY="$DEPS/yyjson/src"
SJ="$DEPS/simdjson"
# simdjson.cpp is the slow part: compile it once per flag set.
cc_native() { # <variant> <flags...>
  local v="$1"; shift
  gcc -O3 -DNDEBUG "$@" -I"$C" -I"$YY" "$C/yyjson_bench.c" "$YY/yyjson.c" -o "$OUT/native/yyjson__$v" &&
  g++ -O3 -DNDEBUG -std=c++17 "$@" -I"$C" -I"$SJ" -c "$SJ/simdjson.cpp" -o "$OUT/log/simdjson-$v.o" &&
  g++ -O3 -DNDEBUG -std=c++17 "$@" -I"$C" -I"$SJ" "$C/simdjson_bench.cpp" "$OUT/log/simdjson-$v.o" -o "$OUT/native/simdjson-dom__$v" &&
  g++ -O3 -DNDEBUG -std=c++17 "$@" -DJW_ONDEMAND -I"$C" -I"$SJ" "$C/simdjson_bench.cpp" "$OUT/log/simdjson-$v.o" -o "$OUT/native/simdjson-ondemand__$v"
}
step c-portable cc_native portable
step c-native   cc_native native -march=native
cc_wasm() { # <variant> <flags...>
  local v="$1"; shift
  local cc="$WASI/bin/clang --target=wasm32-wasip1" cxx="$WASI/bin/clang++ --target=wasm32-wasip1"
  local cxxf=(-std=c++17 -fno-exceptions -DSIMDJSON_EXCEPTIONS=0)
  $cc -O3 -DNDEBUG "$@" -I"$C" -I"$YY" "$C/yyjson_bench.c" "$YY/yyjson.c" -o "$OUT/wasm/yyjson__$v.wasm" &&
  $cxx -O3 -DNDEBUG "${cxxf[@]}" "$@" -I"$C" -I"$SJ" -c "$SJ/simdjson.cpp" -o "$OUT/log/simdjson-wasm-$v.o" &&
  $cxx -O3 -DNDEBUG "${cxxf[@]}" "$@" -I"$C" -I"$SJ" "$C/simdjson_bench.cpp" "$OUT/log/simdjson-wasm-$v.o" -o "$OUT/wasm/simdjson-dom__$v.wasm" &&
  $cxx -O3 -DNDEBUG "${cxxf[@]}" "$@" -DJW_ONDEMAND -I"$C" -I"$SJ" "$C/simdjson_bench.cpp" "$OUT/log/simdjson-wasm-$v.o" -o "$OUT/wasm/simdjson-ondemand__$v.wasm"
}
step c-wasm         cc_wasm plain
step c-wasm-simd128 cc_wasm simd128 -msimd128

# The C/C++ benches count allocations in every build; reuse the portable/plain ones.
for f in yyjson simdjson-dom simdjson-ondemand; do
  [ -f "$OUT/native/${f}__portable" ] && cp "$OUT/native/${f}__portable" "$OUT/count/native/$f"
  [ -f "$OUT/wasm/${f}__plain.wasm" ] && cp "$OUT/wasm/${f}__plain.wasm" "$OUT/count/wasm/$f.wasm"
done

# ---- wasm AOT -------------------------------------------------------------
for w in "$OUT"/wasm/*.wasm; do
  step "aot-$(basename "$w" .wasm)" wasmtime compile "$w" -o "${w%.wasm}.cwasm"
done

echo "== built"; ls -la "$OUT/native" "$OUT/wasm" "$OUT/count/native" "$OUT/count/wasm"
if [ ${#FAILED[@]} -gt 0 ]; then echo "BUILD-FAILED: ${FAILED[*]}"; fi
printf '%s\n' "${FAILED[@]}" > "$OUT/build-failed.txt"
exit 0

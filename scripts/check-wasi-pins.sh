#!/usr/bin/env bash
# #1628 stage 4 — the WASI/Component-Model pin gate.
#
# proofs/wasi-pin-policy.toml is the ledger; this gate re-derives every
# fact it states from the tree, so none of them can drift silently:
#   1. the vendored p3 WIT interfaces are at exactly [wasi].minor;
#   2. the p3 shim imports interfaces at that same version;
#   3. the embedded host's wasmtime Cargo pin is [runtime].crate_major;
#   4. CI installs [runtime].ci for the execution legs;
#   5. the p3 test harness passes [runtime].flags verbatim;
#   6. the wasm-tools family pins (wasmparser / wit-component) agree
#      with [wasm-tools].family AND with each other (lockstep doctrine).
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

fail=0
err() { echo "::error::check-wasi-pins: $1"; fail=1; }

policy="proofs/wasi-pin-policy.toml"
get() { grep -E "^$2 *= *" "$1" | head -1 | sed -E 's/^[^=]*= *"?([^"]*)"?.*/\1/'; }

minor="$(get "$policy" minor)"
crate_major="$(get "$policy" crate_major)"
ci_ver="$(get "$policy" ci)"
flags="$(get "$policy" flags)"
family="$(get "$policy" family)"

# 1. vendored WIT versions.
while IFS= read -r v; do
  [ "$v" = "$minor" ] || err "vendored WIT at wasi:*@$v, policy says $minor"
done < <(grep -rhoE "^package wasi:[a-z-]+@[0-9.]+" crates/almide-wasm-run/wit/p3/deps/*/package.wit | sed -E 's/.*@//' | sort -u)

# 2. the p3 shim's import interface versions.
while IFS= read -r v; do
  [ "$v" = "$minor" ] || err "wasi_p3.rs imports wasi:*@$v, policy says $minor"
done < <(grep -ohE "wasi:[a-z/-]+@[0-9.]+" crates/almide-wasm-run/src/wasi_p3.rs | sed -E 's/.*@//' | sort -u)

# 3. the embedded host's wasmtime pin.
got="$(grep -E '^wasmtime *= *' crates/almide-wasm-run/Cargo.toml | sed -E 's/[^0-9]*([0-9]+).*/\1/')"
[ "$got" = "$crate_major" ] || err "wasmtime Cargo pin is $got, policy says $crate_major"

# 4. the CI-installed runtime.
grep -q "wasmtime-${ci_ver}-" .github/workflows/ci.yml \
  || err "ci.yml does not install wasmtime ${ci_ver} (policy [runtime].ci)"

# 5. the p3 harness flag surface.
python3 - "$flags" <<'PY'
import re, sys
flags = sys.argv[1]
src = open("tests/component_p3_test.rs").read()
# The harness passes the same flags as one -W value and -S p3=y args.
w = re.search(r'"-W",\s*\n?\s*"([^"]+)"', src)
ok = w and w.group(1) in flags and '"-S"' in src and '"p3=y"' in src
sys.exit(0 if ok else 1)
PY
[ $? -eq 0 ] || err "component_p3_test.rs flag surface drifted from policy [runtime].flags"

# 6. wasm-tools family lockstep.
for dep in wasmparser wit-component; do
  got="$(grep -E "^$dep *= *" Cargo.toml | head -1 | sed -E 's/[^0-9]*([0-9]+\.[0-9]+).*/\1/')"
  [ "$got" = "$family" ] || err "$dep pinned $got, policy family $family"
done

# 7. Capability DCE (#2152 item 6). `almide check --profile critical --allow IO`
#    refuses a program that names a denied capability at CHECK time; this
#    fact says the same thing about the ARTIFACT: a program granted only IO
#    (and using only `io`) must ship a wasm whose import section carries no
#    host function of a denied family — FS (granted by IO but unused, so it
#    must be dead too), Net, Env/args, Process. Read straight from the
#    import section (a 40-line LEB128 walk, no wasm-tools dependency), never
#    from the emitter's own claim about itself.
#
#    KNOWN BASE (measured 2026-09-22 at the landing): the structural leg's
#    WASI form imports fd_write / proc_exit / random_get / clock_time_get /
#    fd_read UNCONDITIONALLY (crates/almide-wasi/src/lib.rs, the "base
#    five"), so a program granted only IO still imports the Rand and Time
#    entry points. That is a capability the profile denied and the artifact
#    still asks for; it is recorded here as the base set the gate tolerates,
#    so that trimming it later is a tightening of this list, not a surprise.
#
#    The gate also builds a CONTROL that uses `fs` and demands that the same
#    reader finds an FS import in it — a detector that reads nothing would
#    pass the first assertion for free.
#
#    Needs a compiler: ALMIDE_BIN, else target/release/almide, else `almide`
#    on PATH. None found is a FAIL, not a skip: a gate that skips when its
#    instrument is missing reads green forever.
ALMIDE_BIN="${ALMIDE_BIN:-}"
if [ -z "$ALMIDE_BIN" ]; then
  if [ -x target/release/almide ]; then ALMIDE_BIN=target/release/almide
  elif command -v almide >/dev/null 2>&1; then ALMIDE_BIN="$(command -v almide)"
  fi
fi
if [ -z "$ALMIDE_BIN" ] || [ ! -x "$ALMIDE_BIN" ]; then
  err "capability DCE: no almide binary (set ALMIDE_BIN, or build target/release/almide)"
else
  dce_tmp="$(mktemp -d)"
  trap 'rm -rf "$dce_tmp"' EXIT
  cat > "$dce_tmp/io_only.almd" <<'ALMD'
import io
effect fn main() -> Unit = {
  let line = io.read_line()
  println("io only: ${line}")
}
ALMD
  cat > "$dce_tmp/fs_control.almd" <<'ALMD'
import fs
effect fn main() -> Unit = println(fs.read_text("control.txt")!)
ALMD
  if ! "$ALMIDE_BIN" check "$dce_tmp/io_only.almd" --profile critical --allow IO >"$dce_tmp/check.log" 2>&1; then
    err "capability DCE: the IO-only program does not pass --profile critical --allow IO: $(tr '\n' ' ' < "$dce_tmp/check.log" | cut -c1-200)"
  elif ! "$ALMIDE_BIN" build "$dce_tmp/io_only.almd" --target wasm -o "$dce_tmp/io_only.wasm" >"$dce_tmp/build.log" 2>&1 \
    || ! "$ALMIDE_BIN" build "$dce_tmp/fs_control.almd" --target wasm -o "$dce_tmp/fs_control.wasm" >>"$dce_tmp/build.log" 2>&1; then
    err "capability DCE: wasm build failed: $(tr '\n' ' ' < "$dce_tmp/build.log" | cut -c1-200)"
  else
    python3 - "$dce_tmp/io_only.wasm" "$dce_tmp/fs_control.wasm" <<'PY'
import re, sys

def leb(b, i):
    r = s = 0
    while True:
        c = b[i]; i += 1
        r |= (c & 0x7f) << s; s += 7
        if not c & 0x80: return r, i

def imports(path):
    b = open(path, "rb").read()
    assert b[:4] == b"\0asm", path
    i, out = 8, []
    while i < len(b):
        sid = b[i]; i += 1
        size, i = leb(b, i)
        end = i + size
        if sid == 2:
            n, i = leb(b, i)
            for _ in range(n):
                ml, i = leb(b, i); mod = b[i:i+ml].decode(); i += ml
                nl, i = leb(b, i); name = b[i:i+nl].decode(); i += nl
                kind = b[i]; i += 1
                if kind == 0: _, i = leb(b, i)
                elif kind == 1:
                    i += 1; flags = b[i]; i += 1; _, i = leb(b, i)
                    if flags & 1: _, i = leb(b, i)
                elif kind == 2:
                    flags = b[i]; i += 1; _, i = leb(b, i)
                    if flags & 1: _, i = leb(b, i)
                elif kind == 3: i += 2
                else: sys.exit("unknown import kind %d in %s" % (kind, path))
                out.append((mod, name))
        i = end
    return out

# Denied families under `--allow IO`, by host import name (WASI p1) or module.
FAMILIES = {
    "fs":      re.compile(r"^(path_.*|fd_prestat_.*|fd_filestat_.*|fd_fdstat_.*|fd_readdir|fd_seek|fd_tell|fd_close|fd_sync|fd_datasync|fd_allocate|fd_advise|fd_pread|fd_pwrite|fd_renumber)$"),
    "net":     re.compile(r"^sock_.*$"),
    "env":     re.compile(r"^(environ_.*|args_.*)$"),
    "process": re.compile(r"^(proc_raise|sched_yield)$"),
}
DENIED_MODULES = re.compile(r"^wasi:(http|sockets|filesystem|cli/environment|random|clocks)")
# The base five (wasi.rs) — tolerated, recorded, see the comment above.
BASE = {"fd_write", "proc_exit", "random_get", "clock_time_get", "fd_read"}

def offending(imps):
    hits = []
    for mod, name in imps:
        if DENIED_MODULES.match(mod):
            hits.append((mod, name, "module")); continue
        for fam, rx in FAMILIES.items():
            if rx.match(name): hits.append((mod, name, fam))
    return hits

io_imps = imports(sys.argv[1])
ctrl_imps = imports(sys.argv[2])
bad = offending(io_imps)
ctrl_fs = [h for h in offending(ctrl_imps) if h[2] == "fs"]
print("capability DCE: io_only imports = %s" % ", ".join("%s.%s" % p for p in io_imps))
print("capability DCE: fs_control imports = %s" % ", ".join("%s.%s" % p for p in ctrl_imps))
fail = 0
if bad:
    print("::error::check-wasi-pins: capability DCE: the IO-only artifact imports a denied family: " + ", ".join("%s.%s (%s)" % h for h in bad))
    fail = 1
if not ctrl_fs:
    print("::error::check-wasi-pins: capability DCE: the fs CONTROL shows no fs import — the reader is not reading the import section")
    fail = 1
extra = {n for _, n in io_imps} - BASE
if extra:
    print("::error::check-wasi-pins: capability DCE: the IO-only artifact imports beyond the recorded base five: " + ", ".join(sorted(extra)))
    fail = 1
sys.exit(fail)
PY
    [ $? -eq 0 ] || err "capability DCE: see the lines above"
  fi
fi

[ "$fail" -eq 0 ] && echo "wasi-pins OK: WASI $minor, wasmtime crate $crate_major / CI $ci_ver, wasm-tools $family — every stated pin re-derived from the tree; the IO-only artifact carries no denied-family host import and the fs control does."
exit "$fail"

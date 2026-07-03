#!/usr/bin/env bash
# TOOLCHAIN STAMP (flight-evidence-gaps F6-2): every proof gate prints the exact
# binaries its evidence was taken with, and FAILS if the PATH `almide` is not the
# workspace build — the 2026-07-03 incident (the PATH binary silently replaced by
# an older v0.27.13 build mid-session) made two spec files fail for reasons that
# had nothing to do with the change under test. Evidence without identity is not
# evidence. Sourced (not executed) by each gate after ROOT is set.
stamp_toolchain() {
    local root="${1:?stamp_toolchain: pass ROOT}"
    local alm; alm="$(command -v almide || true)"
    local ws="$root/target/release/almide"
    echo "── toolchain stamp ──"
    if [ -n "$alm" ]; then
        local ver hash mt
        ver="$("$alm" --version 2>/dev/null | head -1)"
        hash="$(shasum -a 256 "$alm" 2>/dev/null | cut -c1-16)"
        mt="$(stat -f '%Sm' "$alm" 2>/dev/null || stat -c '%y' "$alm" 2>/dev/null)"
        echo "  almide: $ver  sha256:$hash  mtime:$mt  ($alm)"
        if [ -x "$ws" ]; then
            local wshash
            wshash="$(shasum -a 256 "$ws" 2>/dev/null | cut -c1-16)"
            if [ "$hash" != "$wshash" ]; then
                echo "  FATAL: PATH almide ($hash) != workspace build ($wshash)."
                echo "  The evidence this gate produces would not describe the tree under test."
                echo "  Run 'make install' (or fix PATH) and re-run."
                return 1
            fi
        fi
    else
        echo "  almide: NOT ON PATH"
    fi
    echo "  rustc:    $(rustc --version 2>/dev/null || echo n/a)"
    echo "  wasmtime: $(wasmtime --version 2>/dev/null || echo n/a)"
    echo "  coqc:     $(coqc --version 2>/dev/null | head -1 || echo n/a)"
    echo "  tree:     $(git -C "$root" rev-parse --short HEAD 2>/dev/null) $(git -C "$root" status --porcelain 2>/dev/null | wc -l | tr -d ' ') dirty file(s)"
    echo "─────────────────────"
}

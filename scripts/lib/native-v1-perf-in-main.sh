#!/usr/bin/env bash
# Shared by scripts/check-native-v1-perf-small.sh and scripts/check-native-v1-perf.sh
# (its time-bound rows): the instructions a binary executes inside its own
# `main` and every thread's start, the stack-guard install toggled out, under
# callgrind with round-robin threads. The caller sets WORK (a scratch dir).
#
# in_main BIN [VALGRIND_ARG...] — prints the count (`.` when `main` was never
# entered, nothing when callgrind wrote no profile). Extra arguments go to
# valgrind on every run (a time-bound row passes `--trace-syscalls=yes
# --log-file=...` to count its passes in the same run).
in_main() {
  local bin="$1"; shift
  local out="$WORK/cg.$$.$RANDOM"
  timeout 900 valgrind --tool=callgrind --fair-sched=yes --collect-atstart=no --toggle-collect='almide_*::main' --toggle-collect='*thread_start*' \
    --toggle-collect='*almide_stack_guard_install' "$@" \
    --callgrind-out-file="$out" "$bin" </dev/null >/dev/null 2>&1
  [ -s "$out" ] || return 0
  local n
  n=$(callgrind_annotate --auto=no "$out" 2>/dev/null | awk '/PROGRAM TOTALS/ { gsub(",", "", $1); print $1 }')
  rm -f "$out"
  # Valgrind 3.22 reads no symbols from a binary whose two writable PT_LOAD
  # segments land in one mapping (a link-layout accident: guard_statement's
  # v1 binary). It then names each block by its address, so the same three
  # entry points are toggled by address: the symbol's address from `nm`
  # plus the load base valgrind gives a PIE on amd64 (0x108000). Both sides
  # fail closed: a wrong base toggles nothing, and nothing collected is
  # MEASURE-FAIL.
  if ! [[ "$n" =~ ^[0-9]+$ ]] && command -v nm >/dev/null; then
    local addrs=()
    while read -r a _k _name; do
      addrs+=("--toggle-collect=$(printf '0x%016x' $((0x$a + 0x108000)))")
    done < <(nm -C "$bin" 2>/dev/null | grep -E ' almide_[A-Za-z0-9_]*::main$|thread_start$|::almide_stack_guard_install$')
    if [ "${#addrs[@]}" -gt 0 ]; then
      timeout 900 valgrind --tool=callgrind --fair-sched=yes --collect-atstart=no "${addrs[@]}" "$@" \
        --callgrind-out-file="$out" "$bin" </dev/null >/dev/null 2>&1
      [ -s "$out" ] && n=$(callgrind_annotate --auto=no "$out" 2>/dev/null | awk '/PROGRAM TOTALS/ { gsub(",", "", $1); print $1 }')
      rm -f "$out"
    fi
  fi
  echo "$n"
}

# A program whose loop runs until a clock has advanced (proofs/native-v1-perf-time-bound.txt)
# executes as many passes as fit in that wall time, so its total instruction
# count measures the machine's speed under valgrind, not the render. Its row
# names the syscall every pass makes and how many times per pass: the passes
# of one run are that syscall's count in the run's own trace, less the empty
# program's count on the same leg, over the reads per pass.
#
# per_pass_work BIN GLUE SYSCALL READS_PER_PASS EMPTY_CALLS — prints
# "<work per pass> <passes> <work>" (work = in_main less GLUE), or nothing
# when `main` was never entered.
per_pass_work() {
  local bin="$1" glue="$2" sc="$3" reads="$4" empty="$5"
  local log="$WORK/tr.$$.$RANDOM" m c
  m=$(in_main "$bin" --trace-syscalls=yes --log-file="$log")
  c=$(grep -cE "\) sys_${sc}[ (]" "$log" 2>/dev/null); rm -f "$log"
  [[ "$m" =~ ^[0-9]+$ ]] || return 0
  awk -v m="$m" -v g="$glue" -v c="${c:-0}" -v e="$empty" -v r="$reads" 'BEGIN {
    p = (c - e) / r; w = m - g; if (w < 1) w = 1
    if (p < 1) { print "0 0 " w; exit }
    printf "%.3f %.1f %d\n", w / p, p, w }'
}

# syscall_calls BIN SYSCALL — the syscall's count in one traced run of BIN
# (the empty program's, taken off a time-bound row's count).
syscall_calls() {
  local log="$WORK/tr.$$.$RANDOM" c
  timeout 900 valgrind --tool=none --trace-syscalls=yes --log-file="$log" "$1" </dev/null >/dev/null 2>&1
  c=$(grep -cE "\) sys_${2}[ (]" "$log" 2>/dev/null); rm -f "$log"
  echo "${c:-0}"
}

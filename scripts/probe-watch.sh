#!/usr/bin/env bash
# Probe only: run one phase of `make verify-trust` under a watchdog.
#
# A hosted runner that runs out of memory or of disk does not fail its step:
# it stops answering, the job ends as "lost communication with the server",
# and the step's log is gone. This wrapper prints the free memory and the free
# disk as the phase runs, and STOPS the phase while there is still room to
# say why: the largest processes, the largest directories. The step then
# fails like any other, with its log.
#
#   scripts/probe-watch.sh <label> <command> [<argument>...]
set -uo pipefail
label="$1"; shift
MEM_FLOOR_MB="${WATCH_MEM_FLOOR_MB:-700}"
DISK_FLOOR_MB="${WATCH_DISK_FLOOR_MB:-2500}"

mem() { awk '/MemAvailable/ { print int($2 / 1024) }' /proc/meminfo; }
disk() { df -BM --output=avail / | tail -1 | tr -dc '0-9'; }
snap() { printf '[watch %s %s] mem available %s MB, disk free %s MB, load %s\n' "$label" "$(date -u +%H:%M:%S)" "$(mem)" "$(disk)" "$(cut -d' ' -f1 /proc/loadavg)"; }
largest() {
  echo "[watch $label] largest processes (rss KiB, seconds, command):"
  ps -eo rss,etimes,args --sort=-rss | head -8 | cut -c1-220
}
dirs() {
  echo "[watch $label] largest directories (MB):"
  { du -sm "${TMPDIR:-/tmp}"/* 2>/dev/null; du -sm target/* 2>/dev/null; du -sm "$HOME/.cargo" "$HOME/.opam" 2>/dev/null; } | sort -rn | head -12
}

LARGEST_EVERY="${WATCH_LARGEST_EVERY:-60}"
LOWFILE="$(mktemp)"
snap
setsid "$@" &
pid=$!
(
  n=0
  low="$(mem)"
  while kill -0 "$pid" 2>/dev/null; do
    m="$(mem)"; d="$(disk)"
    if [ "$m" -lt "$low" ]; then low="$m"; echo "$low" > "$LOWFILE"; fi
    if [ "$m" -lt "$MEM_FLOOR_MB" ] || [ "$d" -lt "$DISK_FLOOR_MB" ]; then
      echo "[watch $label] STOPPING the phase: mem available $m MB (floor $MEM_FLOOR_MB), disk free $d MB (floor $DISK_FLOOR_MB)"
      largest
      kill -TERM -- "-$pid" 2>/dev/null
      sleep 3
      kill -KILL -- "-$pid" 2>/dev/null
      dirs
      exit 0
    fi
    n=$((n + 1))
    if [ $((n % 12)) -eq 0 ]; then snap; fi
    if [ $((n % LARGEST_EVERY)) -eq 0 ]; then largest; fi
    sleep 5
  done
) &
wait "$pid"; rc=$?
snap
echo "[watch $label] lowest mem available seen, sampled every 5 s: $(cat "$LOWFILE" 2>/dev/null || echo '(never below the start)') MB"
dirs
echo "[watch $label] exit $rc"
exit "$rc"

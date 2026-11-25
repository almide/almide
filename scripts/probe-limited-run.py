#!/usr/bin/env python3
"""Run one command under a time limit and a resident-memory limit.

  probe-limited-run.py <seconds> <rss_kb> <stdout file> <stderr file> -- <command> [args...]

The command is killed, with its process group, when it has run longer than
<seconds> or when its resident set exceeds <rss_kb>. Exit code: the command's
own, 124 after the time limit, 125 after the memory limit.

Why not `ulimit -v`: an address-space limit counts what a process RESERVES, and
the OCaml 5 runtime reserves its minor heaps before it runs anything, so the
proof assistant aborts at start under any limit small enough to protect the
machine. Resident memory is what the machine actually runs out of.
"""
import os, signal, subprocess, sys, time

seconds, rss_kb, out, err = float(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
cmd = sys.argv[sys.argv.index("--") + 1:]


def rss(pid):
    try:
        with open(f"/proc/{pid}/status", encoding="ascii", errors="replace") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return 0


why = None
with open(out, "wb") as o, open(err, "wb") as e:
    p = subprocess.Popen(cmd, stdout=o, stderr=e, start_new_session=True)
    t0 = time.monotonic()
    while p.poll() is None:
        if time.monotonic() - t0 > seconds:
            why = (124, f"the time limit of {seconds:g} s")
        elif rss(p.pid) > rss_kb:
            why = (125, f"the resident-memory limit of {rss_kb} kB")
        if why:
            try:
                os.killpg(p.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            p.wait()
            break
        time.sleep(0.1)
if why:
    with open(err, "ab") as e:
        e.write(f"LIMIT: killed at {why[1]}\n".encode())
    sys.exit(why[0])
sys.exit(p.returncode)

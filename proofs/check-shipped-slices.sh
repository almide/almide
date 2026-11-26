#!/usr/bin/env bash
# The byte strings the kernel judges, RE-DERIVED from the shipped module by a
# second decoder.
#
# check-shipped-windows.sh, check-shipped-fresh.sh and check-shipped-loops.sh
# hand the Rocq kernel byte strings that almide-anchor-check cut out of an
# artifact, and every theorem behind them is about "these bytes, run as
# instructions of one function". That the strings ARE consecutive instruction
# runs of one function body was the cutter's word alone: its offsets come from
# the decoder library it links. This script reads the same artifact with a
# decoder of its own (the module's sections, the code section's bodies, the
# length of every instruction; no library) and holds every proposal to it:
#
#   * the function and the access the proposal names exist, and the access is
#     a load or a store;
#   * the strings, laid end to end so that the one ending in the access ends
#     where the access ends, are the module's bytes there, inside that one body;
#   * every place a string starts or ends is where an instruction starts in
#     this script's own reading of the body;
#   * a window starts at the access's control depth and never goes below it;
#   * the module has ONE memory, unshared and addressed by 32-bit offsets. The
#     three rules read a load or a store as an access of the one linear memory
#     that no other thread writes, and the kernel is never shown the import
#     section or the memory section, so that this is true of the artifact is
#     held here.
#
# It judges where the bytes are, not what they mean: the verdict on a proposal
# stays the kernel's.
#
#   proofs/check-shipped-slices.sh <manifest> <window-dump> <fresh-dump> <loops-dump>
#   proofs/check-shipped-slices.sh --forge <dir> <manifest> <window-dump> <fresh-dump> <loops-dump>
#
# <manifest> has one line per swept artifact, `<label>\t<path to the .wasm>`,
# in sweep order; each dump is the one its gate reads (`MODULE <label>` and
# the cutter's output per artifact). Fails when a dump's modules are not the
# manifest's, in order (a module went unread), when a proposal fails any of
# the points above, or when this script's decoder meets an instruction it does
# not read (it does not guess a length).
#
# --forge writes, under <dir>, one sub-directory per forged input: the three
# dumps, cut down to one module's proposals, with ONE proposal altered (a
# start moved inside an instruction, a cut between two strings moved inside
# one, a byte changed, another function named, a module's block removed) and
# a file `expect` with the words the refusal must contain. Two more leave the
# dumps as they are and alter the MODULE (a second memory added to its memory
# section; its memory made shared): their sub-directory also holds the forged
# `module.wasm` and a `manifest` that names it. The caller runs this script on each and requires the
# refusal (proofs/structural-wall.sh does, on every run). The forgeries are
# built with this script's decoder so that each breaks exactly one point.
set -euo pipefail
export LC_ALL=C
FORGE=""
if [ "${1:-}" = --forge ]; then
  FORGE="${2:?usage: proofs/check-shipped-slices.sh --forge <dir> <manifest> <window-dump> <fresh-dump> <loops-dump>}"
  shift 2
fi
[ $# -eq 4 ] || { echo "usage: proofs/check-shipped-slices.sh [--forge <dir>] <manifest> <window-dump> <fresh-dump> <loops-dump>" >&2; exit 2; }

python3 -I - "$FORGE" "$@" <<'PY'
import os, sys

forge_dir, manifest_path, window_path, fresh_path, loops_path = sys.argv[1:6]


class Unread(Exception):
    """This script's decoder does not read the bytes: no length is guessed."""


def uleb(b, p, lim):
    r = s = 0
    while True:
        if p >= lim:
            raise Unread("a number runs past the bytes it is in")
        x = b[p]
        p += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, p
        if s > 63:
            raise Unread("a number of more than ten bytes")


VALTYPES = (0x7F, 0x7E, 0x7D, 0x7C, 0x7B, 0x70, 0x6F)


def function_bodies(b):
    """(first instruction's offset, one past the body) per defined function."""
    if b[:8] != b"\x00asm\x01\x00\x00\x00":
        raise Unread("not a version-1 module")
    p, out, seen = 8, [], False
    while p < len(b):
        sid = b[p]
        size, p = uleb(b, p + 1, len(b))
        end = p + size
        if end > len(b):
            raise Unread("a section runs past the file")
        if sid == 10:
            if seen:
                raise Unread("two code sections")
            seen = True
            n, q = uleb(b, p, end)
            for _ in range(n):
                size, q = uleb(b, q, end)
                fe = q + size
                if fe > end:
                    raise Unread("a function body runs past the code section")
                groups, r = uleb(b, q, fe)
                for _ in range(groups):
                    _, r = uleb(b, r, fe)
                    if r >= fe or b[r] not in VALTYPES:
                        raise Unread("a local of a type this script does not read")
                    r += 1
                out.append((r, fe))
                q = fe
            if q != end:
                raise Unread("the code section does not end with its last body")
        p = end
    return out


def limits(b, p, end):
    """(flags, minimum, maximum or None, the offset past the limits)."""
    if p >= end:
        raise Unread("limits run past their section")
    flags = b[p]
    if flags > 0x07:
        raise Unread(f"limits with flags 0x{flags:02x}")
    lo, p = uleb(b, p + 1, end)
    hi = None
    if flags & 0x01:
        hi, p = uleb(b, p, end)
    return flags, lo, hi, p


def memories(b):
    """(flags, minimum, maximum or None) of every memory the module imports or
    defines, in index order. function_bodies has held the sections' extents."""
    p, out = 8, []
    while p < len(b):
        sid = b[p]
        size, p = uleb(b, p + 1, len(b))
        end = p + size
        if sid == 2:
            n, q = uleb(b, p, end)
            for _ in range(n):
                for _name in range(2):
                    k, q = uleb(b, q, end)
                    q += k
                if q >= end:
                    raise Unread("an import runs past its section")
                kind, q = b[q], q + 1
                if kind == 0x00:
                    _, q = uleb(b, q, end)
                elif kind == 0x01:
                    if q >= end or b[q] not in (0x70, 0x6F):
                        raise Unread("an imported table of a type this script does not read")
                    q = limits(b, q + 1, end)[3]
                elif kind == 0x02:
                    flags, lo, hi, q = limits(b, q, end)
                    out.append((flags, lo, hi))
                elif kind == 0x03:
                    if q >= end or b[q] not in VALTYPES:
                        raise Unread("an imported global of a type this script does not read")
                    q += 2
                elif kind == 0x04:
                    _, q = uleb(b, q + 1, end)
                else:
                    raise Unread(f"an import of kind 0x{kind:02x}")
            if q != end:
                raise Unread("the import section does not end with its last import")
        elif sid == 5:
            n, q = uleb(b, p, end)
            for _ in range(n):
                flags, lo, hi, q = limits(b, q, end)
                out.append((flags, lo, hi))
            if q != end:
                raise Unread("the memory section does not end with its last memory")
        p = end
    return out


def instructions(b, start, end):
    """(offset, opcode, control depth before it) per instruction of a body."""
    p, depth, out = start, 0, []
    while p < end:
        at, op, before = p, b[p], depth
        p += 1
        if op in (0x02, 0x03, 0x04):  # block, loop, if: a block type
            _, p = uleb(b, p, end)
            depth += 1
        elif op == 0x0B:  # end
            depth -= 1
        elif op in (0x00, 0x01, 0x05, 0x0F, 0x1A, 0x1B, 0xD1):
            pass
        elif op in (0x0C, 0x0D, 0x10, 0x12, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x3F, 0x40, 0x41, 0x42, 0xD2):
            _, p = uleb(b, p, end)  # one index or one constant
        elif op == 0x0E:  # br_table
            n, p = uleb(b, p, end)
            for _ in range(n + 1):
                _, p = uleb(b, p, end)
        elif op in (0x11, 0x13):  # call_indirect, return_call_indirect
            _, p = uleb(b, p, end)
            _, p = uleb(b, p, end)
        elif op == 0x1C:  # select with types
            n, p = uleb(b, p, end)
            p += n
        elif 0x28 <= op <= 0x3E:  # loads and stores: alignment, offset
            align, p = uleb(b, p, end)
            if align >= 64:
                raise Unread("an access that names a memory")
            _, p = uleb(b, p, end)
        elif op == 0x43:
            p += 4
        elif op == 0x44:
            p += 8
        elif 0x45 <= op <= 0xC4:
            pass
        elif op == 0xD0:  # ref.null
            if p >= end or b[p] not in (0x70, 0x6F):
                raise Unread("a null of a type this script does not read")
            p += 1
        elif op == 0xFC:
            sub, p = uleb(b, p, end)
            if sub <= 7:
                pass
            elif sub in (9, 11, 13, 15, 16, 17):
                _, p = uleb(b, p, end)
            elif sub in (8, 10, 12, 14):
                _, p = uleb(b, p, end)
                _, p = uleb(b, p, end)
            else:
                raise Unread(f"instruction 0xfc {sub}")
        else:
            raise Unread(f"instruction 0x{op:02x}")
        if p > end:
            raise Unread("an instruction runs past its body")
        out.append((at, op, before))
    if depth != -1:
        raise Unread("a body whose nesting does not close with its last instruction")
    return out


class Module:
    def __init__(self, path):
        self.bytes = open(path, "rb").read()
        self.bodies = function_bodies(self.bytes)
        self.read = {}
        # None, or why the module is not one the rules are about: a load or a
        # store must be an access of the one memory, unshared, 32-bit.
        mems = memories(self.bytes)
        self.memory = (f"the module has {len(mems)} memories" if len(mems) != 1
                       else "its memory is shared" if mems[0][0] & 0x02
                       else "its memory is addressed by 64-bit offsets" if mems[0][0] & 0x04
                       else None)

    def body(self, func):
        """The function's instructions, and the offsets they start at."""
        if func not in self.read:
            start, end = self.bodies[func]
            ins = instructions(self.bytes, start, end)
            self.read[func] = (ins, {i[0] for i in ins})
        return self.read[func]


# A proposal: (kind, function, access, [the strings that end in the access],
# the string after it or None). The names are for the refusals.
NAMES = {"WINDOW": ["window"], "FRESH": ["allocation", "between", "access run"],
         "LOOP": ["hold", "capture", "up to the test", "test and access"]}


def parse(path, kind, parts):
    """[(label, [proposal])] of a dump, in order."""
    mods = []
    for line in open(path, encoding="utf-8"):
        t = line.split()
        if not t:
            continue
        if t[0] == "MODULE":
            mods.append((" ".join(t[1:]), []))
        elif t[0] == kind:
            if not mods:
                sys.exit(f"FAIL: {path} starts with a {kind} line, not a MODULE line")
            cut = line.split("|")
            head = cut[0].split()
            try:
                func, access = int(head[1]), int(head[2])
                if kind == "WINDOW":
                    strings = [bytes(int(x) for x in head[3:])]
                else:
                    strings = [bytes(int(x) for x in c.split()) for c in cut[1:]]
            except (ValueError, IndexError):
                sys.exit(f"FAIL: {mods[-1][0]}: a {kind} line is not numbers: {line.strip()[:120]}")
            if len(strings) != parts:
                sys.exit(f"FAIL: {mods[-1][0]}: a {kind} line holds {len(strings)} byte string(s), not {parts}: {line.strip()[:120]}")
            rest = strings.pop() if kind == "LOOP" else None
            mods[-1][1].append((kind, func, access, strings, rest))
    return mods


def judge(m, prop):
    """None, or why the proposal is not what it says."""
    kind, func, access, strings, rest = prop
    if func >= len(m.bodies):
        return f"the module has no defined function {func}"
    ins, starts = m.body(func)
    start, end = m.bodies[func]
    if access >= len(ins):
        return f"function {func} has no instruction {access}"
    at, op, depth = ins[access]
    if not 0x28 <= op <= 0x3E:
        return f"instruction {access} is not a load or a store"
    hi = ins[access + 1][0] if access + 1 < len(ins) else end
    lo = hi - sum(len(s) for s in strings)
    if lo == hi:
        return "no bytes before the access's end"
    if lo < start:
        return "starts before the function's first instruction"
    cut = lo
    for name, s in zip(NAMES[kind], strings):
        if cut not in starts:
            return f"the {name} string is cut inside an instruction (offset {cut})"
        if m.bytes[cut:cut + len(s)] != s:
            return f"the {name} string is not the module's bytes at offset {cut}"
        cut += len(s)
    if rest is not None:
        if m.bytes[hi:hi + len(rest)] != rest:
            return f"the string after the access is not the module's bytes at offset {hi}"
        if hi + len(rest) > end or (hi + len(rest) != end and hi + len(rest) not in starts):
            return f"the string after the access ends inside an instruction or past the body (offset {hi + len(rest)})"
    if kind == "WINDOW":
        run = [i for i in ins if lo <= i[0] <= at]
        if run[0][2] != depth or any(i[2] < depth for i in run):
            return "the window does not stay at the access's control depth"
    return None


def verdict(manifest, dumps):
    """(failures, modules, proposals per kind) for the dumps held to the manifest."""
    fails, counts, cache = [], {}, {}
    for kind, mods in dumps.items():
        counts[kind] = sum(len(ps) for _, ps in mods)
        if [l for l, _ in mods] != [l for l, _ in manifest]:
            fails.append(f"the {kind} dump names {len(mods)} module(s) that are not the {len(manifest)} swept, in order: a module went unread")
            continue
        for (label, path), (_, props) in zip(manifest, mods):
            if not props:
                continue
            try:
                if path not in cache:
                    cache[path] = Module(path)
                m = cache[path]
                if m.memory:
                    fails.append(f"{label}: {m.memory}: the rules are about a module with one unshared 32-bit memory")
                    continue
                for prop in props:
                    why = judge(m, prop)
                    if why:
                        fails.append(f"{label}: {prop[0]} {prop[1]} {prop[2]}: {why}")
            except Unread as e:
                fails.append(f"{label}: this script's decoder does not read the module ({e}); no length is guessed")
    counts["READ"] = len(cache)
    return fails, counts


manifest = [tuple(l.rstrip("\n").split("\t")) for l in open(manifest_path, encoding="utf-8") if l.strip()]
if not manifest or any(len(r) != 2 for r in manifest):
    sys.exit(f"FAIL: {manifest_path} is not `<label>\\t<path>` lines, or is empty")
dumps = {"WINDOW": parse(window_path, "WINDOW", 1), "FRESH": parse(fresh_path, "FRESH", 3), "LOOP": parse(loops_path, "LOOP", 5)}
fails, counts = verdict(manifest, dumps)

if not forge_dir:
    for f in fails[:10]:
        print("FAIL: " + f)
    if fails:
        print(f"shipped slices: {len(fails)} refusal(s)")
        sys.exit(1)
    print(f"shipped slices: {len(manifest)} module(s); {counts['WINDOW']} window(s), {counts['FRESH']} fresh-block proposal(s) "
          f"and {counts['LOOP']} loop proposal(s) re-derived from the module's bytes by a second decoder: "
          f"every string is an instruction run of the function it names, and each of the {counts['READ']} module(s) "
          f"that hold a proposal has one unshared 32-bit memory")
    sys.exit(0)

# --forge: the control first (the untouched dumps must pass), then one altered
# proposal per forgery.
if fails:
    sys.exit(f"FAIL: the untouched dumps are refused, so there is nothing to forge from: {fails[0]}")
paths = dict(manifest)


def line(prop):
    kind, func, access, strings, rest = prop
    text = lambda s: "".join(f" {x}" for x in s)
    if kind == "WINDOW":
        return f"WINDOW {func} {access}{text(strings[0])}"
    tail = strings + ([rest] if rest is not None else [])
    return f"{kind} {func} {access} 0 |" + " |".join(text(s) for s in tail)


def write(name, kind, label, old, new, expect, drop_module=False):
    """The three dumps with every module named and the proposals of `label`
    alone (the other modules' are left out: they were judged above, and a
    forged input is refused for its one altered proposal)."""
    d = os.path.join(forge_dir, name)
    os.makedirs(d)
    for k, fname in (("WINDOW", "window-dump"), ("FRESH", "fresh-dump"), ("LOOP", "loops-dump")):
        with open(os.path.join(d, fname), "w", encoding="utf-8") as f:
            for l, props in dumps[k]:
                if drop_module and k == kind and l == label:
                    continue
                f.write(f"MODULE {l}\n")
                if l != label:
                    continue
                for p in props:
                    f.write(line(new if (k == kind and p is old) else p) + "\n")
    with open(os.path.join(d, "expect"), "w", encoding="utf-8") as f:
        f.write(expect + "\n")


loaded = {}


def module(label):
    if label not in loaded:
        loaded[label] = Module(paths[label])
    return loaded[label]


def first(kind, ok):
    for label, props in dumps[kind]:
        for p in props:
            if ok(module(label), p):
                return label, p
    return None, None


def long_first(m, p):
    """The proposal's first string starts with an instruction of two bytes or more."""
    ins, starts = m.body(p[1])
    hi = ins[p[2] + 1][0] if p[2] + 1 < len(ins) else m.bodies[p[1]][1]
    lo = hi - sum(len(s) for s in p[3])
    return lo + 1 not in starts and len(p[3][0]) > 2


def long_cut(i):
    """The proposal's string `i` ends with an instruction of two bytes or more."""
    def ok(m, p):
        ins, starts = m.body(p[1])
        hi = ins[p[2] + 1][0] if p[2] + 1 < len(ins) else m.bodies[p[1]][1]
        cut = hi - sum(len(s) for s in p[3]) + sum(len(s) for s in p[3][:i + 1])
        return len(p[3][i]) > 1 and cut - 1 not in starts
    return ok


made, missing = 0, []
label, p = first("WINDOW", long_first)
if p:
    kind, func, access, strings, rest = p
    write("window-start-inside", kind, label, p, (kind, func, access, [strings[0][1:]], rest), "is cut inside an instruction")
    s = strings[0]
    write("window-byte-changed", kind, label, p, (kind, func, access, [s[:1] + bytes([(s[1] + 1) % 256]) + s[2:]], rest), "is not the module's bytes")
    made += 2
    # Another function of the module whose body does not hold the window's
    # bytes anywhere: naming it is false whatever this script's decoder reads.
    m = module(label)
    others = [g for g, (a, e) in enumerate(m.bodies) if g != func and s not in m.bytes[a:e]]
    if others:
        write("window-other-function", kind, label, p, (kind, others[0], access, strings, rest), f"WINDOW {others[0]} {access}: ")
        made += 1
else:
    missing.append("window")
label, p = first("FRESH", long_cut(0))
if p:
    kind, func, access, (pre, mid, tail), rest = p
    write("fresh-cut-inside", kind, label, p, (kind, func, access, [pre[:-1], pre[-1:] + mid, tail], rest), "the between string is cut inside an instruction")
    write("fresh-byte-changed", kind, label, p, (kind, func, access, [pre, mid, tail[:-1] + bytes([(tail[-1] + 1) % 256])], rest), "is not the module's bytes")
    write("fresh-module-unread", kind, label, p, p, "a module went unread", drop_module=True)
    made += 3
else:
    missing.append("fresh-block")
label, p = first("LOOP", long_cut(1))
if p:
    kind, func, access, (hold, pre, mid, ht), rest = p
    write("loop-cut-inside", kind, label, p, (kind, func, access, [hold, pre[:-1], pre[-1:] + mid, ht], rest), "the up to the test string is cut inside an instruction")
    made += 1
    label, q = first("LOOP", lambda m, x: len(x[4]) > 0)
    if q:
        kind, func, access, strings, rest = q
        write("loop-rest-changed", kind, label, q, (kind, func, access, strings, bytes([(rest[0] + 1) % 256]) + rest[1:]), "the string after the access is not the module's bytes")
        made += 1
else:
    missing.append("loop")


# Two forged MODULES, the dumps untouched: the first swept module that holds a
# proposal and defines its one memory, with a second memory added to its
# memory section, and with that memory made shared. Functions and instructions
# keep their indices, so every proposal is still where it says it is: the one
# false thing is how many memories the module has, or of what kind.
def leb(n):
    out = bytearray()
    while True:
        x, n = n & 0x7F, n >> 7
        out.append(x | (0x80 if n else 0))
        if not n:
            return bytes(out)


def with_memory_section(b, payload):
    """The module with its memory section's payload replaced; None without one."""
    p = 8
    while p < len(b):
        at, sid = p, b[p]
        size, p = uleb(b, p + 1, len(b))
        if sid == 5:
            return b[:at] + b"\x05" + leb(len(payload)) + payload + b[p + size:]
        p += size
    return None


def write_module(name, label, forged, expect):
    write(name, None, label, None, None, expect)
    d = os.path.join(forge_dir, name)
    path = os.path.join(d, "module.wasm")
    with open(path, "wb") as f:
        f.write(forged)
    with open(os.path.join(d, "manifest"), "w", encoding="utf-8") as f:
        for l, p in manifest:
            f.write(f"{l}\t{path if l == label else p}\n")


done = False
for label in dict.fromkeys(l for k in dumps for l, props in dumps[k] if props):
    m = module(label)
    mems = memories(m.bytes)
    if len(mems) != 1 or mems[0][0] & 0x06:
        continue
    flags, lo, hi = mems[0]
    entry = bytes([flags]) + leb(lo) + (leb(hi) if hi is not None else b"")
    two = with_memory_section(m.bytes, leb(2) + entry + b"\x00\x01")
    shared = with_memory_section(m.bytes, leb(1) + bytes([0x03]) + leb(lo) + leb(hi if hi is not None else max(lo, 1)))
    if two is None or shared is None:
        continue  # the memory is imported: there is no memory section to alter
    write_module("module-second-memory", label, two, "the module has 2 memories")
    write_module("module-shared-memory", label, shared, "its memory is shared")
    made += 2
    done = True
    break
if not done:
    missing.append("module that defines its one memory")
print(f"forged {made} input(s) under {forge_dir}" + (f"; no {', '.join(missing)} proposal to forge from" if missing else ""))
if missing and os.environ.get("SLICES_FORGE_PARTIAL") != "1":
    sys.exit(1)
PY

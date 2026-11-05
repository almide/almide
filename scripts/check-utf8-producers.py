#!/usr/bin/env python3
"""UTF-8 producer ledger gate (#4005 A4; proofs/TRUSTED_BASE.md, "String
representation invariants").

The wasm leg's string readers (the case mappers, trim, the JSON parser, the
regex engine, the copy loops of first/last/pad/reverse/chars/replace) step
through a String block by the width its lead byte announces and size some
output blocks from those widths. They stay inside their blocks only if every
String block holds valid UTF-8. That invariant is trusted, not proven; this
gate keeps the trust honest.

Every operation that can put bytes into a String block is a row of
proofs/utf8-producers.toml. The ROW SET IS DERIVED from declarations, never
typed, so a new producer without a row fails CI:

  [[fn]]     every public stdlib function whose declared type hands out a
             String: in its return type, or as an argument of a callback it
             calls. Names come from the committed signature indexes of
             docs/stdlib/*.md (CI keeps them fresh); each type is read from the
             function's declaration in stdlib/*.almd (the index drops an effect
             fn's Result[_, String] and every bound). Record, variant and alias
             names are expanded through the `type` declarations of the indexes
             and the sources; a name declared nowhere the gate can read, and
             every generic bound, needs a [[leaf]] row saying whether it can
             hold a String.
  [[op]]     every host op number the guest can call through `almide.fs_call`:
             the emitter's OP_* constants (crates/almide-wasm/src), the
             capability table `op_caps` (cert_project.rs), the embedded host's
             OP_* constants and OP_FIRST..OP_LAST ranges
             (crates/almide-wasm-run/src), and the stock shim's served sets
             (P1_SERVED_OPS, FS_SERVICE_OPS, PROC_OPS in crates/almide-wasi/src).
             Each row has a verdict per leg: `embedded` (the core-module host,
             almide-wasm-run), `p1` (a stock WASI preview-1 runtime through the
             almide-wasi shims) and `component` (the WASI 0.3 component shim).
  [[prim]]   every `prim.*` operation the self-hosted stdlib calls.
  [[mint]]   every stdlib source that mints String blocks itself
             (`prim.alloc_str`): the self-hosted bodies, public or reached only
             through lowering (concatenation, display, quoting).
  [[helper]] every runtime helper the wasm emitter declares (`F_*` in
             crates/almide-wasm/src/lib.rs): concatenation, interpolation,
             slicing, the host-read transport.

The verdicts:
  validates     checks (or lossily repairs) bytes it did not make;
  construction  builds from valid Strings and ASCII, cutting only at character
                boundaries;
  host          (fn/prim) its String comes from the host ops named in `ops`,
                whose rows carry the verdicts and the evidence;
  obligation    (prim/helper) writes or moves raw bytes; whether a String
                block is valid afterwards is the obligation of the [[mint]] /
                [[op]] row that called it, which carries the evidence;
  typed         (op leg) the text crosses an interface whose type admits only
                valid UTF-8 (a Component Model `string`, a Rust `String`)
                and nothing on the guest side re-reads it as bytes first;
  message       (op leg) its only String output is host-authored: static
                ASCII, the guest's own valid path, a Rust String;
  none          puts no bytes into a String block;
  unserved      (op leg) the leg does not serve the op at all;
  open          CAN produce invalid UTF-8 today: a named exception of the
                trusted-base row, with the `fix` that closes it. The open list
                only shrinks: `open_ceiling` equals the number of open rows.

A validates/construction verdict cites EVIDENCE (`file::test`), checked
statically here (whether it PASSES is the test runs' job: `almide test spec/`
on native and wasm, `cargo test`):
  - an Almide test (`file.almd::test name`) must exist, check its output with
    the file's own UTF-8 oracle `utf8_ok`, call the producer (an [[fn]] row; a
    [[mint]] / [[helper]] row names what reaches it in `reached_by`), and FEED
    INVALID BYTES: its block, or a fn of the same file it calls, holds a byte
    no UTF-8 contains (0xC0, 0xC1, 0xF5..0xFF) or a surrogate lead pair
    (0xED 0xA0..0xBF);
  - a Rust test (`file.rs::fn_name`) must exist and hold such a byte in its
    body or a helper fn of its file it calls; for an [[fn]] row the file must
    name the producer.

  --self-test   the negative controls (kept as the drill): forged trees and
                ledgers that must each be refused.
  --bless       print the TOML of every missing row to stdout.
"""
import os
import re
import shutil
import sys
import tempfile
import tomllib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))
LEDGER = 'proofs/utf8-producers.toml'

SIG = re.compile(r'^(?:effect )?([a-z_0-9]+)\.([a-z_0-9]+)\((.*)\) -> (.*?)(?:\s+\(deprecated[^)]*\))?\s*$')
INDEX_TYPE = re.compile(r'^type ([a-z_0-9]+)\.([A-Z][A-Za-z0-9]*)(?:\[[^\]]*\])? = (.*)$')
SRC_TYPE = re.compile(r'^\s*(?:pub |mod |opaque |local )*type ([A-Z][A-Za-z0-9]*)(?:\[[^\]]*\])?(?:\s*:[^=]*)?\s*=\s*(.*)$')
SRC_FN = re.compile(r'^[ \t]*(?:(?:pub|effect|local|mod|async)[ \t]+)*fn[ \t]+([a-z_0-9]+)[ \t]*(?=[\[(])', re.M)
# A type name; a `Name(` is a variant constructor, never a type.
TYPE_NAME = re.compile(r'\b(?:([a-z_0-9]+)\.)?([A-Z][A-Za-z0-9]*)\b(?!\s*\()')
# A byte no well-formed UTF-8 contains (RFC 3629: C0, C1, F5..FF), or a
# surrogate's lead pair ED A0..BF, spelled as a literal.
INVALID_BYTE = re.compile(
    r'\\x(?:c[01]|f[5-9a-f])|0x(?:c[01]|f[5-9a-f])(?![0-9a-f])'
    r'|\\xed\\x[ab][0-9a-f]|0xed\s*,\s*0x[ab][0-9a-f]',
    re.I,
)
HOWS = {
    'fn': {'validates', 'construction', 'host', 'open'},
    'op': {'validates', 'construction', 'typed', 'message', 'none', 'unserved', 'open'},
    'prim': {'none', 'obligation', 'host', 'open'},
    'mint': {'validates', 'construction', 'open'},
    'helper': {'validates', 'construction', 'obligation', 'none', 'open'},
}
LEGS = ('embedded', 'p1', 'component')
EVIDENCED = {'validates', 'construction'}


def read(root, rel):
    with open(os.path.join(root, rel), encoding='utf-8') as f:
        return f.read()


def files(root, rel, ext):
    out = []
    for d, _, fs in os.walk(os.path.join(root, rel)):
        for f in fs:
            if f.endswith(ext):
                out.append(os.path.relpath(os.path.join(d, f), root))
    return sorted(out)


def close_of(src, at):
    """Index just past the bracket that closes the one opening at `at`."""
    pairs = {'(': ')', '[': ']', '{': '}'}
    stack = []
    for i in range(at, len(src)):
        c = src[i]
        if c in pairs:
            stack.append(pairs[c])
        elif stack and c == stack[-1]:
            stack.pop()
            if not stack:
                return i + 1
    return len(src)


# ── the derived row sets ─────────────────────────────────────────────────


def index_blocks(root):
    """The lines of every generated signature-index block."""
    out = []
    for rel in files(root, 'docs/stdlib', '.md'):
        on, buf = False, []
        for line in read(root, rel).splitlines():
            if 'BEGIN GENERATED SIGNATURE INDEX' in line:
                on = True
            elif 'END GENERATED SIGNATURE INDEX' in line:
                on = False
            elif on:
                buf.append(line)
        out.append(buf)
    return out


def strip_cases(rhs):
    """A variant's payload-free case names are constructors, not types."""
    if '|' not in rhs:
        return rhs
    return re.sub(r'(^|\|)\s*[A-Z][A-Za-z0-9]*\b(?!\s*[\[(])', r'\1', rhs)


def declared_types(root):
    """{(module, Name): rhs} from the type indexes and the stdlib sources."""
    types = {}
    for lines in index_blocks(root):
        for line in lines:
            m = INDEX_TYPE.match(line)
            if m:
                types[(m.group(1), m.group(2))] = strip_cases(m.group(3))
    for rel in files(root, 'stdlib', '.almd'):
        module = os.path.basename(rel)[:-5].split('_')[0]
        for line in read(root, rel).splitlines():
            m = SRC_TYPE.match(line)
            if m:
                types.setdefault((module, m.group(1)), strip_cases(m.group(2)))
    return types


def source_decls(root):
    """{(file stem, fn name): (generics, params, ret, is_effect)} of every fn
    the stdlib sources declare."""
    out = {}
    for rel in files(root, 'stdlib', '.almd'):
        src = read(root, rel)
        stem = os.path.basename(rel)[:-5]
        for m in SRC_FN.finditer(src):
            i = m.end()
            generics = ''
            if src[i] == '[':
                j = close_of(src, i)
                generics, i = src[i + 1:j - 1], j
            if i >= len(src) or src[i] != '(':
                continue
            j = close_of(src, i)
            params, rest, ret = src[i + 1:j - 1], src[j:], ''
            r = re.match(r'\s*->\s*', rest)
            if r:
                k, depth = r.end(), 0
                while k < len(rest):
                    c = rest[k]
                    if c in '([':
                        depth += 1
                    elif c in ')]':
                        depth -= 1
                    elif depth == 0 and (c in '\n{' or (c == '=' and rest[k + 1:k + 2] not in ('>', '='))):
                        break
                    k += 1
                ret = rest[r.end():k].strip()
            head = src[m.start():m.end()]
            out[(stem, m.group(1))] = (generics, params, ret, bool(re.search(r'\beffect\b', head)))
    return out


def signatures(root):
    """{'module.fn': (generics, params, ret, is_effect)} of the public surface:
    the names the committed index lists, typed from their declaration in
    `<module>.almd` (else `<module>_*.almd`), else from the index line."""
    decls = source_decls(root)
    sigs = {}
    for lines in index_blocks(root):
        for line in lines:
            m = SIG.match(line)
            if not m or m.group(2).startswith('__'):
                continue
            mod, fn = m.group(1), m.group(2)
            hit = decls.get((mod, fn))
            if hit is None:
                hit = next((v for (stem, name), v in sorted(decls.items())
                            if name == fn and stem.startswith(mod + '_')), None)
            sigs[f'{mod}.{fn}'] = hit or ('', m.group(3), m.group(4), line.startswith('effect '))
    return sigs


def bounds(generics):
    """{type var: [bound names]} of a `[T: Text, M: HttpMessage]` list."""
    out = {}
    for part in re.split(r',(?![^\[]*\])', generics):
        if ':' in part:
            var, bs = part.split(':', 1)
            out[var.strip()] = [b.strip() for b in bs.split('+') if b.strip()]
    return out


def callback_args(params):
    """The argument lists of every function type in `params`."""
    out = []
    for m in re.finditer(r'\(', params):
        j = close_of(params, m.start())
        if re.match(r'\s*->', params[j:]):
            out.append(params[m.start() + 1:j - 1])
    return out


def lookup(types, q, name, module):
    for cand in ([(q, name)] if q else [(module, name)]) + sorted(k for k in types if k[1] == name):
        if cand in types:
            return cand
    return None


def with_aliases(params, module, types, depth=3):
    """`params` plus the right-hand side of every declared type it names that
    is a function type (`f: HttpHandler` is `(HttpRequest) -> HttpResponse`)."""
    text = params
    for _ in range(depth):
        more = ''
        for m in TYPE_NAME.finditer(text):
            key = lookup(types, m.group(1), m.group(2), module)
            if key and '->' in types[key] and types[key] not in text:
                more += ' ' + types[key]
        if not more:
            break
        text += more
    return text


def carries_string(text, module, types, leaves, unknown, seen=(), bound_vars=None):
    """Whether a type expression can hold a String. An unbounded type variable
    (one capital letter) is parametric: it passes a caller's value through and
    makes no bytes. A bounded one is what its bounds admit."""
    bound_vars = bound_vars or {}
    found = False
    for m in TYPE_NAME.finditer(text):
        q, name = m.group(1), m.group(2)
        if name in bound_vars:
            for b in bound_vars[name]:
                if b in leaves:
                    found = found or leaves[b]
                else:
                    unknown.add(b)
            continue
        if len(name) == 1:
            continue
        key = lookup(types, q, name, module)
        if key is not None:
            if key not in seen and carries_string(types[key], key[0], types, leaves, unknown,
                                                  seen + (key,), bound_vars):
                found = True
        elif name in leaves:
            found = found or leaves[name]
        else:
            unknown.add(name)
    return found


def string_producers(root, leaves):
    """({fid: why}, {fid: is_effect}, unknown names)."""
    types = declared_types(root)
    unknown = set()
    out, effect = {}, {}
    for fid, (generics, params, ret, eff) in signatures(root).items():
        mod = fid.split('.')[0]
        bv = bounds(generics)
        why = []
        if carries_string(ret, mod, types, leaves, unknown, (), bv):
            why.append(f'returns {ret}')
        for args in callback_args(with_aliases(params, mod, types)):
            if carries_string(args, mod, types, leaves, unknown, (), bv):
                why.append(f'hands its callback ({args})')
        if why:
            out[fid], effect[fid] = '; '.join(why), eff
    return out, effect, unknown


def host_ops(root):
    """Every fs_call op number a declaration names."""
    ops = set()
    const = re.compile(r'const OP_[A-Z0-9_]+: i32 = (\d+);')
    for crate in ('crates/almide-wasm/src', 'crates/almide-wasm-run/src'):
        for rel in files(root, crate, '.rs'):
            src = read(root, rel)
            ops.update(int(n) for n in const.findall(src))
            first = re.search(r'const OP_FIRST: i32 = (\d+);', src)
            last = re.search(r'const OP_LAST: i32 = (\d+);', src)
            if first and last:
                ops.update(range(int(first.group(1)), int(last.group(1)) + 1))
    caps = read(root, 'crates/almide-wasm/src/cert_project.rs')
    body = caps[caps.index('pub fn op_caps'):]
    body = body[:body.index('\n}\n')]
    for arm in re.findall(r'^\s*([0-9][0-9 |.=]*)=>', body, re.M):
        for part in arm.split('|'):
            part = part.strip()
            if '..=' in part:
                a, b = part.split('..=')
                ops.update(range(int(a), int(b) + 1))
            elif part:
                ops.add(int(part))
    lib = read(root, 'crates/almide-wasi/src/lib.rs')
    served = lib[lib.index('pub const P1_SERVED_OPS'):]
    served = re.sub(r'//[^\n]*', '', served[served.index('['):served.index('];')])
    ops.update(int(n) for n in re.findall(r'\b\d+\b', served))
    fsv = read(root, 'crates/almide-wasi/src/fs_service.rs')
    table = fsv[fsv.index('pub const FS_SERVICE_OPS'):]
    ops.update(int(n) for n in re.findall(r'\((\d+),', table[:table.index('];')]))
    proc_ = re.search(r'PROC_OPS: [^=]+= (\d+)..=(\d+);', read(root, 'crates/almide-wasi/src/proc_service.rs'))
    ops.update(range(int(proc_.group(1)), int(proc_.group(2)) + 1))
    return ops


def prims(root):
    out = set()
    for rel in files(root, 'stdlib', '.almd'):
        out.update(re.findall(r'\bprim\.([a-z_0-9]+)\(', read(root, rel)))
    return out


def mints(root):
    """The stdlib sources that mint String blocks (`prim.alloc_str`). The
    prim's own definition (stdlib/prim.almd) is the [[prim]] row."""
    return {rel for rel in files(root, 'stdlib', '.almd')
            if re.search(r'\bprim\.alloc_str\(', read(root, rel)) and not rel.endswith('/prim.almd')}


def helpers(root):
    return set(re.findall(r'const (F_[A-Z0-9_]+): u32', read(root, 'crates/almide-wasm/src/lib.rs')))


# ── evidence ─────────────────────────────────────────────────────────────


def balanced(src, at):
    return src[at:close_of(src, at)]


def almide_test(src, name):
    m = re.search(r'^test "' + re.escape(name) + r'" \{', src, re.M)
    return balanced(src, m.end() - 1) if m else None


def almide_fns(src):
    """{name: text} of the file's top-level fns (to follow the corpus)."""
    out = {}
    for m in re.finditer(r'^(?:effect )?fn ([a-z_0-9]+)\b', src, re.M):
        rest = src[m.start():]
        nxt = re.search(r'\n(?=(?:effect )?fn |test ")', rest)
        out[m.group(1)] = rest[:nxt.start()] if nxt else rest
    return out


def rust_fns(src):
    out = {}
    for m in re.finditer(r'\bfn ([a-z_0-9]+)\s*[(<]', src):
        at = src.find('{', m.end())
        if at >= 0:
            out[m.group(1)] = balanced(src, at)
    for m in re.finditer(r'\b(?:const|static) ([A-Z_0-9]+)\s*:[^=]*=', src):
        end = src.find(';\n', m.end())
        out[m.group(1)] = src[m.start():end if end >= 0 else len(src)]
    return out


def reaches(block, helpers_, found, depth=3, seen=()):
    """Whether `found` holds of `block` or of a same-file fn it names
    (followed `depth` calls deep)."""
    if found(block):
        return True
    if depth == 0:
        return False
    for name, body in helpers_.items():
        if name in seen or body == block:
            continue
        if re.search(r'\b' + re.escape(name) + r'\b', block):
            if reaches(body, helpers_, found, depth - 1, seen + (name,)):
                return True
    return False


def reaches_invalid(block, helpers_):
    return reaches(block, helpers_, lambda t: INVALID_BYTE.search(t) is not None)


def calls(text, fid):
    if fid in ('${}', '+'):
        return ('${' in text) if fid == '${}' else re.search(r'\S \+ \S', text) is not None
    m, f = fid.split('.', 1)
    return re.search(r'\b' + re.escape(m) + r'\.' + re.escape(f) + r'\b', text) is not None


def check_evidence(root, ev, reach, cache):
    """Problems with one evidence pointer. `reach` is the producer names one
    of which the test must call (empty: no call is required)."""
    if '::' not in ev:
        return [f'evidence `{ev}` is not file::test']
    rel, name = ev.split('::', 1)
    if not os.path.exists(os.path.join(root, rel)):
        return [f'evidence file {rel} does not exist']
    src = cache.setdefault(rel, read(root, rel))
    if rel.endswith('.almd'):
        block = almide_test(src, name)
        if block is None:
            return [f'no test "{name}" in {rel}']
        probs = []
        if reach and not any(calls(block, r) for r in reach):
            probs.append(f'test "{name}" in {rel} never reaches {" / ".join(reach)}')
        fns = {k: v for k, v in almide_fns(src).items() if k != 'utf8_ok'}
        if not reaches(block, fns, lambda t: 'utf8_ok(' in t):
            probs.append(f'test "{name}" in {rel} does not check its output with utf8_ok')
        if not reaches_invalid(block, fns):
            probs.append(f'test "{name}" in {rel} feeds no invalid UTF-8 byte')
        return probs
    if rel.endswith('.rs'):
        fns = rust_fns(src)
        body = fns.get(name)
        if body is None or not re.search(r'\bfn ' + re.escape(name) + r'\b', src):
            return [f'no fn {name} in {rel}']
        probs = []
        if reach and not any(calls(src, r) for r in reach):
            probs.append(f'{rel} never names {" / ".join(reach)}')
        if not reaches_invalid(body, fns):
            probs.append(f'{rel}::{name} feeds no invalid UTF-8 byte')
        return probs
    return [f'evidence {rel} is neither an .almd test nor a .rs test']


# ── the check ────────────────────────────────────────────────────────────


def as_list(v):
    return [v] if isinstance(v, str) else list(v or [])


def check(root, ledger_text):
    """(problems, open rows, number of [[fn]] producers)."""
    led = tomllib.loads(ledger_text)
    probs, opens, cache = [], [], {}
    leaves = {}
    for row in led.get('leaf', []):
        leaves[row['name']] = bool(row['string'])
        if not row.get('why'):
            probs.append(f'leaf {row["name"]}: needs a why')
    want_fns, effect, unknown = string_producers(root, leaves)
    for name in sorted(unknown):
        probs.append(f'type {name}: declared nowhere the gate reads; add a [[leaf]] row (string = true/false)')
    derived = {
        'fn': set(want_fns), 'op': host_ops(root), 'prim': prims(root),
        'mint': mints(root), 'helper': helpers(root),
    }
    ops_rows = {r['id'] for r in led.get('op', [])}
    fn_rows = {r['id'] for r in led.get('fn', [])}

    def ev_check(rid, row, reach):
        evs = as_list(row.get('evidence'))
        if not evs:
            probs.append(f'{rid}: a validates/construction verdict without evidence')
        for ev in evs:
            probs.extend(f'{rid}: {p}' for p in check_evidence(root, ev, reach, cache))

    for kind in ('fn', 'op', 'prim', 'mint', 'helper'):
        rows = led.get(kind, [])
        seen = set()
        for row in rows:
            rid = row.get('id')
            if rid in seen:
                probs.append(f'{kind} {rid}: duplicate row')
            seen.add(rid)
            if rid not in derived[kind]:
                probs.append(f'{kind} {rid}: stale row (nothing declares it any more)')
        for rid in sorted(derived[kind] - seen, key=str):
            probs.append(f'{kind} {rid}: no ledger row — a new String producer must say how its output is valid')
        for row in rows:
            rid = f'{kind} {row.get("id")}'
            verdicts = {leg: row.get(leg, '') for leg in LEGS} if kind == 'op' else {'how': row.get('how', '')}
            for field, how in verdicts.items():
                if how not in HOWS[kind]:
                    probs.append(f'{rid}: {field} = "{how}" is not one of {sorted(HOWS[kind])}')
            values = set(verdicts.values())
            if 'open' in values:
                legs = [k for k, v in verdicts.items() if v == 'open']
                opens.append(rid + (f' ({", ".join(legs)})' if kind == 'op' else ''))
                if not row.get('fix'):
                    probs.append(f'{rid}: an open verdict names the fix that closes it')
            if kind == 'op' and not row.get('name'):
                probs.append(f'{rid}: needs `name`, the call it serves')
            unevidenced = values & {'obligation', 'typed', 'message', 'none'}
            if unevidenced and not row.get('why'):
                probs.append(f'{rid}: a {"/".join(sorted(unevidenced))} verdict says why')
            if 'host' in values:
                ops = row.get('ops', [])
                if not ops:
                    probs.append(f'{rid}: a host verdict names its ops')
                for op in ops:
                    if op not in ops_rows:
                        probs.append(f'{rid}: op {op} has no [[op]] row')
                if kind == 'fn' and not effect.get(row['id']):
                    probs.append(f'{rid}: a host verdict on a fn that is not an effect fn')
            if values & EVIDENCED:
                if kind == 'fn':
                    ev_check(rid, row, [row['id']])
                elif kind in ('mint', 'helper'):
                    reach = as_list(row.get('reached_by'))
                    if not reach:
                        probs.append(f'{rid}: names what reaches it (reached_by)')
                    for r in reach:
                        if r not in ('${}', '+') and r not in fn_rows:
                            probs.append(f'{rid}: reached_by {r} is not an [[fn]] row')
                    ev_check(rid, row, reach)
                else:
                    ev_check(rid, row, [])
    m = re.search(r'^open_ceiling = (\d+)', ledger_text, re.M)
    ceiling = int(m.group(1)) if m else -1
    if len(opens) > ceiling:
        probs.append(f'open rows {len(opens)} > open_ceiling {ceiling}: the open list only shrinks')
    elif len(opens) < ceiling:
        probs.append(f'open rows {len(opens)} < open_ceiling {ceiling}: lower the ceiling (its own commit)')
    return probs, opens, len(want_fns)


def bless(root, ledger_text):
    led = tomllib.loads(ledger_text) if ledger_text else {}
    leaves = {r['name']: bool(r['string']) for r in led.get('leaf', [])}
    want, effect, unknown = string_producers(root, leaves)
    for n in sorted(unknown):
        print(f'[[leaf]]\nname = "{n}"\nstring = false\nwhy = ""\n')
    have = {r['id'] for r in led.get('fn', [])}
    for fid in sorted(set(want) - have):
        print(f'[[fn]]\nid = "{fid}"  # {"effect; " if effect[fid] else ""}{want[fid]}\nhow = ""\n')
    for kind, ids in (('op', host_ops(root)), ('prim', prims(root)), ('mint', mints(root)), ('helper', helpers(root))):
        have = {r['id'] for r in led.get(kind, [])}
        for rid in sorted(ids - have, key=str):
            key = rid if isinstance(rid, int) else f'"{rid}"'
            print(f'[[{kind}]]\nid = {key}\n' + ('name = ""\nembedded = ""\np1 = ""\ncomponent = ""\n'
                                                   if kind == 'op' else 'how = ""\n'))


# ── the negative controls ────────────────────────────────────────────────

TREE = ('docs/stdlib', 'stdlib', 'spec/stdlib', 'tests', 'crates/almide-wasm/src',
        'crates/almide-wasm-run/src', 'crates/almide-wasi/src')


def self_test(root, ledger_text):
    probs, _, _ = check(root, ledger_text)
    assert not probs, 'self-test needs the committed tree to pass:\n  ' + '\n  '.join(probs)
    led = tomllib.loads(ledger_text)
    done = []
    tmp = tempfile.mkdtemp()
    try:
        for rel in TREE:
            shutil.copytree(os.path.join(root, rel), os.path.join(tmp, rel), dirs_exist_ok=True,
                            ignore=shutil.ignore_patterns('target', 'golden', '*.wasm'))
        def refused(why, expect, ledger=ledger_text, edits=()):
            """Forge the tree and/or the ledger, require a problem naming
            `expect`, then put every touched file back."""
            saved = {}
            for rel, fn in edits:
                path = os.path.join(tmp, rel)
                saved.setdefault(rel, read(tmp, rel) if os.path.exists(path) else None)
                with open(path, 'w', encoding='utf-8') as f:
                    f.write(fn(saved[rel] or ''))
            try:
                p, _, _ = check(tmp, ledger)
            finally:
                for rel, text in saved.items():
                    path = os.path.join(tmp, rel)
                    if text is None:
                        os.remove(path)
                    else:
                        with open(path, 'w', encoding='utf-8') as f:
                            f.write(text)
            hit = [x for x in p if expect in x]
            assert hit, f'NOT refused for its own reason: {why} (wanted `{expect}`, got {p[:3]})'
            print(f'  refused ({why}): {hit[0]}')
            done.append(why)

        def index_add(line):
            return lambda t: t.replace('<!-- END GENERATED SIGNATURE INDEX',
                                       line + '\n<!-- END GENERATED SIGNATURE INDEX', 1)

        def append(text):
            return lambda t: t + text

        # The drill the gate exists for: a String producer nobody ledgered.
        refused('an unledgered String producer', 'fn string.drill_unledgered: no ledger row',
                edits=[('docs/stdlib/string.md', index_add('string.drill_unledgered(s: String) -> String'))])
        refused('an unledgered producer whose String hides in a declared record',
                'fn url.drill_unledgered: no ledger row', edits=[('docs/stdlib/url.md', index_add('url.drill_unledgered(s: String) -> Url'))])
        refused('an unledgered producer that hands its callback a String', 'fn string.drill_each: no ledger row',
                edits=[('docs/stdlib/string.md',
                        index_add('string.drill_each(s: String, f: (String) -> Unit) -> Unit'))])
        refused('a return type the gate cannot see into', 'type Mystery: declared nowhere',
                edits=[('docs/stdlib/string.md', index_add('string.drill_opaque(s: String) -> Mystery'))])
        refused('an unledgered host op', 'op 199: no ledger row',
                edits=[('crates/almide-wasm/src/fs.rs', append('\nconst OP_DRILL_UNLEDGERED: i32 = 199;\n'))])
        refused('an unledgered prim', 'prim drill_alloc: no ledger row',
                edits=[('stdlib/string.almd', append('\nfn __drill(n: Int) -> String = prim.drill_alloc(n)\n'))])
        refused('an unledgered minting source', 'mint stdlib/drill_mint.almd: no ledger row',
                edits=[('stdlib/drill_mint.almd', append('fn __drill_mint(n: Int) -> String = prim.alloc_str(n)\n'))])
        refused('an unledgered emitter helper', 'helper F_DRILL_UNLEDGERED: no ledger row',
                edits=[('crates/almide-wasm/src/lib.rs', append('\npub const F_DRILL_UNLEDGERED: u32 = 999;\n'))])
        # Evidence that feeds no invalid bytes: retarget a construction row at
        # a planted test that calls the producer on clean input only.
        row = next(r for r in led['fn']
                   if r.get('how') == 'construction' and '.almd::' in as_list(r.get('evidence'))[0])
        ev = as_list(row['evidence'])[0]
        rel = ev.split('::')[0]
        clean = f'\ntest "drill clean input" {{\n  assert(utf8_ok({row["id"]}("ascii")))\n}}\n'
        refused('evidence that feeds no invalid UTF-8', 'feeds no invalid UTF-8 byte',
                ledger=ledger_text.replace(f'"{ev}"', f'"{rel}::drill clean input"', 1),
                edits=[(rel, append(clean))])
        refused('evidence that never checks its output', 'does not check its output with utf8_ok',
                ledger=ledger_text.replace(f'"{ev}"', f'"{rel}::drill unchecked"', 1),
                edits=[(rel, append(f'\ntest "drill unchecked" {{\n  let b = bytes.from_list([0xFF])\n'
                                    f'  let s = {row["id"]}(bytes.to_string_lossy(b))\n}}\n'))])
        refused('evidence that never calls its producer', 'never reaches',
                ledger=ledger_text.replace(f'"{ev}"', f'"{rel}::drill wrong producer"', 1),
                edits=[(rel, append('\ntest "drill wrong producer" {\n  let b = bytes.from_list([0xFF])\n'
                                    '  assert(utf8_ok(bytes.to_string_lossy(b)))\n}\n'))])
        # A host op whose Rust evidence never touches an invalid byte.
        host_ev = 'tests/utf8_host_boundary_test.rs::file_text_reaches_the_guest_valid'
        refused('host evidence that feeds no invalid UTF-8', 'wasmtime_major feeds no invalid UTF-8 byte',
                ledger=ledger_text.replace(f'"{host_ev}"', '"tests/utf8_host_boundary_test.rs::wasmtime_major"', 1))
        refused('a fixed row whose ceiling did not come down', '< open_ceiling',
                ledger=re.sub(r'^open_ceiling = (\d+)', lambda m: f'open_ceiling = {int(m.group(1)) + 1}',
                              ledger_text, count=1, flags=re.M))
        refused('a missing evidence test', 'no test "no such test"', ledger=ledger_text.replace(f'"{ev}"', f'"{rel}::no such test"', 1))
        # The open list cannot grow, and an open row needs its fix.
        grown = re.sub(r'(\[\[fn\]\]\nid = "' + re.escape(row['id']) + r'"\nhow = )"construction"',
                       r'\1"open"\nfix = "drill"', ledger_text, count=1)
        assert grown != ledger_text, 'self-test could not forge an open row'
        refused('an open list above its ceiling', '> open_ceiling', ledger=grown)
        refused('an open row without its fix', 'names the fix', ledger=grown.replace('\nfix = "drill"', '', 1))
        refused('an empty ledger', 'no ledger row', ledger='open_ceiling = 0\n')
    finally:
        shutil.rmtree(tmp)
    print(f'self-test: {len(done)} forged inputs each refused; the committed tree passes')
    return 0


def main(argv):
    root = os.environ.get('UTF8_PRODUCERS_ROOT', ROOT)
    path = os.path.join(root, LEDGER)
    text = open(path, encoding='utf-8').read() if os.path.exists(path) else ''
    if '--bless' in argv:
        bless(root, text)
        return 0
    if '--self-test' in argv:
        return self_test(root, text)
    probs, opens, nfn = check(root, text)
    if probs:
        print(f'utf8-producers: {len(probs)} problem(s)', file=sys.stderr)
        for p in probs:
            print(f'  {p}', file=sys.stderr)
        return 1
    print(f'utf8-producers: {nfn} String-producing stdlib fns and every host op, prim, minting source '
          f'and emitter helper ledgered; {len(opens)} open: {"; ".join(opens)}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

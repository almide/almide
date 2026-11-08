#!/usr/bin/env python3
"""Tiny profiling wasm interpreter (MVP + bulk-memory + sign-ext + sat-trunc + tail calls).
Usage: wi.py module.wasm [--patch-i64 OLD NEW]... [--top N]
Counts executed instructions per function (self), calls per function, and opcode mix."""
import sys, struct, math, collections

sys.setrecursionlimit(1000000)
M32 = 0xFFFFFFFF; M64 = 0xFFFFFFFFFFFFFFFF

def s32(x): x &= M32; return x - (1 << 32) if x & 0x80000000 else x
def s64(x): x &= M64; return x - (1 << 64) if x & 0x8000000000000000 else x

class R:
    def __init__(s, b): s.b = b; s.p = 0
    def u8(s): v = s.b[s.p]; s.p += 1; return v
    def uleb(s):
        r = 0; sh = 0
        while True:
            b = s.b[s.p]; s.p += 1; r |= (b & 0x7f) << sh; sh += 7
            if b < 0x80: return r
    def sleb(s, bits):
        r = 0; sh = 0
        while True:
            b = s.b[s.p]; s.p += 1; r |= (b & 0x7f) << sh; sh += 7
            if b < 0x80:
                if b & 0x40 and sh < 64 + 7: r -= (1 << sh)
                return r
    def bytes_(s, n): v = s.b[s.p:s.p + n]; s.p += n; return v
    def vt(s):
        t = s.u8()
        if t in (0x63, 0x64): s.sleb(33); return 0x70
        return t
    def name(s): n = s.uleb(); return s.bytes_(n).decode('utf-8', 'replace')

class Trap(Exception): pass
class Exit(Exception):
    def __init__(s, c): s.code = c

def parse(b):
    r = R(b); assert r.bytes_(4) == b'\0asm'; r.bytes_(4)
    m = dict(types=[], imports=[], funcs=[], globals=[], exports={}, codes=[], data=[], mem=0, elems=[], table=0, start=None, names={})
    while r.p < len(b):
        sid = r.u8(); size = r.uleb(); end = r.p + size
        if sid == 1:
            for _ in range(r.uleb()):
                assert r.u8() == 0x60
                ps = [r.vt() for _ in range(r.uleb())]; rs = [r.vt() for _ in range(r.uleb())]
                m['types'].append((ps, rs))
        elif sid == 2:
            for _ in range(r.uleb()):
                mod = r.name(); nm = r.name(); k = r.u8()
                if k == 0: m['imports'].append((mod, nm, r.uleb()))
                elif k == 1: r.u8(); fl = r.uleb(); r.uleb();  # table
                elif k == 2:
                    fl = r.uleb(); m['mem'] = r.uleb()
                    if fl & 1: r.uleb()
                elif k == 3: r.u8(); r.u8()
        elif sid == 3:
            m['funcs'] = [r.uleb() for _ in range(r.uleb())]
        elif sid == 4:
            for _ in range(r.uleb()):
                r.u8(); fl = r.uleb(); m['table'] = r.uleb()
                if fl & 1: r.uleb()
        elif sid == 5:
            for _ in range(r.uleb()):
                fl = r.uleb(); m['mem'] = r.uleb()
                if fl & 1: r.uleb()
        elif sid == 6:
            for _ in range(r.uleb()):
                t = r.u8(); r.u8(); op = r.u8()
                if op == 0x41: v = r.sleb(32) & M32
                elif op == 0x42: v = r.sleb(64) & M64
                elif op == 0x44: v = struct.unpack('<d', r.bytes_(8))[0]
                elif op == 0x43: v = struct.unpack('<f', r.bytes_(4))[0]
                else: raise Exception('global init op %x' % op)
                assert r.u8() == 0x0b
                m['globals'].append(v)
        elif sid == 7:
            for _ in range(r.uleb()):
                nm = r.name(); k = r.u8(); idx = r.uleb(); m['exports'][nm] = (k, idx)
        elif sid == 8: m['start'] = r.uleb()
        elif sid == 9:
            for _ in range(r.uleb()):
                fl = r.uleb()
                if fl in (1, 3):
                    r.u8(); [r.uleb() for _ in range(r.uleb())]; continue
                assert fl == 0, 'elem flags %d' % fl
                assert r.u8() == 0x41; off = r.sleb(32); assert r.u8() == 0x0b
                m['elems'].append((off, [r.uleb() for _ in range(r.uleb())]))
        elif sid == 10:
            for _ in range(r.uleb()):
                sz = r.uleb(); e = r.p
                locs = []
                for _ in range(r.uleb()):
                    n = r.uleb(); t = r.vt(); locs += [t] * n
                m['codes'].append((locs, r.p, e + sz)); r.p = e + sz
        elif sid == 11:
            for _ in range(r.uleb()):
                fl = r.uleb()
                if fl == 0:
                    assert r.u8() == 0x41; off = r.sleb(32); assert r.u8() == 0x0b
                elif fl == 1: off = None
                else: r.uleb(); assert r.u8() == 0x41; off = r.sleb(32); assert r.u8() == 0x0b
                n = r.uleb(); m['data'].append((off, r.bytes_(n)))
        elif sid == 0:
            nm = r.name()
            if nm == 'name':
                while r.p < end:
                    sub = r.u8(); ssz = r.uleb(); se = r.p + ssz
                    if sub == 1:
                        for _ in range(r.uleb()):
                            i = r.uleb(); m['names'][i] = r.name()
                    r.p = se
        r.p = end
    return m

MEMARG = set(range(0x28, 0x3f))

def decode(b, start, end, types):
    """Return list of (op, a, b) with resolved jumps."""
    r = R(b); r.p = start
    ins = []; ctl = []
    while r.p < end:
        op = r.u8(); a = None; bb = None
        if op in (0x02, 0x03, 0x04):
            bt = b[r.p]
            if bt == 0x40: r.p += 1; arity = 0; params = 0
            elif bt in (0x7f, 0x7e, 0x7d, 0x7c, 0x7b, 0x70, 0x6f): r.p += 1; arity = 1; params = 0
            elif bt in (0x63, 0x64): r.p += 1; r.sleb(33); arity = 1; params = 0
            else:
                ti = r.sleb(33); params = len(types[ti][0]); arity = len(types[ti][1])
            ins.append([op, arity, params, None, None]); ctl.append(len(ins) - 1); continue
        elif op == 0x05:
            ins.append([op, None, None, None, None]); ins[ctl[-1]][4] = len(ins) - 1; continue
        elif op == 0x0b:
            ins.append([op, None, None, None, None])
            if ctl:
                o = ctl.pop(); ins[o][3] = len(ins) - 1
            continue
        elif op in (0x0c, 0x0d): a = r.uleb()
        elif op == 0x0e:
            a = [r.uleb() for _ in range(r.uleb())]; bb = r.uleb()
        elif op in (0x10, 0x12, 0x14, 0x15, 0xd2): a = r.uleb()
        elif op == 0xd0: r.sleb(33)
        elif op in (0x11, 0x13): a = r.uleb(); bb = r.uleb()
        elif op == 0x1c: n = r.uleb(); [r.u8() for _ in range(n)]
        elif op in (0x20, 0x21, 0x22, 0x23, 0x24): a = r.uleb()
        elif op in MEMARG: r.uleb(); a = r.uleb()
        elif op in (0x3f, 0x40): r.u8()
        elif op == 0x41: a = r.sleb(32) & M32
        elif op == 0x42: a = r.sleb(64) & M64
        elif op == 0x43: a = struct.unpack('<f', r.bytes_(4))[0]
        elif op == 0x44: a = struct.unpack('<d', r.bytes_(8))[0]
        elif op == 0xfc:
            sub = r.uleb(); op = 0xfc00 + sub
            if sub == 8: a = r.uleb(); r.u8()
            elif sub == 9: a = r.uleb()
            elif sub == 10: r.u8(); r.u8()
            elif sub == 11: r.u8()
        ins.append([op, a, bb, None, None])
    return ins

class VM:
    def __init__(s, m, wasm_mod_name='?'):
        s.m = m; s.nimp = len(m['imports'])
        pages = max(m['mem'], 1)
        s.mem = bytearray(pages * 65536)
        for off, d in m['data']:
            if off is not None: s.mem[off:off + len(d)] = d
        s.g = list(m['globals'])
        s.table = {}
        for off, fs in m['elems']:
            for i, f in enumerate(fs): s.table[off + i] = f
        s.code = {}
        s.selfc = collections.Counter(); s.calls = collections.Counter(); s.opc = collections.Counter()
        s.out = []
        s.ftype = [t for (_, _, t) in m['imports']] + m['funcs']
        s.grow_count = 0; s.copy_bytes = 0; s.copy_count = 0
    def fn(s, i):
        c = s.code.get(i)
        if c is None:
            locs, st, en = s.m['codes'][i - s.nimp]
            c = (locs, decode(s.m['raw'], st, en, s.m['types'])); s.code[i] = c
        return c
    def host(s, i, args):
        mod, nm, t = s.m['imports'][i]
        mem = s.mem
        if nm == 'fd_write':
            fd, iovs, n, nw = args; tot = 0
            for k in range(n):
                p, l = struct.unpack_from('<II', mem, iovs + 8 * k)
                s.out.append(bytes(mem[p:p + l])); tot += l
            struct.pack_into('<I', mem, nw, tot); return [0]
        if nm == 'proc_exit': raise Exit(args[0])
        if nm in ('environ_sizes_get', 'args_sizes_get'):
            struct.pack_into('<II', mem, args[0], 0, 0); struct.pack_into('<I', mem, args[1], 0); return [0]
        if nm in ('environ_get', 'args_get'): return [0]
        if nm == 'random_get': return [0]
        if nm == 'clock_time_get': struct.pack_into('<Q', mem, args[2], 0); return [0]
        if nm == 'fd_fdstat_get': mem[args[1]:args[1] + 24] = bytes(24); mem[args[1]] = 2; return [0]
        if nm == 'sched_yield': return [0]
        return [8]
    def call(s, fi, args):
        while True:
            if fi < s.nimp:
                s.calls[fi] += 1
                return s.host(fi, args)
            r = s.run(fi, args)
            if r[0] == 'tail':
                fi, args = r[1], r[2]; continue
            return r[1]

    def run(s, fi, args):
        s.calls[fi] += 1
        locs, ins = s.fn(fi)
        ps, rs = s.m['types'][s.ftype[fi]]
        L = list(args) + [0.0 if t in (0x7c, 0x7d) else 0 for t in locs]
        st = []; ctl = []  # (cont_pc, height, arity, is_loop)
        mem = s.mem; g = s.g
        pc = 0; n = 0; nins = len(ins)
        opc = s.opc
        up = struct.unpack_from; pk = struct.pack_into
        while pc < nins:
            I = ins[pc]; op = I[0]; n += 1
            opc[op] += 1
            if op == 0x20: st.append(L[I[1]])
            elif op == 0x21: L[I[1]] = st.pop()
            elif op == 0x22: L[I[1]] = st[-1]
            elif op == 0x41 or op == 0x42 or op == 0x44 or op == 0x43: st.append(I[1])
            elif op == 0x6a: b = st.pop(); st[-1] = (st[-1] + b) & M32
            elif op == 0x7c: b = st.pop(); st[-1] = (st[-1] + b) & M64
            elif op == 0x6b: b = st.pop(); st[-1] = (st[-1] - b) & M32
            elif op == 0x7d: b = st.pop(); st[-1] = (st[-1] - b) & M64
            elif op == 0x02:
                ctl.append((I[3], len(st) - I[2], I[1], False))
            elif op == 0x03:
                ctl.append((pc, len(st) - I[2], I[2], True))
            elif op == 0x04:
                c = st.pop()
                ctl.append((I[3], len(st) - I[2], I[1], False))
                if not c:
                    if I[4] is not None: pc = I[4]
                    else: ctl.pop(); pc = I[3]
            elif op == 0x05:
                cont, h, ar, _ = ctl.pop(); pc = cont; continue_ = True
            elif op == 0x0b:
                if ctl: ctl.pop()
            elif op == 0x0c or op == 0x0d:
                if op == 0x0d and not st.pop():
                    pc += 1; continue
                d = I[1]
                if d >= len(ctl): break
                cont, h, ar, isl = ctl[-1 - d]
                if ar: vals = st[len(st) - ar:]; del st[h:]; st += vals
                else: del st[h:]
                if isl:
                    del ctl[len(ctl) - d:]; pc = cont + 1; continue
                del ctl[len(ctl) - 1 - d:]; pc = cont + 1; continue
            elif op == 0x0e:
                k = st.pop(); tbl = I[1]; d = tbl[k] if k < len(tbl) else I[2]
                if d >= len(ctl): break
                cont, h, ar, isl = ctl[-1 - d]
                if ar: vals = st[len(st) - ar:]; del st[h:]; st += vals
                else: del st[h:]
                if isl: del ctl[len(ctl) - d:]
                else: del ctl[len(ctl) - 1 - d:]
                pc = cont + 1; continue
            elif op == 0x0f: break
            elif op == 0x10 or op == 0x11 or op == 0x12 or op == 0x13:
                if op in (0x11, 0x13):
                    ti = I[1]; k = st.pop(); f = s.table[k]
                else: f = I[1]
                np = len(s.m['types'][s.ftype[f]][0])
                a = st[len(st) - np:] if np else []
                if np: del st[len(st) - np:]
                if op in (0x12, 0x13):
                    s.selfc[fi] += n
                    return ('tail', f, a)
                s.selfc[fi] += n; n = 0
                st += s.call(f, a)
            elif op == 0xd2: st.append(('fref', I[1]))
            elif op == 0xd4: pass
            elif op == 0xd0: st.append(None)
            elif op == 0xd1: st[-1] = int(st[-1] is None)
            elif op == 0x14 or op == 0x15:
                f = st.pop()[1]
                np = len(s.m['types'][s.ftype[f]][0])
                a = st[len(st) - np:] if np else []
                if np: del st[len(st) - np:]
                s.selfc[fi] += n
                if op == 0x15: return ('tail', f, a)
                n = 0
                st += s.call(f, a)
            elif op == 0x1a: st.pop()
            elif op == 0x1b or op == 0x1c:
                c = st.pop(); b = st.pop()
                if not c: st[-1] = b
            elif op == 0x23: st.append(g[I[1]])
            elif op == 0x24: g[I[1]] = st.pop()
            elif 0x28 <= op <= 0x35:
                ad = st.pop() + I[1]
                if ad + 8 > len(mem) and ad >= len(mem): raise Trap('oob load %d' % ad)
                if op == 0x28: v = up('<I', mem, ad)[0]
                elif op == 0x29: v = up('<Q', mem, ad)[0]
                elif op == 0x2a: v = up('<f', mem, ad)[0]
                elif op == 0x2b: v = up('<d', mem, ad)[0]
                elif op == 0x2c: v = up('<b', mem, ad)[0] & M32
                elif op == 0x2d: v = mem[ad]
                elif op == 0x2e: v = up('<h', mem, ad)[0] & M32
                elif op == 0x2f: v = up('<H', mem, ad)[0]
                elif op == 0x30: v = up('<b', mem, ad)[0] & M64
                elif op == 0x31: v = mem[ad]
                elif op == 0x32: v = up('<h', mem, ad)[0] & M64
                elif op == 0x33: v = up('<H', mem, ad)[0]
                elif op == 0x34: v = up('<i', mem, ad)[0] & M64
                else: v = up('<I', mem, ad)[0]
                st.append(v)
            elif 0x36 <= op <= 0x3e:
                v = st.pop(); ad = st.pop() + I[1]
                if op == 0x36: pk('<I', mem, ad, v & M32)
                elif op == 0x37: pk('<Q', mem, ad, v & M64)
                elif op == 0x38: pk('<f', mem, ad, v)
                elif op == 0x39: pk('<d', mem, ad, v)
                elif op == 0x3a or op == 0x3c: mem[ad] = v & 0xff
                elif op == 0x3b or op == 0x3d: pk('<H', mem, ad, v & 0xffff)
                else: pk('<I', mem, ad, v & M32)
            elif op == 0x3f: st.append(len(mem) // 65536)
            elif op == 0x40:
                d = st.pop(); old = len(mem) // 65536
                mem.extend(bytes(d * 65536)); s.grow_count += 1; st.append(old)
            else:
                s.num(op, I, st)
            pc += 1
        s.selfc[fi] += n
        nr = len(rs)
        return ('ret', st[len(st) - nr:] if nr else [])

    def num(s, op, I, st):
        if op == 0x45: st[-1] = int(st[-1] == 0); return
        if op == 0x50: st[-1] = int(st[-1] == 0); return
        if 0x46 <= op <= 0x4f or 0x51 <= op <= 0x5a:
            b = st.pop(); a = st.pop()
            if op >= 0x51: op -= 0x0b; sg = s64
            else: sg = s32
            if op == 0x46: v = a == b
            elif op == 0x47: v = a != b
            elif op == 0x48: v = sg(a) < sg(b)
            elif op == 0x49: v = a < b
            elif op == 0x4a: v = sg(a) > sg(b)
            elif op == 0x4b: v = a > b
            elif op == 0x4c: v = sg(a) <= sg(b)
            elif op == 0x4d: v = a <= b
            elif op == 0x4e: v = sg(a) >= sg(b)
            else: v = a >= b
            st.append(int(v)); return
        if 0x5b <= op <= 0x66:
            b = st.pop(); a = st.pop(); k = op if op <= 0x60 else op - 6
            v = [a == b, a != b, a < b, a > b, a <= b, a >= b][k - 0x5b]
            st.append(int(v)); return
        if op in (0x67, 0x68, 0x69, 0x79, 0x7a, 0x7b):
            w = 32 if op < 0x79 else 64; a = st[-1]; k = op if op < 0x79 else op - 0x12
            if k == 0x67: v = w - a.bit_length()
            elif k == 0x68: v = w if a == 0 else ((a & -a).bit_length() - 1)
            else: v = bin(a).count('1')
            st[-1] = v; return
        if 0x6c <= op <= 0x78 or 0x7e <= op <= 0x8a:
            if op >= 0x7e: op -= 0x12; w = 64; MK = M64; sg = s64
            else: w = 32; MK = M32; sg = s32
            b = st.pop(); a = st.pop()
            if op == 0x6c: v = a * b
            elif op in (0x6d, 0x6f):
                if b == 0: raise Trap('div0')
                x, y = sg(a), sg(b); q = abs(x) // abs(y)
                if (x < 0) != (y < 0): q = -q
                v = q if op == 0x6d else x - q * y
            elif op == 0x6e:
                if b == 0: raise Trap('div0')
                v = a // b
            elif op == 0x70:
                if b == 0: raise Trap('div0')
                v = a % b
            elif op == 0x71: v = a & b
            elif op == 0x72: v = a | b
            elif op == 0x73: v = a ^ b
            elif op == 0x74: v = a << (b % w)
            elif op == 0x75: v = sg(a) >> (b % w)
            elif op == 0x76: v = a >> (b % w)
            elif op == 0x77: k = b % w; v = (a << k) | (a >> (w - k))
            else: k = b % w; v = (a >> k) | (a << (w - k))
            st.append(v & MK); return
        if 0x8b <= op <= 0xa6:
            if op >= 0x99: op -= 0x0e
            if op <= 0x91:
                a = st[-1]
                if op == 0x8b: v = abs(a)
                elif op == 0x8c: v = -a
                elif op == 0x8d: v = float(math.ceil(a)) if math.isfinite(a) else a
                elif op == 0x8e: v = float(math.floor(a)) if math.isfinite(a) else a
                elif op == 0x8f: v = float(math.trunc(a)) if math.isfinite(a) else a
                elif op == 0x90: v = float(round(a)) if math.isfinite(a) else a
                else: v = math.sqrt(a) if a >= 0 else float('nan')
                st[-1] = v; return
            b = st.pop(); a = st.pop()
            if op == 0x92: v = a + b
            elif op == 0x93: v = a - b
            elif op == 0x94: v = a * b
            elif op == 0x95:
                v = a / b if b != 0 else (float('nan') if a == 0 or a != a else math.copysign(float('inf'), a) * math.copysign(1, b))
            elif op == 0x96: v = min(a, b)
            elif op == 0x97: v = max(a, b)
            else: v = math.copysign(a, b)
            st.append(v); return
        a = st[-1]
        if op == 0xa7: st[-1] = a & M32
        elif op in (0xa8, 0xaa, 0xae, 0xb0): st[-1] = int(a) & (M32 if op in (0xa8, 0xaa) else M64)
        elif op in (0xa9, 0xab, 0xaf, 0xb1): st[-1] = int(a) & (M32 if op in (0xa9, 0xab) else M64)
        elif op == 0xac: st[-1] = s32(a) & M64
        elif op == 0xad: st[-1] = a
        elif op in (0xb2, 0xb7): st[-1] = float(s32(a))
        elif op in (0xb3, 0xb8): st[-1] = float(a)
        elif op in (0xb4, 0xb9): st[-1] = float(s64(a))
        elif op in (0xb5, 0xba): st[-1] = float(a)
        elif op in (0xb6, 0xbb): st[-1] = float(a)
        elif op == 0xbc: st[-1] = struct.unpack('<I', struct.pack('<f', a))[0]
        elif op == 0xbd: st[-1] = struct.unpack('<Q', struct.pack('<d', a))[0]
        elif op == 0xbe: st[-1] = struct.unpack('<f', struct.pack('<I', a))[0]
        elif op == 0xbf: st[-1] = struct.unpack('<d', struct.pack('<Q', a))[0]
        elif op == 0xc0: st[-1] = s32(((a & 0xff) ^ 0x80) - 0x80) & M32
        elif op == 0xc1: st[-1] = (((a & 0xffff) ^ 0x8000) - 0x8000) & M32
        elif op == 0xc2: st[-1] = (((a & 0xff) ^ 0x80) - 0x80) & M64
        elif op == 0xc3: st[-1] = (((a & 0xffff) ^ 0x8000) - 0x8000) & M64
        elif op == 0xc4: st[-1] = s32(a) & M64
        elif op >= 0xfc00:
            sub = op - 0xfc00
            if sub <= 7:
                v = a
                if v != v: r = 0
                else:
                    lo, hi = [(-2**31, 2**31-1), (0, 2**32-1)][sub & 1] if sub < 4 else [(-2**63, 2**63-1), (0, 2**64-1)][sub & 1]
                    r = int(max(lo, min(hi, math.trunc(v) if math.isfinite(v) else (hi if v > 0 else lo))))
                st[-1] = r & (M32 if sub < 4 else M64)
            elif sub == 10:
                n = st.pop(); src = st.pop(); d = st.pop()
                s.mem[d:d + n] = s.mem[src:src + n]; s.copy_bytes += n; s.copy_count += 1
            elif sub == 11:
                n = st.pop(); v = st.pop(); d = st.pop()
                s.mem[d:d + n] = bytes([v & 0xff]) * n
            elif sub == 8:
                n = st.pop(); src = st.pop(); d = st.pop()
                s.mem[d:d + n] = s.m['data'][I[1]][1][src:src + n]
            elif sub == 9: pass
            else: raise Exception('fc %d' % sub)
        else:
            raise Exception('op %x' % op)

def main():
    a = sys.argv[1:]
    path = a[0]; raw = bytearray(open(path, 'rb').read())
    top = 25; i = 1; entry = None
    while i < len(a):
        if a[i] == '--patch-i64':
            old, new = int(a[i + 1]), int(a[i + 2]); i += 3
            def leb(v):
                out = bytearray()
                while True:
                    b = v & 0x7f; v >>= 7
                    if (v == 0 and not b & 0x40) or (v == -1 and b & 0x40): out.append(b); return bytes(out)
                    out.append(b | 0x80)
            o = bytes([0x42]) + leb(old); nn = bytes([0x42]) + leb(new)
            cnt = raw.count(o)
            if len(nn) < len(o): nn = nn[:-1] + bytes([nn[-1] | 0x80]) + bytes([0x80] * (len(o) - len(nn) - 1)) + b'\x00'
            assert len(nn) == len(o), (o, nn)
            raw = raw.replace(o, nn); print('patched %d sites of i64.const %d' % (cnt, old), file=sys.stderr)
        elif a[i] == '--patch-i32':
            old, new = int(a[i + 1]), int(a[i + 2]); i += 3
            def leb(v):
                out = bytearray()
                while True:
                    b = v & 0x7f; v >>= 7
                    if (v == 0 and not b & 0x40) or (v == -1 and b & 0x40): out.append(b); return bytes(out)
                    out.append(b | 0x80)
            o = bytes([0x41]) + leb(old); nn = bytes([0x41]) + leb(new)
            if len(nn) < len(o): nn = nn[:-1] + bytes([nn[-1] | 0x80]) + bytes([0x80] * (len(o) - len(nn) - 1)) + b'\x00'
            assert len(nn) == len(o)
            cnt = raw.count(o); raw = raw.replace(o, nn); print('patched %d sites of i32.const %d' % (cnt, old), file=sys.stderr)
        elif a[i] == '--top': top = int(a[i + 1]); i += 2
        elif a[i] == '--entry': entry = a[i + 1]; i += 2
        else: raise SystemExit('bad arg ' + a[i])
    m = parse(bytes(raw)); m['raw'] = bytes(raw)
    vm = VM(m)
    ent = entry or ('_start' if '_start' in m['exports'] else 'main')
    fi = m['exports'][ent][1]
    code = 0
    try:
        vm.call(fi, [])
    except Exit as e: code = e.code
    out = b''.join(vm.out)
    print('stdout:', out[:200], 'exit', code)
    tot = sum(vm.selfc.values())
    print('total instrs %d  calls %d  memory.grow %d  memory.copy %d (%d bytes)' % (tot, sum(vm.calls.values()), vm.grow_count, vm.copy_count, vm.copy_bytes))
    nm = m['names']
    for f, c in vm.selfc.most_common(top):
        print('%6.2f%% %12d self  %9d calls  f%-4d %s' % (100.0 * c / tot, c, vm.calls[f], f, nm.get(f, '')[:70]))
    print('opcode mix (top 30):')
    for o, c in vm.opc.most_common(30):
        print('   %6x %6.2f%%' % (o, 100.0 * c / tot))

if __name__ == '__main__':
    main()

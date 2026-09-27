#!/usr/bin/env python3
r"""Prototype of the proposed concurrent-host var-reach rule, over `almide --emit-ast`.

A *site* is a function value handed to a concurrent host:
  - http.serve(_, H), http.route(_, H), http.mount(_, H), http.wrap(H, [M...])
  - every arm of a `fan { ... }` block, every fn-valued argument of fan.<op>(...)
A site BREAKS when, from its expression, one can reach a `var` declared outside it:
  - captured:   a `var` local of an enclosing fn
  - toplevel:   a top-level `var` named directly
  - transitive: a top-level `var` reached through a called / referenced top-level fn,
                or through a local/top-level `let` whose value is such a closure
Vars declared INSIDE the site (a lambda's own `var`) are per-invocation and fine.

A lambda capturing a `var` of its enclosing fn is its own body (the var is shared)
unless it is a direct argument of a non-retaining stdlib HOF (list.map & co.), which
runs it inside the invocation. A `let` copy of a var's value taken outside the site
is a snapshot, not a reach.

Usage (ADR-0020, migration measurement):
  find <dir> -name '*.almd' | xargs grep -lE 'http\.(serve|route|mount|wrap)\b|\bfan\b' \
    | python3 research/spike/concurrent-var-reach/reach.py <label>
Prints one JSON summary line, then one BREAK line per site that reaches a var.
Needs `almide` (for --emit-ast) on PATH. Cross-package calls are not followed
(reported as `unresolved`).
"""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor

HTTP_SITES = {"serve": [1], "route": [1], "mount": [1], "wrap": [0, 1]}
FAN_OPS = {"map", "settle", "any", "any_map", "race", "timeout", "bounded"}
import glob as _g
STDLIB = {os.path.basename(f).split(".")[0].split("_")[0] for f in _g.glob(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../stdlib/*.almd"))} | set("""string int float list map set math datetime error bytes value option result json fs http env io
random regex process testing url fan int8 int16 int32 int64 uint8 uint16 uint32 uint64 float32 hash base64
path log time csv toml yaml""".split())


def emit_ast(path):
    try:
        ap = os.path.abspath(path); d = os.path.dirname(ap)
        while d != "/" and not os.path.exists(os.path.join(d, "almide.toml")):
            d = os.path.dirname(d)
        cwd = d if d != "/" else os.path.dirname(ap)
        out = subprocess.run(["almide", ap, "--emit-ast"], capture_output=True, text=True, timeout=120, cwd=cwd)
        t = out.stdout
        i = t.find("{")
        return json.loads(t[i:]) if i >= 0 else None
    except Exception:
        return None


def children(n):
    if isinstance(n, dict):
        for k, v in n.items():
            if isinstance(v, (dict, list)):
                yield v
    elif isinstance(n, list):
        for v in n:
            yield v


def bound_names(n, acc):
    """Every name bound anywhere inside n (params, let, var, for, patterns)."""
    if isinstance(n, dict):
        k = n.get("kind")
        if k in ("let", "var") and isinstance(n.get("name"), str):
            acc.add(n["name"])
        if k == "lambda" or k == "fn":
            for p in n.get("params") or []:
                if isinstance(p, dict) and isinstance(p.get("name"), str):
                    acc.add(p["name"])
        if k in ("for_in",):
            for key in ("var", "var_name", "name"):
                if isinstance(n.get(key), str):
                    acc.add(n[key])
        # patterns: any node with kind ending in "pattern"/"bind" carrying a name
        if k and ("pattern" in k or k in ("bind", "binding", "ident_pattern", "pvar")) and isinstance(n.get("name"), str):
            acc.add(n["name"])
        for key in ("pattern", "patterns", "var_tuple", "destructure", "names"):
            v = n.get(key)
            if isinstance(v, list):
                for x in v:
                    if isinstance(x, str):
                        acc.add(x)
    for c in children(n):
        bound_names(c, acc)
    # param-pattern tuples: {"kind":"tuple_pattern", "elements":[{"kind":"ident","name":..}]}
    return acc


class Module:
    def __init__(self, ast):
        self.top_vars, self.top_lets, self.fns, self.user_imports = set(), {}, {}, set()
        for imp in ast.get("imports") or []:
            p = imp.get("path") or []
            if p and p[0] not in STDLIB:
                self.user_imports.add(imp.get("alias") or p[-1])
        for d in ast.get("decls") or []:
            k = d.get("kind")
            if k == "top_let":
                (self.top_vars.add(d["name"]) if d.get("mutable") else self.top_lets.__setitem__(d["name"], d.get("value")))
            elif k == "fn" and isinstance(d.get("name"), str):
                self.fns[d["name"]] = d
            elif k in ("impl", "protocol_impl"):
                for m in d.get("methods") or []:
                    if isinstance(m, dict) and isinstance(m.get("name"), str):
                        self.fns.setdefault(m["name"], m)
        self.fn_memo, self.let_memo = {}, {}

    # reasons a top-level fn reaches a top-level var (transitively)
    def fn_reach(self, name, stack=()):
        if name in self.fn_memo:
            return self.fn_memo[name]
        if name in stack:
            return set()
        d = self.fns[name]
        bound = bound_names(d.get("body"), set())
        for p in d.get("params") or []:
            if isinstance(p, dict) and isinstance(p.get("name"), str):
                bound.add(p["name"])
        r = self.walk(d.get("body"), bound, {}, set(), stack + (name,))
        r |= escaping_captures(d.get("body"))
        # everything reached from inside a fn is 'transitive' from the site's point of view
        out = {(k0 if k0 in ("unresolved", "escape") else "transitive", v, w, via) for (k0, v, w, via) in r}
        self.fn_memo[name] = out
        return out

    def let_reach(self, name, stack=()):
        if name in self.let_memo:
            return self.let_memo[name]
        if ("let", name) in stack:
            return set()
        v = self.top_lets[name]
        r = self.walk(v, bound_names(v, set()), {}, set(), stack + (("let", name),), False)
        out = {(k if k in ("toplevel", "unresolved") else "transitive", var, w, via) for (k, var, w, via) in r}
        self.let_memo[name] = out
        return out

    def walk(self, n, bound, local_lets, local_vars, stack, live=True):
        """Reasons reachable from n. bound = names bound inside the site.
        live=False while evaluating a let's value OUTSIDE the site: a var read there is a
        snapshot (a `let` copy), not a reach; entering a lambda makes it live again."""
        out = set()
        if isinstance(n, dict):
            k = n.get("kind")
            if k == "lambda" and not live:
                return self.walk(n, bound, local_lets, local_vars, stack, True)
            if k in ("ident", "assign") and (live or n.get("name") not in local_vars and n.get("name") not in self.top_vars):
                name = n.get("name")
                write = k == "assign"
                if isinstance(name, str) and name not in bound:
                    if name in local_vars:
                        out.add(("captured", name, write, ""))
                    elif name in local_lets:
                        if ("local", name) not in stack:
                            out |= self.walk(local_lets[name], bound, local_lets, local_vars, stack + (("local", name),), False)
                    elif name in self.top_vars:
                        out.add(("toplevel", name, write, ""))
                    elif name in self.top_lets:
                        out |= self.let_reach(name, stack)
                    elif name in self.fns:
                        out |= {(k2, v, w, via or name) for (k2, v, w, via) in self.fn_reach(name, stack)}
            if k == "member" and isinstance(n.get("object"), dict) and n["object"].get("kind") == "ident":
                if n["object"].get("name") in self.user_imports:
                    out.add(("unresolved", n["object"]["name"] + "." + str(n.get("field")), False, ""))
        for c in children(n):
            out |= self.walk(c, bound, local_lets, local_vars, stack, live)
        return out


FOLDING = None


def var_decls(n, acc):
    if isinstance(n, dict) and n.get("kind") == "var" and isinstance(n.get("name"), str):
        acc.add(n["name"])
    for c in children(n):
        if not (isinstance(c, dict) and c.get("kind") == "lambda"):
            var_decls(c, acc)
    return acc


def names_used(n, acc):
    if isinstance(n, dict) and n.get("kind") in ("ident", "assign") and isinstance(n.get("name"), str):
        acc.add(n["name"])
    for c in children(n):
        names_used(c, acc)
    return acc


def escaping_captures(body):
    """Unfolded lambdas (not a direct arg of a non-retaining stdlib HOF) that name a var
    declared in the enclosing fn body outside the lambda: the closure may outlive the
    invocation (a factory returning it, a record holding it), so the var is shared."""
    fn_vars = var_decls(body, set())
    out = set()
    if not fn_vars:
        return out

    def go(n, folded_args):
        if isinstance(n, dict):
            if n.get("kind") == "lambda" and id(n) not in folded_args:
                inner = bound_names(n, set())
                for v in (names_used(n.get("body"), set()) & fn_vars) - inner:
                    out.add(("escape", v, True, ""))
            fa = set(folded_args)
            if n.get("kind") == "call":
                m, f = callee_path(n)
                if m in STDLIB and m not in ("fan", "http"):
                    fa |= {id(a) for a in n.get("args") or []}
            for c in children(n):
                go(c, fa)
        elif isinstance(n, list):
            for c in n:
                go(c, folded_args)
    go(body, set())
    return out


def callee_path(call):
    c = call.get("callee") or {}
    if c.get("kind") == "member" and isinstance(c.get("object"), dict) and c["object"].get("kind") == "ident":
        return c["object"].get("name"), c.get("field")
    return None, None


def find_sites(mod, n, local_lets, local_vars, sites, where):
    """Walk a fn body in order, tracking the enclosing fn's var/let bindings."""
    if isinstance(n, dict):
        k = n.get("kind")
        if k == "block":
            ll, lv = dict(local_lets), set(local_vars)
            for s in n.get("stmts") or []:
                find_sites(mod, s, ll, lv, sites, where)
                if isinstance(s, dict) and s.get("kind") == "var":
                    lv.add(s["name"]); ll.pop(s["name"], None)
                elif isinstance(s, dict) and s.get("kind") == "let" and isinstance(s.get("name"), str):
                    ll[s["name"]] = s.get("value"); lv.discard(s["name"])
            if n.get("expr") is not None:
                find_sites(mod, n["expr"], ll, lv, sites, where)
            return
        if k == "lambda":
            # a lambda's own params/vars shadow; its body can itself contain sites
            ll, lv = dict(local_lets), set(local_vars)
            for p in n.get("params") or []:
                if isinstance(p, dict):
                    ll.pop(p.get("name"), None); lv.discard(p.get("name"))
            find_sites(mod, n.get("body"), ll, lv, sites, where)
            return
        site_exprs = []
        if k == "fan":
            site_exprs = [("fan-block", e) for e in n.get("exprs") or []]
        elif k == "call":
            m, f = callee_path(n)
            args = n.get("args") or []
            if m == "http" and f in HTTP_SITES:
                site_exprs = [("http." + f, args[i]) for i in HTTP_SITES[f] if i < len(args)]
            elif m == "fan" and f in FAN_OPS:
                site_exprs = [("fan." + f, a) for a in args if isinstance(a, dict) and a.get("kind") in ("lambda", "ident", "list", "member")]
        for label, e in site_exprs:
            bound = bound_names(e, set()) if isinstance(e, dict) and e.get("kind") in ("lambda", "list", "block") else set()
            r = mod.walk(e, bound, local_lets, local_vars, ())
            sites.append({"site": label, "in": where, "reasons": sorted({(a, b, w, via) for (a, b, w, via) in r})})
    for c in children(n):
        find_sites(mod, c, local_lets, local_vars, sites, where)


def analyze(path):
    ast = emit_ast(path)
    if ast is None:
        return {"file": path, "parse": False}
    mod = Module(ast)
    sites = []
    for d in ast.get("decls") or []:
        if d.get("kind") == "fn":
            find_sites(mod, d.get("body"), {}, set(), sites, d.get("name"))
        elif d.get("kind") in ("test", "test_decl"):
            find_sites(mod, d.get("body"), {}, set(), sites, "test:" + str(d.get("name")))
        elif d.get("kind") == "top_let":
            find_sites(mod, d.get("value"), {}, set(), sites, "let:" + d["name"])
        else:
            for key in ("body", "methods"):
                if key in d:
                    find_sites(mod, d[key], {}, set(), sites, str(d.get("kind")))
    return {"file": path, "parse": True, "sites": sites}


def main():
    corpus, files = sys.argv[1], [l.strip() for l in sys.stdin if l.strip()]
    with ThreadPoolExecutor(4) as ex:
        res = list(ex.map(analyze, files))
    n_sites = n_break = 0
    by_kind, broke_files, unparsed, labels = {}, [], [], {}
    rows = []
    for r in res:
        if not r["parse"]:
            unparsed.append(r["file"]); continue
        fb = False
        for s in r["sites"]:
            n_sites += 1
            labels[s["site"]] = labels.get(s["site"], 0) + 1
            real = [x for x in s["reasons"] if x[0] != "unresolved"]
            if real:
                n_break += 1; fb = True
                kinds = sorted({x[0] for x in real})
                key = s["site"] + ":" + "+".join(kinds)
                by_kind[key] = by_kind.get(key, 0) + 1
                rows.append((r["file"], s["in"], s["site"], real))
            elif s["reasons"]:
                by_kind[s["site"] + ":unresolved-only"] = by_kind.get(s["site"] + ":unresolved-only", 0) + 1
        if fb:
            broke_files.append(r["file"])
    print(json.dumps({"corpus": corpus, "files": len(files), "unparsed": len(unparsed), "sites": n_sites,
                      "breaking_sites": n_break, "breaking_files": len(broke_files), "by_kind": by_kind, "site_labels": labels}))
    for f, where, site, real in rows:
        print("  BREAK", f, where, site, real)
    for f in unparsed:
        print("  UNPARSED", f)


if __name__ == "__main__":
    main()

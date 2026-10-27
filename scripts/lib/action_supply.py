#!/usr/bin/env python3
"""The run-time half of the action pin gate (security row 16).

`scripts/check-action-pins.sh` proves every `uses:` names a commit. A commit
pin still runs whatever that commit FETCHES when it runs, so this file closes
the two channels the pin cannot see:

  versions  A step that installs a toolchain or a runtime by a moving name —
            `dtolnay/rust-toolchain` with no `toolchain:` (its `stable` branch
            defaults to the `stable` channel) or with `stable`, a `rustup`
            command naming a channel, `actions/setup-node` with `node-version:
            20` (whatever 20.x is newest today), `cargo install` without
            `--locked` and an exact `--version`, `pip install` without `==`.
            Every value is resolved through `${{ env.X }}` (any `env:` block of
            the file), `${{ inputs.X }}` (the composite's callers, or the
            dispatch input's default) and `a || 'b'` (both sides) before it is
            judged. Offline.

  closure   A pinned third-party action that is itself a composite runs its
            own `uses:` — by tag, if its author wrote one. `--closure` fetches
            each pinned action's `action.yml` AT THE PINNED COMMIT and requires
            every nested `uses:` to be a 40-hex commit (a `./` path inside the
            pinned repository is the same commit), recursing, and every
            `docker://` image to carry an `@sha256:` digest. Needs the network
            (raw.githubusercontent.com); `ACTION_SUPPLY_FETCH_ROOT=<dir>` reads
            `<dir>/<owner>/<repo>/<sha>/<path>/action.yml` instead (the
            self-test's mirror).

Usage: action_supply.py versions <root>
       action_supply.py closure <root>
Exit 0 clean, 1 on a finding, 2 on a usage or fetch error.
"""
import os
import re
import sys
import urllib.request

EXACT = {
    # The action, the input that names what it downloads, and what an exact
    # name looks like for it.
    "dtolnay/rust-toolchain": ("toolchain", re.compile(r"^(\d+\.\d+\.\d+|(nightly|beta)-\d{4}-\d{2}-\d{2})$")),
    "actions/setup-node": ("node-version", re.compile(r"^v?\d+\.\d+\.\d+$")),
}
# An action that must be given its version input (its own default is a moving
# name). setup-node without `node-version` reads `.nvmrc` / the runner's node,
# so only the toolchain action is listed.
REQUIRED = {"dtolnay/rust-toolchain"}

RUSTUP_CHANNEL = re.compile(r"(?<![\w.-])\+?(stable|beta|nightly)(?![\w-])")
SHA40 = re.compile(r"@[0-9a-f]{40}$")


def workflow_files(root):
    out = []
    for sub in (".github/workflows", ".github/actions"):
        base = os.path.join(root, sub)
        for dirpath, _, names in os.walk(base):
            for n in names:
                if n.endswith((".yml", ".yaml")):
                    out.append(os.path.join(dirpath, n))
    return sorted(out)


def indent(line):
    return len(line) - len(line.lstrip(" "))


def strip_value(v):
    v = v.split(" #", 1)[0].strip()
    if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
        v = v[1:-1]
    return v.strip()


def step_inputs(lines, i):
    """The `with:` map of the step whose `uses:` is on line i."""
    uses_line = lines[i]
    # The step's own key column: the `uses:` key, after an optional `- `.
    m = re.match(r"^(\s*)(-\s+)?uses:", uses_line)
    key_col = len(m.group(1)) + (len(m.group(2)) if m.group(2) else 0)
    out = {}
    j = i + 1
    while j < len(lines):
        ln = lines[j]
        if not ln.strip() or ln.lstrip().startswith("#"):
            j += 1
            continue
        if indent(ln) < key_col or (indent(ln) == key_col and ln.lstrip().startswith("- ")):
            break
        if indent(ln) == key_col and ln.strip() == "with:":
            k = j + 1
            child_col = None
            while k < len(lines):
                w = lines[k]
                if not w.strip() or w.lstrip().startswith("#"):
                    k += 1
                    continue
                if indent(w) <= key_col:
                    break
                if child_col is None:
                    child_col = indent(w)
                mm = re.match(r"^\s*([A-Za-z0-9_-]+):\s*(.*)$", w)
                if mm and indent(w) == child_col:
                    out[mm.group(1)] = (strip_value(mm.group(2)), k + 1)
                k += 1
            j = k
            continue
        j += 1
    return out


def env_values(lines, name):
    """Every value `name` is given under any `env:` block of the file."""
    vals = []
    for ln in lines:
        m = re.match(r"^\s*" + re.escape(name) + r":\s*(.+)$", ln)
        if m:
            vals.append(strip_value(m.group(1)))
    return vals


def input_default(lines, name):
    """The `default:` of a declared input `name` (workflow_dispatch or composite)."""
    for i, ln in enumerate(lines):
        if re.match(r"^\s*" + re.escape(name) + r":\s*$", ln):
            col = indent(ln)
            for w in lines[i + 1:]:
                if w.strip() and indent(w) <= col:
                    break
                m = re.match(r"^\s*default:\s*(.+)$", w)
                if m:
                    return strip_value(m.group(1))
    return None


def callers(root, action_dir, input_name):
    """(file, line, value) for every `uses: ./<action_dir>` passing `input_name`."""
    out = []
    for f in workflow_files(root):
        lines = open(f, encoding="utf-8").read().splitlines()
        for i, ln in enumerate(lines):
            m = re.match(r"^\s*(-\s+)?uses:\s*['\"]?\./" + re.escape(action_dir) + r"/?['\"]?\s*(#.*)?$", ln)
            if m:
                ins = step_inputs(lines, i)
                if input_name in ins:
                    out.append((f, lines, ins[input_name][0], ins[input_name][1]))
                else:
                    out.append((f, lines, None, i + 1))
    return out


def resolve(root, f, lines, value, depth=0):
    """Every concrete string `value` can take, or (None, why) for a hole."""
    if depth > 8:
        return [(None, "expression nests too deep")]
    m = re.fullmatch(r"\$\{\{\s*(.+?)\s*\}\}", value)
    if not m:
        return [(value, None)]
    expr = m.group(1)
    out = []
    for alt in [a.strip() for a in expr.split("||")]:
        lit = re.fullmatch(r"'([^']*)'|\"([^\"]*)\"", alt)
        if lit:
            out.append((lit.group(1) if lit.group(1) is not None else lit.group(2), None))
            continue
        em = re.fullmatch(r"env\.([A-Za-z0-9_]+)", alt)
        if em:
            vals = env_values(lines, em.group(1))
            if not vals:
                out.append((None, f"`env.{em.group(1)}` is defined nowhere in the file"))
            for v in vals:
                out.extend(resolve(root, f, lines, v, depth + 1))
            continue
        im = re.fullmatch(r"inputs\.([A-Za-z0-9_-]+)", alt)
        if im:
            name = im.group(1)
            d = input_default(lines, name)
            if d is not None:
                out.extend(resolve(root, f, lines, d, depth + 1))
            rel = os.path.relpath(f, root)
            if rel.startswith(".github/actions/"):
                action_dir = os.path.dirname(rel)
                cs = callers(root, action_dir, name)
                if not cs and d is None:
                    out.append((None, f"composite input `{name}` has no caller and no default"))
                for cf, clines, cv, _cl in cs:
                    if cv is None:
                        if d is None:
                            out.append((None, f"{os.path.relpath(cf, root)} calls ./{action_dir} without `{name}`"))
                        continue
                    out.extend(resolve(root, cf, clines, cv, depth + 1))
            elif d is None:
                out.append((None, f"dispatch input `{name}` has no default"))
            continue
        out.append((None, f"cannot resolve `{alt}`"))
    return out


def check_versions(root):
    bad = []
    n = 0
    for f in workflow_files(root):
        rel = os.path.relpath(f, root)
        lines = open(f, encoding="utf-8").read().splitlines()
        for i, ln in enumerate(lines):
            um = re.match(r"^\s*(-\s+)?uses:\s*['\"]?([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)(/[^@\s'\"]*)?@", ln)
            if um and um.group(2) in EXACT:
                action = um.group(2)
                key, shape = EXACT[action]
                ins = step_inputs(lines, i)
                n += 1
                if key not in ins:
                    if action in REQUIRED:
                        bad.append(f"{rel}:{i + 1}  {action} without `{key}:` installs whatever its branch defaults to")
                    continue
                raw, at = ins[key]
                for val, why in resolve(root, f, lines, raw):
                    if val is None:
                        bad.append(f"{rel}:{at}  {key}: {raw} — {why}")
                    elif not shape.match(val):
                        bad.append(f"{rel}:{at}  {key}: {raw} resolves to `{val}`, a moving name — name an exact version")
            body = ln.split("#", 1)[0] if not ln.lstrip().startswith("run:") else ln
            body = re.sub(r"^\s*(-\s+)?run:\s*", "", body)
            if ln.lstrip().startswith("#"):
                continue
            if re.search(r"(^|[\s;&|(])rustup\s", body):
                n += 1
                cm = RUSTUP_CHANNEL.search(body.split("rustup", 1)[1])
                if cm:
                    bad.append(f"{rel}:{i + 1}  rustup names the `{cm.group(1)}` channel — name an exact toolchain")
            if re.search(r"(^|[\s;&|(])cargo\s+install\s", body):
                n += 1
                if "--locked" not in body or not re.search(r"--version[ =]\S|@\d", body):
                    bad.append(f"{rel}:{i + 1}  cargo install without `--locked` and an exact `--version`")
            pm = re.search(r"(^|[\s;&|(])pip3?\s+install\s+(.*)$|python3?\s+-m\s+pip\s+install\s+(.*)$", body)
            if pm:
                n += 1
                args = (pm.group(2) or pm.group(3) or "").split()
                pkgs = [a for a in args if not a.startswith("-")]
                for p in pkgs:
                    if "==" not in p:
                        bad.append(f"{rel}:{i + 1}  pip install `{p}` without an exact `==` version")
    for b in bad:
        print("MOVING " + b, file=sys.stderr)
    if bad:
        print(f"action-supply versions: {len(bad)} step(s) fetch by a moving name ({n} checked).", file=sys.stderr)
        return 1
    print(f"action-supply versions: OK — {n} toolchain / runtime / installer step(s), every one names an exact version")
    return 0


def fetch(owner_repo, sha, sub):
    rootdir = os.environ.get("ACTION_SUPPLY_FETCH_ROOT")
    for name in ("action.yml", "action.yaml"):
        rel = "/".join(p for p in (owner_repo, sha, sub, name) if p)
        if rootdir:
            p = os.path.join(rootdir, *rel.split("/"))
            if os.path.exists(p):
                return open(p, encoding="utf-8").read()
            continue
        url = "https://raw.githubusercontent.com/" + rel
        for attempt in range(4):
            try:
                with urllib.request.urlopen(url, timeout=30) as r:
                    return r.read().decode("utf-8")
            except urllib.error.HTTPError as e:
                if e.code == 404:
                    break
            except Exception:
                pass
    return None


def parse_ref(ref):
    m = re.fullmatch(r"([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)(?:/([^@]+))?@([0-9a-f]{40})", ref)
    return (m.group(1), m.group(2) or "", m.group(3)) if m else None


def refs_in(text):
    for ln in text.splitlines():
        if ln.lstrip().startswith("#"):
            continue
        m = re.match(r"^\s*(-\s+)?uses:\s*(.+)$", ln)
        if m:
            yield strip_value(m.group(2))
        m = re.match(r"^\s*image:\s*(.+)$", ln)
        if m:
            yield "image:" + strip_value(m.group(1))


def check_closure(root):
    todo = []
    for f in workflow_files(root):
        for r in refs_in(open(f, encoding="utf-8").read()):
            if parse_ref(r):
                todo.append((r, os.path.relpath(f, root)))
    seen = set()
    bad = []
    errs = []
    while todo:
        ref, via = todo.pop()
        if ref in seen:
            continue
        seen.add(ref)
        owner_repo, sub, sha = parse_ref(ref)
        text = fetch(owner_repo, sha, sub)
        if text is None:
            errs.append(f"{ref} (via {via}): no action.yml at the pinned commit could be fetched")
            continue
        for nested in refs_in(text):
            if nested.startswith("image:"):
                img = nested[len("image:"):]
                if img.startswith("docker://") and "@sha256:" not in img:
                    bad.append(f"{ref} runs the image `{img}` by tag")
                continue
            if nested.startswith("./") or nested.startswith("docker://"):
                if nested.startswith("docker://") and "@sha256:" not in nested:
                    bad.append(f"{ref} runs the image `{nested}` by tag")
                continue
            if not SHA40.search(nested):
                bad.append(f"{ref} runs `uses: {nested}`, which is not pinned to a commit")
                continue
            todo.append((nested, ref))
    for e in errs:
        print("FETCH " + e, file=sys.stderr)
    for b in bad:
        print("UNPINNED-NESTED " + b, file=sys.stderr)
    if bad:
        print(f"action-supply closure: {len(bad)} nested reference(s) fetch by a moving name ({len(seen)} action(s) walked).", file=sys.stderr)
        return 1
    if errs:
        return 2
    print(f"action-supply closure: OK — {len(seen)} pinned action(s) walked at their commits, every nested `uses:` pinned")
    return 0


def main(argv):
    if len(argv) != 3 or argv[1] not in ("versions", "closure"):
        print(__doc__, file=sys.stderr)
        return 2
    return check_versions(argv[2]) if argv[1] == "versions" else check_closure(argv[2])


if __name__ == "__main__":
    sys.exit(main(sys.argv))

#!/usr/bin/env python3
"""The download half of the action pin gate (security row 16).

A commit-pinned workflow still runs whatever bytes a `curl` of a release asset
returns, and the upstream can replace those bytes under the same versioned URL.
This file finds every fetch of a binary or archive in CI and refuses any whose
bytes are not checked against a digest stated in this repository.

What is scanned (derived from the files, no names listed):
  .github/workflows/*.yml, .github/actions/**/*.yml  — per step (`run:` block)
  scripts/**/*.sh                                     — per file
Every `curl`, `wget` and `Invoke-WebRequest` invocation (comment lines and
heredoc bodies skipped, `\\` continuations joined) is a DOWNLOAD when it writes
a file or a pipe (`-o`, `-O`, `--output`, `--remote-name`, `-OutFile`, any
`wget`, or `| tar|sh|bash|unzip|python`) AND it fetches a binary or archive:
the URL is a `/releases/` asset, or the URL / output names an archive or a
binary (.tar.*, .tgz, .zip, .xz, .gz, .exe, .msi, .deb, .rpm, .sh, .bin), or
it is piped into an unpacker or an interpreter. A data fetch (a `.toml` read at
a commit, an API POST, a metadata query) is not a download.

A download is accepted when its unit:
  PINNED    computes a sha256 (`sha256sum` / `shasum -a 256`) AND states the
            digest — a 64-hex literal, or .github/pinned-downloads.txt; or
  ATTESTED  runs `gh attestation verify ... --repo|-R <owner/repo>` and the URL
            is a release asset of that same <owner/repo> (a first-party
            release fetched by a moving name, which no digest can pin).
A call of scripts/fetch-pinned.sh is checked the same way statically: its URL
is expanded through the step's shell assignments and `${{ env.X }}` /
`${{ inputs.X }}` (the callers of a composite), and every value it can take must
be a line of .github/pinned-downloads.txt — or, with `--attested <owner/repo>`,
a release asset of <owner/repo>. The manifest itself must be well formed.

Fetches by other clients (check_sources):
  git clone          must be followed in its unit by a `git checkout` of a
                     40-hex commit (expanded through the step / job env), or
                     carry `# supply: head-tracked — <why>` on the line above
                     (a clone whose moving head IS the subject: the canary,
                     the grammar drift check);
  actions/checkout   of another `repository:` needs a 40-hex `ref:`;
  elan               `toolchain install` / `update` / `default <tc>` and
                     elan-init with a toolchain are refused — elan checks no
                     pinned digest; the archive goes through fetch-pinned.sh;
  opam               `init` / `repo add|set-url` must name
                     git+https://…#<40-hex>: the commit's opam files carry
                     every source archive's checksum;
  actions            every third-party action is declared in the manifest
                     (`action <owner/repo> <what it fetches at run time>`).

Usage: download_pins.py <root>
Exit 0 clean, 1 on a finding, 2 on a usage error.
"""
import os
import re
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from action_supply import env_values, resolve, step_inputs, workflow_files  # noqa: E402

MANIFEST = ".github/pinned-downloads.txt"
HELPER = "scripts/fetch-pinned.sh"
FETCH = re.compile(r"(?<![\w./-])(curl|wget|Invoke-WebRequest|iwr)(?![\w-])")
URL = re.compile(r"""https?://[^\s"'`)]+""")
ARCHIVE = re.compile(r"\.(tar(\.[a-z0-9]+)?|tgz|zip|xz|gz|bz2|zst|exe|msi|deb|rpm|sh|bin|AppImage)\b")
PIPE_INTO = re.compile(r"\|\s*(sudo\s+)?(tar|sh|bash|zsh|unzip|python3?|pwsh|iex)\b")
WRITES = re.compile(r"(^|\s)(-[A-Za-z]*[oO][A-Za-z]*|--output|--remote-name|-OutFile)(\s|=|$)")
HEX64 = re.compile(r"\b[0-9a-f]{64}\b")
SHA = re.compile(r"\bsha256sum\b|\bshasum\s+-a\s*256\b")
ATTEST = re.compile(r"gh\s+attestation\s+verify\b[^\n]*?(?:--repo|-R)[ =]+['\"]?([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)")
ACTIONS = {}  # owner/repo -> what it fetches at run time (manifest `action` lines)
SHA40 = re.compile(r"^[0-9a-f]{40}$")
STEP_START = re.compile(r"^\s*-\s+(name|uses|run|id|if|shell|env|with|working-directory|timeout-minutes|continue-on-error):")


def load_manifest(root, bad):
    path = os.path.join(root, MANIFEST)
    urls = {}
    if not os.path.exists(path):
        return urls
    for n, ln in enumerate(open(path, encoding="utf-8").read().splitlines(), 1):
        s = ln.strip()
        if not s or s.startswith("#"):
            continue
        am = re.fullmatch(r"action\s+([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)\s+(\S.*)", s)
        if am:
            ACTIONS[am.group(1)] = am.group(2)
            continue
        m = re.fullmatch(r"([0-9a-f]{64})  (https://\S+)", s)
        if not m:
            bad.append(f"{MANIFEST}:{n}  not `<sha256>  <https url>` or `action <owner/repo> <what it fetches>`: {s}")
            continue
        if m.group(2) in urls:
            bad.append(f"{MANIFEST}:{n}  {m.group(2)} is listed twice")
        urls[m.group(2)] = m.group(1)
    return urls


def logical_lines(text):
    """(first line number, joined text) with `\\` continuations joined, comments
    and heredoc bodies dropped."""
    out = []
    raw = text.splitlines()
    i = 0
    heredoc = None
    while i < len(raw):
        ln = raw[i]
        if heredoc is not None:
            if ln.strip() == heredoc:
                heredoc = None
            i += 1
            continue
        start = i
        joined = ln
        while joined.rstrip().endswith("\\") and i + 1 < len(raw):
            i += 1
            joined = joined.rstrip()[:-1] + " " + raw[i].strip()
        i += 1
        body = joined.lstrip()
        if body.startswith("#"):
            continue
        hm = re.search(r"<<-?\s*['\"]?([A-Za-z_]+)['\"]?", joined)
        if hm:
            heredoc = hm.group(1)
        out.append((start + 1, joined))
    return out


def fetches(text):
    """(line, command text, url or None) for every download in a unit."""
    out = []
    for ln, s in logical_lines(text):
        for m in FETCH.finditer(s):
            # The command runs to the next `;`, `&&` or `||`; a pipe stays.
            cmd = re.split(r"\s(?:&&|\|\|)\s|;\s", s[m.start():])[0]
            if re.search(r"""^(curl|wget|Invoke-WebRequest|iwr)\s*['"]""", cmd) or re.match(r"(curl|wget)\s*\(", cmd):
                continue
            url = URL.search(cmd)
            if url:
                u = url.group(0)
            else:  # the URL through a shell variable: `"${DOWNLOAD_URL}"`
                cand = [t.strip("\"'") for t in re.findall(r"""\S+""", cmd) if re.fullmatch(r"""["']?\$\{?[A-Za-z_][A-Za-z0-9_]*\}?["']?""", t)]
                u = cand[-1] if cand else None
            if re.search(r"-X\s*POST|--data\b|--fail-with-body\s+-X", cmd):
                continue
            tool = m.group(1)
            writes = tool == "wget" or bool(WRITES.search(cmd)) or bool(PIPE_INTO.search(cmd))
            if not writes:
                continue
            binary = (u is not None and "/releases/" in u) or bool(ARCHIVE.search(cmd)) or bool(PIPE_INTO.search(cmd))
            if binary:
                out.append((ln, cmd.strip(), u))
    return out


def matrix_values(lines, name):
    """Every value the file's matrices give `name` (`name: v`, `{ name: v }`, `name: [a, b]`)."""
    vals = []
    for ln in lines:
        for m in re.finditer(r"(?:^|[\s{,])" + re.escape(name) + r":\s*(\[[^\]]*\]|[^,}\s#]+)", ln):
            v = m.group(1)
            items = v[1:-1].split(",") if v.startswith("[") else [v]
            for it in items:
                it = it.strip().strip("\"'")
                if it and "${{" not in it and it not in vals:
                    vals.append(it)
    return vals


def shell_vars(text):
    """Every `NAME=value` assignment in a run block, quotes stripped."""
    vals = {}
    for m in re.finditer(r"(?m)(?:^|[;)]|\bthen|\belse)\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)=(\"[^\"]*\"|'[^']*'|[^\s;]*)", text):
        v = m.group(2)
        if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
            v = v[1:-1]
        vals.setdefault(m.group(1), []).append(v)
    return vals


def expand(root, f, lines, text, value, depth=0, loose=False):
    """Every concrete string `value` can take, or (None, why). `loose` puts
    `<?>` for a part it cannot resolve (enough to read a URL's host and repo)."""
    if depth > 6:
        return [(None, "expansion nests too deep")]
    m = re.search(r"\$\{\{\s*[^}]+?\s*\}\}", value)
    if m:
        out = []
        mm = re.fullmatch(r"\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}", m.group(0))
        cands = [(v, None) for v in matrix_values(lines, mm.group(1))] if mm else resolve(root, f, lines, m.group(0))
        if mm and not cands:
            cands = [(None, f"`matrix.{mm.group(1)}` has no value in the file")]
        if loose and any(v is None for v, _ in cands):
            cands = [("<?>", None)]
        for v, why in cands:
            if v is None:
                out.append((None, why))
            else:
                out.extend(expand(root, f, lines, text, value.replace(m.group(0), v), depth + 1, loose))
        return out
    m = re.search(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)", value)
    if m:
        name = m.group(1) or m.group(2)
        vals = shell_vars(text).get(name) or env_values(lines, name)
        if not vals and loose:
            vals = ["<?>"]
        if not vals:
            return [(None, f"`${name}` is not assigned in the step")]
        out = []
        for v in vals:
            # One value per name: every occurrence of the variable takes it.
            sub = re.sub(r"\$\{" + name + r"\}|\$" + name + r"(?![A-Za-z0-9_])", lambda _m: v, value)
            out.extend(expand(root, f, lines, text, sub, depth + 1, loose))
        return out
    return [(value, None)]


def helper_calls(text):
    """(line, attested repo or None, raw url arg) for every scripts/fetch-pinned.sh call."""
    out = []
    for ln, s in logical_lines(text):
        # The helper as a command (line start, after `;` / `&&` / `||` / `if !`,
        # optionally via `bash`) — not a mention inside an echo or a comment.
        m = re.search(r"(?:^|[;&|]|\bif\s+!?|\bthen|\bdo|^(?:-\s+)?run:)\s*(?:bash\s+)?[\w./${}\"'-]*fetch-pinned\.sh\s+(.*)$", s.strip())
        if not m:
            continue
        args = re.findall(r"\"[^\"]*\"|'[^']*'|\S+", m.group(1))
        args = [a[1:-1] if len(a) >= 2 and a[0] == a[-1] and a[0] in "\"'" else a for a in args]
        if args and args[0] == "--attested":
            if len(args) < 4:
                out.append((ln, "?", None))
            else:
                out.append((ln, args[1], args[2]))
        elif args:
            out.append((ln, None, args[0]))
    return out


def units(root):
    """(file, lines, unit text, first line) — workflow steps and whole scripts."""
    out = []
    for f in workflow_files(root):
        lines = open(f, encoding="utf-8").read().splitlines()
        starts = [i for i, ln in enumerate(lines) if STEP_START.match(ln)]
        starts.append(len(lines))
        if starts[0] != 0:
            starts.insert(0, 0)
        for a, b in zip(starts, starts[1:]):
            out.append((f, lines, "\n".join(lines[a:b]), a + 1))
    sdir = os.path.join(root, "scripts")
    for dirpath, _, names in os.walk(sdir):
        for n in sorted(names):
            if n.endswith(".sh"):
                p = os.path.join(dirpath, n)
                text = open(p, encoding="utf-8", errors="replace").read()
                out.append((p, text.splitlines(), text, 1))
    return out


def main(root):
    bad = []
    manifest = load_manifest(root, bad)
    n = 0
    for f, lines, text, first in units(root):
        rel = os.path.relpath(f, root)
        pinned = bool(SHA.search(text)) and (bool(HEX64.search(text)) or "pinned-downloads.txt" in text)
        attested = {m.group(1) for _, s in logical_lines(text) for m in ATTEST.finditer(s)}
        for ln, cmd, u in fetches(text):
            n += 1
            at = f"{rel}:{first + ln - 1}"
            if pinned:
                continue
            if attested and u:
                vals = expand(root, f, lines, text, u, loose=True)
                repos = {(re.match(r"https://github\.com/([^/]+/[^/]+)/releases/", v or "") or [None, None])[1] for v, _ in vals}
                if repos and repos <= attested:
                    continue
                bad.append(f"{at}  {cmd[:140]} — attested by {sorted(attested)}, but the URL is not a release asset of that repository")
                continue
            bad.append(f"{at}  {cmd[:140]} — no pinned sha256 (use {HELPER} with a line in {MANIFEST})")
        for ln, repo, raw in helper_calls(text):
            n += 1
            at = f"{rel}:{first + ln - 1}"
            if raw is None:
                bad.append(f"{at}  {HELPER} --attested without <owner/repo> <url> <out>")
                continue
            for v, why in expand(root, f, lines, text, raw):
                if v is None:
                    bad.append(f"{at}  {HELPER} {raw} — {why}")
                elif repo is not None:
                    if not v.startswith(f"https://github.com/{repo}/releases/"):
                        bad.append(f"{at}  {HELPER} --attested {repo} {v} — not a release asset of {repo}")
                elif v not in manifest:
                    bad.append(f"{at}  {HELPER} {v} — no line in {MANIFEST}")
    m = check_sources(root, bad)
    for b in bad:
        print("UNPINNED-DOWNLOAD " + b, file=sys.stderr)
    if bad:
        print(f"download-pins: {len(bad)} finding(s) ({n} download(s), {m} other fetch site(s) checked) — a fetched binary or archive must be checked against a pinned sha256, a fetched source pinned to a commit.", file=sys.stderr)
        return 1
    print(f"download-pins: OK — {n} download(s) of a binary or archive, every one checked against a pinned sha256 or a first-party attestation; {m} other fetch site(s) (git clone, a checkout of another repository, elan, opam, a third-party action) pinned to a commit, marked head-tracked, or declared")
    return 0


HEAD_TRACKED = re.compile(r"#\s*supply:\s*head-tracked\s*[—-]+\s*\S")
ELAN_FETCH = re.compile(r"(?<![\w-])elan[\"']?\s+(toolchain\s+install|update|default\s+(?!none\b))|elan-init\b[^\n]*--default-toolchain[ =](?!none\b)")


def check_sources(root, bad):
    """Fetches by other clients: `git clone` of a moving branch, an
    `actions/checkout` of another repository, elan's toolchain fetch, an opam
    repository at its head, and a third-party action not declared with what
    it fetches. Returns the number of sites checked."""
    n = 0
    for f, lines, text, first in units(root):
        rel = os.path.relpath(f, root)
        raw = text.splitlines()
        for ln, s in logical_lines(text):
            at = f"{rel}:{first + ln - 1}"
            prev = raw[ln - 2] if ln >= 2 else ""
            # A command inside a single-quoted string is text (a message, an
            # expected-output pattern), not a command this step runs.
            s = re.sub(r"'[^'\n]*'", "''", s)
            for m in re.finditer(r"(?<![\w-])git\s+clone\b", s):
                n += 1
                if HEAD_TRACKED.search(prev) or HEAD_TRACKED.search(s):
                    continue
                cos = re.findall(r"git\s+(?:-C\s+\S+\s+)?checkout\s+(?:-\S+\s+)*([\"']?[^\s;&|]+)", text)
                shas = [v for c in cos for v, _ in expand(root, f, lines, text, c.strip("\"'")) if v and SHA40.match(v)]
                if not shas:
                    bad.append(f"{at}  {s.strip()[:120]} — a clone of a moving branch: check out a 40-hex commit after it (or mark the line `# supply: head-tracked — <why>`)")
            if ELAN_FETCH.search(s):
                n += 1
                bad.append(f"{at}  {s.strip()[:120]} — elan fetches the toolchain against no pinned digest: unpack the release archive through {HELPER}")
            if re.search(r"(?<![\w-])opam\s+(init|repo(sitory)?\s+(add|set-url))\b", s):
                n += 1
                urls = [t.strip("\"'") for t in s.split() if "git+https://" in t]
                vals = [v for u in urls for v, _ in expand(root, f, lines, text, u)]
                if not vals or not all(v and re.fullmatch(r"git\+https://\S+#[0-9a-f]{40}", v) for v in vals):
                    bad.append(f"{at}  {s.strip()[:120]} — an opam repository at its moving head: name it as git+https://…#<40-hex commit>")
    for f in workflow_files(root):
        rel = os.path.relpath(f, root)
        lines = open(f, encoding="utf-8").read().splitlines()
        if True:
            for i, ln in enumerate(lines):
                um = re.match(r"^\s*(-\s+)?uses:\s*['\"]?([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)(/[^@\s'\"]*)?@", ln)
                if not um:
                    continue
                n += 1
                if um.group(2) not in ACTIONS:
                    bad.append(f"{rel}:{i + 1}  uses: {um.group(2)} — not declared in {MANIFEST} (`action {um.group(2)} <what it fetches at run time>`)")
                if um.group(2) == "actions/checkout":
                    ins = step_inputs(lines, i)
                    if "repository" in ins:
                        ref = ins.get("ref", (None, None))[0]
                        vals = [v for v, _ in expand(root, f, lines, "", ref)] if ref else [None]
                        if not vals or any(v is None or not SHA40.match(v) for v in vals):
                            bad.append(f"{rel}:{i + 1}  actions/checkout of {ins['repository'][0]} at `{ref}` — not a 40-hex commit")
    return n


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))

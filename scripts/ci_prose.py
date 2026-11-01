#!/usr/bin/env python3
"""Narrow prose-only classification and fail-closed exact-base compiler reuse.

No artifact from a PR/fork, another SHA, failed run, or missing digest is used.
Reuse failure means the existing build runs; it never means a passing build.
This helper does not change permissions or the existing docs-only classifier.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
import zipfile

SHA = re.compile(r"[0-9a-f]{40}\Z")
MAX_ZIP = 128 * 1024 * 1024
MAX_BINARY = 256 * 1024 * 1024
# Every file here was audited for readers outside the prose lane's own checks
# (include_str!, Rust/Almide tests, gates in jobs the lane skips). A reader
# found later is a reason to remove the path, not to widen the lane's checks.
ROOT_DOCS = {"README.md", "README.ja.md", "README.zh-CN.md", "CLAUDE.md", "CHANGELOG.md",
             "SUPPORT.md", "SECURITY.md"}
# docs/GRAMMAR.md is not here: crates/almide-syntax/tests/grammar_assign_rule.rs
# parses its `assign` rule.
TOP_DOCS = {"SPEC.md", "CHEATSHEET.md", "ARCHITECTURE.md",
            "status.md", "how-it-works.md", "written-in-almide.md"}
# Hand-written pages whose generated blocks are re-derived by Emit & Format.
# A change touching a block or a marker leaves the lane (structured_change).
DOCS_PAGES = {"docs/benchmarks/README.md"}
# docs/stdlib is not here: its pages are generated signature indexes that Rust
# tests parse (native_result_ownership, stdlib_inspection, name_keyed_sites).
DOCS_DIRS = {"roadmap", "specs", "design", "project"}
# HTML comments carry generated-block markers and roadmap metadata.
COMMENT = re.compile(r"<!--|-->")
BLOCK_START = re.compile(r"<!--.*(generated:start|BEGIN GENERATED|:\s*begin\s*-->)")
BLOCK_END = re.compile(r"<!--.*(generated:end|END GENERATED|:\s*end\s*-->)")
HUNK = re.compile(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@")


def prose_path(path):
    p = PurePosixPath(path)
    if p.is_absolute() or ".." in p.parts or "\n" in path or "\r" in path:
        return False
    if path in ROOT_DOCS or path in DOCS_PAGES:
        return True
    if len(p.parts) == 3 and p.parts[:2] == (".claude", "commands") and p.suffix == ".md":
        return True  # agent slash-command prompts; nothing in the build reads them
    if p.suffix != ".md" or not p.parts or p.parts[0] != "docs":
        return False
    if len(p.parts) == 2:
        return p.name in TOP_DOCS
    if path == "docs/project/CLAUDE_TEMPLATE.md" or path == "docs/specs/cli.md":
        return False  # compiled template / Rust-test-consumed toolchain text
    if path == "docs/roadmap/README.md":
        return False  # generated whole by docs/roadmap/generate-readme.sh
    if path.startswith("docs/specs/als/"):
        return False  # normative contract surfaces stay outside the narrow lane
    return p.parts[1] in DOCS_DIRS


def git(*args):
    return subprocess.check_output(["git", *args], stderr=subprocess.DEVNULL)


def blocks(text):
    """1-based line numbers inside generated blocks, markers included. An
    unclosed block runs to the end of the file."""
    inside, depth = set(), 0
    for number, line in enumerate(text.splitlines(), 1):
        if BLOCK_START.search(line):
            depth += 1
        if depth:
            inside.add(number)
        if BLOCK_END.search(line) and depth:
            depth -= 1
    return inside


def structured_change(base, head, path):
    """Whether a changed line is an HTML comment or lies inside a generated
    block, on either side of the diff."""
    def text(ref):
        if not git("--literal-pathspecs", "ls-tree", ref, "--", path):
            return ""
        return git("show", f"{ref}:{path}").decode("utf-8", "strict")
    old, new = blocks(text(base)), blocks(text(head))
    diff = git("--literal-pathspecs", "diff", "-U0", "--no-renames", "--no-ext-diff", "--no-color",
               base, head, "--", path).decode("utf-8", "strict")
    old_line = new_line = None
    for line in diff.splitlines():
        hunk = HUNK.match(line)
        if hunk:
            old_line, new_line = int(hunk.group(1)), int(hunk.group(2))
        elif old_line is None or line.startswith("\\"):
            continue
        elif line.startswith("-"):
            if old_line in old or COMMENT.search(line):
                return True
            old_line += 1
        elif line.startswith("+"):
            if new_line in new or COMMENT.search(line):
                return True
            new_line += 1
    return False


def classify_reason(base, head="HEAD"):
    if not SHA.fullmatch(base or "") or base == "0" * 40:
        return False, "no comparable base commit"
    try:
        git("rev-parse", "--verify", base + "^{commit}")
        git("merge-base", "--is-ancestor", base, head)
        paths = git("diff", "--name-only", "--no-renames", "-z", base, head).split(b"\0")
        paths = [p.decode("utf-8", "strict") for p in paths if p]
        if not paths:
            return False, "empty change set"
        for path in paths:
            if not prose_path(path):
                return False, f"not an allowlisted prose path: {path}"
        for path in paths:
            for ref in (base, head):
                row = git("--literal-pathspecs", "ls-tree", ref, "--", path)
                if row and not row.startswith(b"100644 blob "):
                    return False, f"not a regular file on both sides: {path}"
            if structured_change(base, head, path):
                return False, f"touches a generated block or HTML comment: {path}"
        return True, f"{len(paths)} allowlisted prose path(s)"
    except (subprocess.CalledProcessError, UnicodeError):
        return False, "range or file unreadable"


def classify(base, head="HEAD"):
    return classify_reason(base, head)[0]


def event_base(event_name, event):
    if event_name == "pull_request":
        return event.get("pull_request", {}).get("base", {}).get("sha", "")
    if event_name == "merge_group":
        return event.get("merge_group", {}).get("base_sha", "")
    if event_name == "push":
        return event.get("before", "")
    return ""


def output(key, value):
    line = f"{key}={value}\n"
    print(line, end="")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as f:
            f.write(line)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class GitHub:
    def __init__(self, repo, token):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
            raise ValueError("invalid repository")
        self.root = "https://api.github.com/repos/" + repo
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect())

    def request(self, suffix):
        return urllib.request.Request(self.root + suffix, headers={
            "Authorization": "Bearer " + self.token,
            "Accept": "application/vnd.github+json", "User-Agent": "almide-prose-ci",
            "X-GitHub-Api-Version": "2022-11-28"})

    def json(self, suffix):
        with self.opener.open(self.request(suffix), timeout=30) as r:
            return json.loads(r.read(8 * 1024 * 1024))

    def archive(self, artifact_id):
        try:
            self.opener.open(self.request(f"/actions/artifacts/{artifact_id}/zip"), timeout=30)
        except urllib.error.HTTPError as e:
            if e.code != 302:
                raise ValueError("artifact redirect unavailable") from None
            location = e.headers.get("Location", "")
        else:
            raise ValueError("unexpected artifact download response")
        url = urllib.parse.urlsplit(location)
        host = url.hostname or ""
        if (url.scheme != "https" or url.username or url.password or
                not host.endswith((".blob.core.windows.net", ".amazonaws.com", ".actions.githubusercontent.com"))):
            raise ValueError("unexpected artifact download host")
        # Never forward the GitHub token to the signed storage URL. No second redirect.
        req = urllib.request.Request(location, headers={"User-Agent": "almide-prose-ci"})
        with self.opener.open(req, timeout=60) as r:
            blob = r.read(MAX_ZIP + 1)
        if len(blob) > MAX_ZIP:
            raise ValueError("artifact too large")
        return blob


def select_artifact(api, base):
    if not SHA.fullmatch(base or ""):
        raise ValueError("invalid base SHA")
    repo_id = api.json("")["id"]
    runs = api.json("/actions/workflows/ci.yml/runs?event=push&status=success&per_page=100&head_sha=" + base)
    for run in runs.get("workflow_runs", []):
        if not (run.get("head_sha") == base and run.get("event") == "push" and
                run.get("head_branch") in {"develop", "main"} and
                run.get("status") == "completed" and run.get("conclusion") == "success" and
                run.get("path") == ".github/workflows/ci.yml" and
                run.get("repository", {}).get("id") == repo_id and
                run.get("head_repository", {}).get("id") == repo_id):
            continue
        artifacts = api.json(f"/actions/runs/{run['id']}/artifacts?per_page=100")
        for artifact in artifacts.get("artifacts", []):
            source = artifact.get("workflow_run", {})
            digest = artifact.get("digest", "")
            if (artifact.get("name") == "almide-linux" and not artifact.get("expired", True) and
                    0 < artifact.get("size_in_bytes", 0) <= MAX_ZIP and
                    re.fullmatch(r"sha256:[0-9a-f]{64}", digest) and
                    source.get("id") == run["id"] and source.get("head_sha") == base and
                    source.get("repository_id") == repo_id and source.get("head_repository_id") == repo_id):
                return artifact
    raise ValueError("no successful same-repository exact-base compiler artifact")


def verified_binary(blob, digest):
    if "sha256:" + hashlib.sha256(blob).hexdigest() != digest:
        raise ValueError("artifact digest mismatch")
    with zipfile.ZipFile(io.BytesIO(blob)) as z:
        entries = z.infolist()
        if len(entries) != 1 or entries[0].filename != "almide":
            raise ValueError("unexpected archive layout")
        info = entries[0]
        mode = info.external_attr >> 16
        if info.is_dir() or stat.S_ISLNK(mode) or not 0 < info.file_size <= MAX_BINARY:
            raise ValueError("unsafe artifact member")
        binary = z.read(info)
    if not binary.startswith(b"\x7fELF"):
        raise ValueError("expected Linux compiler executable")
    return binary


def reuse(api, base, destination):
    artifact = select_artifact(api, base)
    binary = verified_binary(api.archive(artifact["id"]), artifact["digest"])
    target = Path(destination)
    if target.is_symlink() or not target.resolve().is_relative_to(Path.cwd().resolve()):
        raise ValueError("output must remain in the checkout")
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(target.name + ".prose-tmp")
    with temporary.open("xb") as f:
        f.write(binary)
    temporary.chmod(0o755)
    temporary.replace(target)
    return artifact["id"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["classify", "reuse"])
    parser.add_argument("--base", default="")
    parser.add_argument("--output", default="target/release/almide")
    args = parser.parse_args()
    if args.command == "classify":
        try:
            event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
            base = event_base(os.environ.get("GITHUB_EVENT_NAME", ""), event)
            ok, reason = classify_reason(base)
        except (OSError, ValueError, KeyError):
            base, ok, reason = "", False, "event unreadable"
        # json.dumps: a path cannot smuggle a newline-led workflow command.
        print("prose lane: " + json.dumps(reason))
        output("prose_only", str(ok).lower())
        output("base_sha", base if ok else "")
    else:
        try:
            api = GitHub(os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_TOKEN"])
            artifact = reuse(api, args.base, args.output)
        except Exception as e:
            # No signed URLs, authorization headers, or token-bearing exception text.
            print(f"prose compiler reuse unavailable ({type(e).__name__}); normal build remains required")
            output("reused", "false")
        else:
            print(f"verified exact-base compiler artifact {artifact}")
            output("reused", "true")


if __name__ == "__main__":
    main()

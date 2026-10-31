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
ROOT_DOCS = {"README.md", "README.ja.md", "README.zh-CN.md", "CLAUDE.md", "CHANGELOG.md"}
TOP_DOCS = {"SPEC.md", "CHEATSHEET.md", "ARCHITECTURE.md", "GRAMMAR.md",
            "status.md", "how-it-works.md", "written-in-almide.md"}


def prose_path(path):
    p = PurePosixPath(path)
    if p.is_absolute() or ".." in p.parts or "\n" in path or "\r" in path:
        return False
    if path in ROOT_DOCS:
        return True
    if p.suffix != ".md" or not p.parts or p.parts[0] != "docs":
        return False
    if len(p.parts) == 2:
        return p.name in TOP_DOCS
    if path == "docs/project/CLAUDE_TEMPLATE.md" or path == "docs/specs/cli.md":
        return False  # compiled template / Rust-test-consumed toolchain text
    if path.startswith("docs/specs/als/"):
        return False  # normative contract surfaces stay outside the narrow lane
    return p.parts[1] in {"roadmap", "stdlib", "specs", "design", "project"}


def git(*args):
    return subprocess.check_output(["git", *args], stderr=subprocess.DEVNULL)


def classify(base, head="HEAD"):
    if not SHA.fullmatch(base or "") or base == "0" * 40:
        return False
    try:
        git("rev-parse", "--verify", base + "^{commit}")
        git("merge-base", "--is-ancestor", base, head)
        paths = git("diff", "--name-only", "--no-renames", "-z", base, head).split(b"\0")
        paths = [p.decode("utf-8", "strict") for p in paths if p]
        if not paths or not all(prose_path(p) for p in paths):
            return False
        for path in paths:
            for ref in (base, head):
                row = git("--literal-pathspecs", "ls-tree", ref, "--", path)
                if row and not row.startswith(b"100644 blob "):
                    return False  # symlink, executable, submodule, or mode change
        return True
    except (subprocess.CalledProcessError, UnicodeError):
        return False


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
            ok = classify(base)
        except (OSError, ValueError, KeyError):
            base, ok = "", False
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

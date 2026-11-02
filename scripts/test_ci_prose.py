#!/usr/bin/env python3
"""Offline regression controls for the narrow prose CI lane. No network calls."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock
import urllib.error
import zipfile

import ci_prose as ci

BASE = "a" * 40
REPO_ID = 123


def archive(name="almide", binary=b"\x7fELFtest", symlink=False):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as z:
        info = zipfile.ZipInfo(name)
        info.external_attr = ((0o120777 if symlink else 0o100755) << 16)
        z.writestr(info, binary)
    return buffer.getvalue()


class API:
    def __init__(self):
        self.blob = archive()
        self.run = {"id": 7, "head_sha": BASE, "event": "push", "head_branch": "develop",
                    "status": "completed", "conclusion": "success", "path": ".github/workflows/ci.yml",
                    "repository": {"id": REPO_ID}, "head_repository": {"id": REPO_ID}}
        self.artifact = {"id": 9, "name": "almide-linux", "expired": False,
                         "size_in_bytes": len(self.blob), "digest": "sha256:" + hashlib.sha256(self.blob).hexdigest(),
                         "workflow_run": {"id": 7, "head_sha": BASE,
                                          "repository_id": REPO_ID, "head_repository_id": REPO_ID}}

    def json(self, path):
        if not path:
            return {"id": REPO_ID}
        if path.startswith("/actions/workflows/"):
            return {"workflow_runs": [self.run]}
        return {"artifacts": [self.artifact]}

    def archive(self, artifact_id):
        assert artifact_id == 9
        return self.blob


class ProseTests(unittest.TestCase):
    def test_real_workflow_bootstrap_fail_closed(self):
        # Execute the literal YAML shell without a YAML dependency in CI.
        root = Path(__file__).resolve().parents[1]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        start = workflow.index("          # Classify with code from the compared base")
        end = workflow.index("\n\n", start)
        shell = "\n".join(line[10:] for line in workflow[start:end].splitlines())
        helper = (root / "scripts/ci_prose.py").read_text()
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory) / "repo"; repo.mkdir()
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=repo, stderr=subprocess.DEVNULL).decode().strip()
            git("init", "-q"); git("config", "user.name", "test")
            git("config", "user.email", "test@example.invalid")
            (repo / "README.md").write_text("base")
            git("add", "."); git("commit", "-qm", "base without helper")
            first = git("rev-parse", "HEAD")
            (repo / "scripts").mkdir()
            (repo / "scripts/ci_prose.py").write_text(helper)
            git("add", "."); git("commit", "-qm", "install helper")
            installed = git("rev-parse", "HEAD")
            def run(event, payload):
                event_path = Path(directory) / "event.json"
                out = Path(directory) / "output"; out.write_text("")
                event_path.write_text(json.dumps(payload))
                env = dict(os.environ, GITHUB_EVENT_NAME=event, GITHUB_EVENT_PATH=str(event_path),
                           GITHUB_OUTPUT=str(out), RUNNER_TEMP=directory, PYTHONPATH=str(repo))
                result = subprocess.run(["bash", "-e", "-c", shell], cwd=repo, env=env,
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                return out.read_text()
            self.assertIn("prose_only=false", run("pull_request", {"pull_request": {"base": {"sha": first}}}))
            (repo / "README.md").write_text("prose")
            git("add", "."); git("commit", "-qm", "prose")
            # A hostile untracked import must not shadow isolated stdlib imports.
            (repo / "json.py").write_text("raise RuntimeError('repository import')")
            self.assertIn("prose_only=true", run("pull_request", {"pull_request": {"base": {"sha": installed}}}))
            self.assertIn("prose_only=false", run("merge_group", {"merge_group": {"base_sha": "invalid"}}))
            self.assertIn("prose_only=false", run("workflow_dispatch", {}))
            (repo / "source.rs").write_text("code")
            git("add", "source.rs"); git("commit", "-qm", "mixed queue source")
            self.assertIn("prose_only=false", run("merge_group", {"merge_group": {"base_sha": installed}}))
            (repo / "scripts/ci_prose.py").write_text("print('prose_only=true')")
            git("add", "scripts/ci_prose.py"); git("commit", "-qm", "edited classifier")
            self.assertIn("prose_only=false", run("pull_request", {"pull_request": {"base": {"sha": installed}}}))

    def test_allowlist_is_narrow(self):
        for path in ["README.md", "CLAUDE.md", "SUPPORT.md", "SECURITY.md", "docs/SPEC.md",
                     "docs/roadmap/active/foo.md", "docs/benchmarks/README.md", ".claude/commands/almide-release.md"]:
            with self.subTest(path=path):
                self.assertTrue(ci.prose_path(path))
        for path in ["src/main.rs", "stdlib/list.almd", "Cargo.lock", "scripts/foo.sh", ".github/workflows/ci.yml",
                     "docs/diagnostics/E001.md", "docs/project/CLAUDE_TEMPLATE.md", "docs/specs/cli.md",
                     "docs/specs/als/semantics.md", "docs/contracts/README.md", "proofs/TOR.md", "spec/a.md",
                     "examples/a.md", "research/a.md", "llms.txt", "docs/../src/main.md", "docs/stdlib/a\nb.md",
                     # test inputs and generated pages
                     "docs/GRAMMAR.md", "docs/stdlib/list.md", "docs/stdlib/README.md", "docs/roadmap/README.md",
                     # data beside the measurements page, and the rest of .claude/
                     "docs/benchmarks/build-speed.txt", "docs/benchmarks/fan-floor.md",
                     ".claude/settings.json", ".claude/commands/nested/a.md", ".claude/a.md"]:
            with self.subTest(path=path):
                self.assertFalse(ci.prose_path(path))

    def test_generated_blocks_and_comments_leave_the_lane(self):
        page = "docs/benchmarks/README.md"
        original = "\n".join([
            "# Measurements", "", "Intro prose.", "",
            "<!-- stats:generated:start — derived; DO NOT EDIT between the markers -->",
            "| total | 12 |",
            "<!-- counts:generated:start (as of 2026-10-21) -->", "| counted | 3 |", "<!-- counts:generated:end -->",
            "<!-- stats:generated:end -->", "", "Closing prose.", ""])
        def edit(old, new):
            return lambda text: text.replace(old, new, 1)
        cases = [
            ("prose line", edit("Intro prose.", "Intro prose, clarified."), True),
            ("line after a block", edit("<!-- stats:generated:end -->\n", "<!-- stats:generated:end -->\nNew prose.\n"), True),
            ("cell in a block", edit("| total | 12 |", "| total | 13 |"), False),
            ("cell in a nested block", edit("| counted | 3 |", "| counted | 4 |"), False),
            ("line inserted in a block", edit("| total | 12 |\n", "| total | 12 |\n| extra | 1 |\n"), False),
            ("end marker", edit("<!-- stats:generated:end -->", "<!-- stats:generated:end  -->"), False),
            ("block deleted", lambda t: t[:t.index("<!-- stats")] + t[t.index("Closing"):], False),
            ("new comment", edit("Closing prose.", "Closing prose. <!-- description: x -->"), False),
            ("new block", edit("Closing prose.", "<!-- x:generated:start -->\nrow\n<!-- x:generated:end -->"), False),
        ]
        with tempfile.TemporaryDirectory() as directory:
            previous = Path.cwd()
            os.chdir(directory)
            try:
                def git(*args):
                    return subprocess.check_output(["git", *args], stderr=subprocess.DEVNULL).decode().strip()
                git("init", "-q"); git("config", "user.name", "test")
                git("config", "user.email", "test@example.invalid")
                Path("docs/benchmarks").mkdir(parents=True)
                Path(page).write_text(original)
                git("add", "."); git("commit", "-qm", "base")
                base = git("rev-parse", "HEAD")
                for label, change, expected in cases:
                    with self.subTest(case=label):
                        git("reset", "-q", "--hard", base)
                        Path(page).write_text(change(original))
                        git("commit", "-qam", label)
                        ok, reason = ci.classify_reason(base)
                        self.assertEqual(ok, expected, reason)
                # An unclosed block runs to the end of the file.
                self.assertEqual(ci.blocks("a\n<!-- y:generated:start -->\nb\nc"), {2, 3, 4})
                self.assertEqual(ci.blocks("<!-- almide switches --md: begin -->\nrow\n<!-- almide switches --md: end -->\nz"),
                                 {1, 2, 3})
            finally:
                os.chdir(previous)

    def test_docs_only_treats_test_inputs_as_code(self):
        root = Path(__file__).resolve().parents[1]
        def verdict(path):
            out = subprocess.run(["bash", str(root / "scripts/check-docs-only.sh"), "--stdin"], input=path + "\n",
                                 text=True, capture_output=True, env={"PATH": os.environ["PATH"]})
            return out.stdout.splitlines()[0]
        for path in ["docs/GRAMMAR.md", "docs/stdlib/list.md", "docs/specs/cli.md", "docs/diagnostics/E001.md"]:
            with self.subTest(path=path):
                self.assertEqual(verdict(path), "docs_only=false")
        for path in ["README.md", "SUPPORT.md", "docs/benchmarks/README.md", ".claude/commands/almide-release.md"]:
            with self.subTest(path=path):
                self.assertEqual(verdict(path), "docs_only=true")

    def test_emit_and_format_lane_skips_only_the_audited_steps(self):
        # Every step the lane skips reads no admitted path. Guarding another step
        # means auditing what it reads first, then naming it here.
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        job = workflow[workflow.index("  checks:\n    name: Emit & Format"):workflow.index("\n  perf-ratchet:\n")]
        self.assertIn("    needs: [changes, build]\n    if: ${{ !cancelled() && needs.build.result == 'success' }}\n", job)
        self.assertIn("PROSE_LANE: ${{ needs.changes.outputs.prose_only == 'true' && "
                      "needs.changes.outputs.docs_only == 'true' }}", job)
        steps = job.split("\n      - ")[1:]
        skipped = {step.splitlines()[0] for step in steps if "        if: env.PROSE_LANE != 'true'" in step}
        self.assertEqual(skipped, {
            "name: Doc fences execute (#1490 — marked stdlib examples check/run byte-exactly)",
            "uses: ./.github/actions/setup-node-pinned",
            "name: Install wasm-opt (binaryen) for the JS host gate's post-opt shim-set assertion (#2276)",
            "name: JS host (#2265/#2276 — `--host js` glue runs the wasm_host_js fixtures under node, stdout byte-equal "
            "to expected and native; the module and glue ship only the program's surface)",
            "name: Stability closure standing (#1485 — informational, never blocks)",
            "name: Installers refuse a release whose attestation they could not verify, unless --skip-attestation "
            "(security row 16)",
            "name: Release evidence seals (every seal re-measured against its tag — the audit freeze)",
            "name: Interface differ fires on a removed effect fn (negative controls, #1860)",
            "name: Pass-isolated fixtures (#1487 — byte-identical under full / ablated / only-pass, native == wasm)",
            "name: Pass-order shuffle (#2186 — byte-identical Rust under every order the declared dependency edges permit)",
            "name: Pass-order shuffle negative controls (#2186 — an order-dependent or panicking stand-in turns the gate red)",
            "name: Ownership certifier (#2231 — every corpus violation is in the shrink-only ledger, none new, none "
            "silently gone)",
            "name: Ownership certifier negative controls (#2231 — a new violation, a vanished one, a panic and an "
            "empty corpus each turn the gate red)",
            "name: Spec test files check and are run by a runner (#3065)",
            "name: Spec test files gate negative control (#3065)",
        })
        self.assertTrue(all("PROSE_LANE" not in step for step in steps if "PROSE_LANE != 'true'" not in step
                            and not step.startswith("name: Lane")))

    def test_no_new_compiler_or_test_reader_of_an_admitted_path(self):
        # A Rust or Almide source naming an admitted path is a consumer the lane
        # would stop testing. The known ones are read by checks the lane keeps,
        # or name the path without reading the repository's copy.
        known = {
            "src/cli/docs_gen.rs",                   # docs-gen --check, kept
            "tools/almide-gates/src/main.almd",      # bench --check-readme kept; roadmap index twin kept
            "src/cli/commands.rs",                   # writes CLAUDE.md into a user's project
            "crates/almide-corpus/tests/manifest_generators_refuse_stale_tree.rs",  # scratch-repo README
            "tests/dep_git_subdir_test.rs",          # scratch-repo README
            "spec/stdlib/path_components_test.almd",  # a path string, not a file read
        }
        needles = sorted(ci.ROOT_DOCS | ci.DOCS_PAGES | {"docs/" + n for n in ci.TOP_DOCS} |
                         {"docs/" + d for d in ci.DOCS_DIRS} | {".claude/commands"})
        # A string literal starting at the path (after ../ or /), not a comment
        # citing it; a literal naming an excluded file is not a reader of the lane.
        pattern = '"[./]*(' + "|".join(re.escape(n) for n in needles) + ')[^"]*'
        args = ["git", "grep", "-o", "-E", "-e", pattern, "--", "*.rs", "*.almd"]
        found = subprocess.run(args, cwd=Path(__file__).resolve().parents[1], text=True, capture_output=True)
        self.assertIn(found.returncode, (0, 1), found.stderr)
        readers = set()
        for row in found.stdout.splitlines():
            path, literal = row.split(":", 1)
            literal = re.sub(r"^\"(\.\./|\./|/)*", "", literal).rstrip("/")
            if (ci.prose_path(literal) or literal in {"docs/" + d for d in ci.DOCS_DIRS}) and \
                    not path.startswith("crates/almide-codegen/src/generated/"):
                readers.add(path)
        self.assertEqual(readers - known, set(), "audit the new reader, then exclude the path or name it here")
        self.assertEqual(known - readers, set(), "a listed reader no longer names an admitted path; drop it")

    def test_event_bases(self):
        self.assertEqual(ci.event_base("pull_request", {"pull_request": {"base": {"sha": BASE}}}), BASE)
        self.assertEqual(ci.event_base("merge_group", {"merge_group": {"base_sha": BASE}}), BASE)
        self.assertEqual(ci.event_base("push", {"before": BASE}), BASE)
        self.assertEqual(ci.event_base("workflow_dispatch", {"before": BASE}), "")

    def test_real_git_range_mixed_rename_symlink_and_empty(self):
        with tempfile.TemporaryDirectory() as directory:
            previous = Path.cwd()
            os.chdir(directory)
            try:
                def git(*args):
                    return subprocess.check_output(["git", *args], stderr=subprocess.DEVNULL).decode().strip()
                git("init", "-q")
                git("config", "user.name", "test")
                git("config", "user.email", "test@example.invalid")
                Path("README.md").write_text("old")
                Path("source.rs").write_text("code")
                git("add", "."); git("commit", "-qm", "base")
                base = git("rev-parse", "HEAD")
                self.assertFalse(ci.classify(base))
                Path("README.md").write_text("new")
                git("add", "."); git("commit", "-qm", "prose")
                self.assertTrue(ci.classify(base))
                self.assertFalse(ci.classify("0" * 40))
                self.assertFalse(ci.classify("f" * 40))
                git("mv", "source.rs", "CHANGELOG.md"); git("commit", "-qm", "renamed code")
                self.assertFalse(ci.classify(base))
                git("reset", "--hard", base)
                Path("README.md").unlink(); Path("README.md").symlink_to("source.rs")
                git("add", "."); git("commit", "-qm", "symlink")
                self.assertFalse(ci.classify(base))
            finally:
                os.chdir(previous)

    def test_exact_successful_artifact_selected(self):
        self.assertEqual(ci.select_artifact(API(), BASE)["id"], 9)

    def test_run_provenance_negative_controls(self):
        mutations = [("head_sha", "b" * 40), ("event", "pull_request"), ("head_branch", "feature"),
                     ("status", "in_progress"), ("conclusion", "failure"), ("path", ".github/workflows/other.yml"),
                     ("repository", {"id": 999}), ("head_repository", {"id": 999})]
        for key, value in mutations:
            with self.subTest(key=key):
                api = API(); api.run[key] = value
                with self.assertRaises(ValueError):
                    ci.select_artifact(api, BASE)

    def test_artifact_metadata_negative_controls(self):
        mutations = [("name", "other"), ("expired", True), ("digest", ""),
                     ("size_in_bytes", 0), ("size_in_bytes", ci.MAX_ZIP + 1)]
        for key, value in mutations:
            with self.subTest(key=key):
                api = API(); api.artifact[key] = value
                with self.assertRaises(ValueError):
                    ci.select_artifact(api, BASE)
        for key in ["id", "head_sha", "repository_id", "head_repository_id"]:
            with self.subTest(source=key):
                api = API(); api.artifact["workflow_run"][key] = "wrong"
                with self.assertRaises(ValueError):
                    ci.select_artifact(api, BASE)

    def test_archive_digest_and_layout(self):
        api = API()
        self.assertEqual(ci.verified_binary(api.blob, api.artifact["digest"]), b"\x7fELFtest")
        with self.assertRaises(ValueError):
            ci.verified_binary(api.blob, "sha256:" + "0" * 64)
        for blob in [archive("../almide"), archive(symlink=True), archive(binary=b"not an ELF")]:
            with self.subTest(blob=hashlib.sha256(blob).hexdigest()):
                with self.assertRaises(ValueError):
                    ci.verified_binary(blob, "sha256:" + hashlib.sha256(blob).hexdigest())

    def test_token_not_forwarded_to_storage(self):
        api = ci.GitHub("owner/repo", "secret-token")
        requests = []
        def open_request(request, timeout):
            requests.append(request)
            if len(requests) == 1:
                raise urllib.error.HTTPError(request.full_url, 302, "redirect",
                                             {"Location": "https://test.blob.core.windows.net/artifact?signature=private"}, None)
            return io.BytesIO(b"zip")
        api.opener.open = open_request
        self.assertEqual(api.archive(9), b"zip")
        self.assertEqual(requests[0].get_header("Authorization"), "Bearer secret-token")
        self.assertIsNone(requests[1].get_header("Authorization"))

    def test_untrusted_redirect_refused(self):
        api = ci.GitHub("owner/repo", "secret-token")
        with mock.patch.object(api.opener, "open", side_effect=urllib.error.HTTPError("api", 302, "redirect", {"Location": "http://localhost/x"}, None)):
            with self.assertRaises(ValueError):
                api.archive(9)

    def test_reuse_failure_selects_normal_build_and_redacts_error(self):
        stdout = io.StringIO()
        with mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": "owner/repo", "GITHUB_TOKEN": "secret-token"}, clear=True), \
                mock.patch("sys.argv", ["ci_prose.py", "reuse", "--base", BASE]), \
                mock.patch.object(ci, "reuse", side_effect=ValueError("secret-token signed-url")), contextlib.redirect_stdout(stdout):
            ci.main()
        self.assertIn("reused=false", stdout.getvalue())
        self.assertNotIn("secret-token", stdout.getvalue())
        self.assertNotIn("signed-url", stdout.getvalue())

    def test_bad_event_is_full_build(self):
        stdout = io.StringIO()
        with mock.patch.dict(os.environ, {}, clear=True), mock.patch("sys.argv", ["ci_prose.py", "classify"]), contextlib.redirect_stdout(stdout):
            ci.main()
        self.assertIn("prose_only=false", stdout.getvalue())


if __name__ == "__main__":
    unittest.main()

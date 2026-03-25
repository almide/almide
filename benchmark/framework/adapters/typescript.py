"""TypeScript language adapter — runs tests via deno test."""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path
from dataclasses import dataclass


@dataclass
class TestResult:
    success: bool
    stdout: str
    stderr: str
    compile_error: bool
    test_failures: int
    timeout: bool


class TypeScriptAdapter:
    name = "typescript"
    extension = ".ts"

    @staticmethod
    def compile_and_test(
        source_code: str,
        test_path: Path,
        *,
        timeout: int = 30,
    ) -> TestResult:
        """Write source_code to a module file, run deno test against the test file."""
        tmpdir = Path(tempfile.mkdtemp(prefix="bench_ts_"))
        source_file = tmpdir / "solution.ts"
        source_file.write_text(source_code)

        # Copy test file and fix imports
        test_dest = tmpdir / test_path.name
        test_content = test_path.read_text()
        test_content = test_content.replace(
            f'from "./{test_path.stem.replace(".test", "")}.ts"',
            'from "./solution.ts"',
        )
        test_content = test_content.replace(
            f"from './{test_path.stem.replace('.test', '')}.ts'",
            "from './solution.ts'",
        )
        test_dest.write_text(test_content)

        try:
            result = subprocess.run(
                ["deno", "test", "--allow-read", "--allow-write", str(test_dest)],
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=str(tmpdir),
            )
            return TestResult(
                success=result.returncode == 0,
                stdout=result.stdout,
                stderr=result.stderr,
                compile_error=_is_compile_error(result.stderr),
                test_failures=_count_failures(result.stdout + result.stderr),
                timeout=False,
            )
        except subprocess.TimeoutExpired:
            return TestResult(
                success=False,
                stdout="",
                stderr=f"timeout after {timeout}s",
                compile_error=False,
                test_failures=0,
                timeout=True,
            )
        finally:
            import shutil
            shutil.rmtree(tmpdir, ignore_errors=True)


def _is_compile_error(stderr: str) -> bool:
    return "error: " in stderr.lower() and "TS" in stderr


def _count_failures(output: str) -> int:
    count = 0
    for line in output.splitlines():
        if line.strip().startswith("FAILED") or "... FAILED" in line:
            count += 1
    return count

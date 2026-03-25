"""Almide language adapter — compiles and tests .almd files via `almide test`."""

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


class AlmideAdapter:
    name = "almide"
    extension = ".almd"

    @staticmethod
    def compile_and_test(
        source_code: str,
        test_path: Path,
        *,
        timeout: int = 30,
    ) -> TestResult:
        """Write source_code to a temp file that includes the tests, then run `almide test`.

        For Almide, tests are inline — we concatenate source + test file content
        into a single .almd file and execute it.
        """
        test_content = test_path.read_text()

        with tempfile.NamedTemporaryFile(
            suffix=".almd", mode="w", delete=False
        ) as f:
            # Source first, then tests (tests reference functions from source)
            f.write(source_code.rstrip("\n"))
            f.write("\n\n")
            f.write(test_content)
            f.flush()
            tmp = Path(f.name)

        try:
            result = subprocess.run(
                ["almide", "test", str(tmp)],
                capture_output=True,
                text=True,
                timeout=timeout,
            )
            return TestResult(
                success=result.returncode == 0,
                stdout=result.stdout,
                stderr=result.stderr,
                compile_error=_is_compile_error(result.stderr) if result.returncode != 0 else False,
                test_failures=_count_failures(result.stdout, result.stderr),
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
            tmp.unlink(missing_ok=True)

    @staticmethod
    def prepare_source(source_code: str, test_path: Path) -> str:
        """For Almide, source and tests live in the same file.

        Return the combined content that `almide test` will execute.
        """
        test_content = test_path.read_text()
        return source_code.rstrip("\n") + "\n\n" + test_content


def _is_compile_error(stderr: str) -> bool:
    lower = stderr.lower()
    return "error" in lower and "test" not in lower.split("error")[0][-20:]


def _count_failures(stdout: str, stderr: str) -> int:
    combined = stdout + stderr
    count = 0
    for line in combined.splitlines():
        if "FAIL" in line or "failed" in line.lower():
            count += 1
    return count

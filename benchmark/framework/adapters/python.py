"""Python language adapter — runs tests via pytest."""

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


class PythonAdapter:
    name = "python"
    extension = ".py"

    @staticmethod
    def compile_and_test(
        source_code: str,
        test_path: Path,
        *,
        timeout: int = 30,
    ) -> TestResult:
        """Write source_code to a module file, run pytest against the test file."""
        tmpdir = Path(tempfile.mkdtemp(prefix="bench_py_"))
        source_file = tmpdir / "solution.py"
        source_file.write_text(source_code)

        # Copy test file into tmpdir so it can `from solution import ...`
        test_dest = tmpdir / test_path.name
        test_content = test_path.read_text()
        # Replace any import of the module name with solution
        test_content = test_content.replace(
            f"from {test_path.stem.replace('test_', '')} import",
            "from solution import",
        )
        test_dest.write_text(test_content)

        try:
            result = subprocess.run(
                ["python3", "-m", "pytest", str(test_dest), "-v", "--tb=short"],
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=str(tmpdir),
            )
            return TestResult(
                success=result.returncode == 0,
                stdout=result.stdout,
                stderr=result.stderr,
                compile_error=_is_syntax_error(result.stdout + result.stderr),
                test_failures=_count_failures(result.stdout),
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


def _is_syntax_error(output: str) -> bool:
    return "SyntaxError" in output or "IndentationError" in output


def _count_failures(stdout: str) -> int:
    for line in stdout.splitlines():
        # pytest summary: "1 failed, 2 passed"
        if "failed" in line and ("passed" in line or "error" in line):
            for part in line.split():
                if part.isdigit():
                    return int(part)
    return 0

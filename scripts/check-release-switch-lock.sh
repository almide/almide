#!/usr/bin/env bash
# RELEASE SWITCH-LOCK CHECK (security row 25): a release binary locks the
# build-changing switches with NO flag on the command line (#4129).
#
# tests/switch_lock_test.rs drives the lock through `--lock-switches`, because
# the binary under test is a dev build. What a user downloads engages the lock
# by default instead, from its version line (`almide_base::env::
# lock_engaged_by_default`, unit-tested in every CI run). This script checks
# the artifact itself: `.github/workflows/release.yml` runs it on every build
# it can execute, before the archive is packaged, so a release whose binary
# would honour an inherited switch is never published.
#
#   1. `--version` names a release build (else this is the wrong binary);
#   2. a project pinned to `almide = "999.0.0"` fails `check` (the pin holds);
#   3. with ALMIDE_SKIP_VERSION_CHECK=1 inherited and no flag, it STILL fails
#      and says the switch was IGNORED — the default engagement;
#   4. control: `--allow-switch ALMIDE_SKIP_VERSION_CHECK` honours it, so step 3
#      failed because of the lock, not because the switch does nothing.
#
# Usage: scripts/check-release-switch-lock.sh <almide binary>
set -uo pipefail
bin="${1:?usage: check-release-switch-lock.sh <almide binary>}"
[ -x "$bin" ] || { echo "::error::not executable: $bin" >&2; exit 2; }

ver="$("$bin" --version 2>&1)"
case "$ver" in
  *"(release"*) ;;
  *) echo "::error::$bin is not a release build (\`--version\`: $ver) — this check needs the release workflow's artifact" >&2; exit 1 ;;
esac

d="$(mktemp -d)"; trap 'rm -rf "$d"' EXIT
mkdir -p "$d/src"
printf '[package]\nname = "pinned"\nalmide = "999.0.0"\n' > "$d/almide.toml"
printf 'effect fn main() -> Unit = {\n  println("pinned")\n}\n' > "$d/src/main.almd"
fail=0

# Every ALMIDE_* the runner may carry is dropped, so only the one under test is inherited.
clean_env() { env $(env | sed -n 's/^\(ALMIDE_[A-Z0-9_]*\)=.*/-u \1/p') "$@"; }

out="$(cd "$d" && clean_env "$bin" check src/main.almd 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then echo "::error::the version pin did not hold with no switch at all:"$'\n'"$out" >&2; fail=1; fi

out="$(cd "$d" && clean_env ALMIDE_SKIP_VERSION_CHECK=1 "$bin" check src/main.almd 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then
  echo "::error::a release binary honoured an inherited ALMIDE_SKIP_VERSION_CHECK with no --allow-switch — the lock is not engaged by default:"$'\n'"$out" >&2; fail=1
elif ! grep -qF "ALMIDE_SKIP_VERSION_CHECK is set but IGNORED" <<< "$out"; then
  echo "::error::the inherited switch was refused without the lock's IGNORED line:"$'\n'"$out" >&2; fail=1
fi

out="$(cd "$d" && clean_env ALMIDE_SKIP_VERSION_CHECK=1 "$bin" --allow-switch ALMIDE_SKIP_VERSION_CHECK check src/main.almd 2>&1)"; rc=$?
if [ "$rc" != 0 ]; then echo "::error::control: --allow-switch did not honour the switch, so the refusal above proves nothing:"$'\n'"$out" >&2; fail=1; fi

[ "$fail" = 0 ] && echo "release-switch-lock OK: $ver ignores an inherited build-changing switch by default and honours it only when --allow-switch names it"
exit "$fail"

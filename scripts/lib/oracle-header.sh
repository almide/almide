# Sourced by scripts/gen-{ast,check,run}-manifest.sh. The oracle every parity
# golden is recorded from is the CLI built from THIS tree (#2183): there is no
# pinned old binary, and scripts/check-parity-goldens.sh regenerates the
# goldens from the same commit's release binary in CI and fails on any diff.
#
# `oracle_header` prints the one `# oracle:` line each manifest starts with —
# the generator's `almide --version` and the git HEAD it ran at. The SHA is
# informational: a rebase changes it, so the gate diffs the ROWS and ignores
# the line (`git diff -I '^# oracle: '`). The VERSION and build kind in it are
# not: every parity test reads them back through
# `almide_corpus::verify_oracle_header` and refuses a manifest recorded by a
# binary that is not `almide <Cargo.toml version> (dev…)` (#2405). Requires
# $ORACLE to be set.
#
# HEAD is read where the script is sourced (the repo root) — the run generator
# later cd's into the judge mount, whose HEAD is a different repository's.
export LC_ALL=C   # the refusal below sorts (#1031); every sourcing script pins it too
ORACLE_HEAD="$(git rev-parse --short=9 HEAD 2>/dev/null || printf 'unknown')"
oracle_header() {
  printf '# oracle: %s at %s — the CLI built from this tree; informational (scripts/check-parity-goldens.sh regenerates the rows and diffs them)\n' \
    "$("$ORACLE" --version | head -1)" "$ORACLE_HEAD"
}

# Prepend the header to a finished (sorted) manifest in place.
stamp_oracle_header() {
  local tmp
  tmp="$(mktemp)"
  { oracle_header; cat "$1"; } > "$tmp" && mv "$tmp" "$1"
}

# The `[package]` version of the root Cargo.toml — the one `almide --version`
# prints. `[workspace.package]` comes first in the file and carries a
# different number, so this reads the line AFTER `[package]`, never the first
# `version =` (the release-seal trap, f84bb6aae).
tree_version() {
  awk '/^\[package\]/{p=1;next} /^\[/{p=0} p && /^version *=/{gsub(/[" ]/,"",$3); print $3; exit}' Cargo.toml
}

# #2405: the committed header is READ before a gate regenerates over it. The
# SHA stays informational (a rebase changes it), but the version and build
# kind must name the CLI built from this tree — the same predicate every
# parity test applies (almide_corpus::verify_oracle_header).
verify_committed_header() {
  local m="$1" want line
  want="$(tree_version)"
  line="$(head -1 "$m" 2>/dev/null)"
  case "$line" in
    "# oracle: almide $want (dev"*) return 0 ;;
  esac
  echo "::error::$m: header \`$line\` does not name the CLI built from this tree (almide $want (dev…)) — the rows were recorded by a released or other-version binary; regenerate them with ORACLE=target/release/almide" >&2
  return 1
}

# #2405: refuse to RECORD goldens from a tree the rows would not reproduce on.
#
# The generators wrote happily from any tree — an ORACLE older than the
# sources, a released binary off PATH, a worktree behind develop, an untracked
# fixture — and the only trace was the `# oracle:` line the gate skips. A row
# recorded that way is a golden from a build that exists on no branch, and the
# gate would compare against it forever. So the failure moves to write time,
# in front of the one person who can fix it cheaply. Nothing is written when
# this refuses: it runs before the generator truncates its outputs.
#
# ALMIDE_MANIFEST_TREE_CHECK selects the mode:
#   strict (default)  every check below
#   gate              the untracked-fixture check only — the caller vouches for
#                     the binary (scripts/check-parity-goldens.sh: CI's artifact
#                     is built from this very commit, and its checkout is
#                     detached, so "behind upstream" has no meaning there)
#   off               the deliberate override; says what it overrode
#
# What identifies the binary, in order of strength (#2384 gives the stamp):
#   1. `almide --version` = `<version> (<kind>[, <sha>])`. The version must be
#      this tree's Cargo.toml version and the kind `dev` — a `(release)`
#      binary came from the release workflow, never from this tree.
#   2. A stamped sha (`make install` passes ALMIDE_BUILD_SHA) must be HEAD.
#      Version alone is not identification: a develop build names the coming
#      release for the whole window between bump and tag.
#   3. An unstamped binary is accepted only from THIS tree's target/, and only
#      when no compiler source is newer than it (mtime: what `cargo build`
#      itself keys on). A checkout gives every file a fresh mtime, so a binary
#      from another worktree cannot be told apart from a stale one — stamp it
#      or build here.
refuse_stale_tree() {
  local mode="${ALMIDE_MANIFEST_TREE_CHECK:-strict}"
  case "$mode" in
    strict|gate) ;;
    off) echo "::warning::ALMIDE_MANIFEST_TREE_CHECK=off — recording goldens without the stale-tree checks (#2405); the # oracle: line is the only trace" >&2; return 0 ;;
    *) echo "::error::ALMIDE_MANIFEST_TREE_CHECK=$mode: expected strict, gate or off" >&2; return 2 ;;
  esac
  local bad=0

  # -- untracked fixtures (both modes) ---------------------------------------
  # `find` sweeps them into the rows; CI regenerates from committed files
  # only, so those rows can never reproduce there — and a fixture forgotten
  # this way was the memory that named the hole. `git add` is the fix.
  local untracked
  untracked="$(git ls-files --others --exclude-standard -- spec 2>/dev/null | grep '\.almd$' || true)"
  if [ -n "$untracked" ]; then
    bad=1
    echo "::error::untracked .almd fixture(s) under spec/ — their rows would be recorded here and never reproduce in CI, which regenerates from committed files only. git add (or remove) them first:" >&2
    printf '%s\n' "$untracked" | sed 's/^/  /' >&2
  fi

  if [ "$mode" = "gate" ]; then
    [ "$bad" -eq 0 ] || { echo "::error::refusing to record goldens from this tree (#2405)" >&2; return 1; }
    return 0
  fi

  # -- the binary --------------------------------------------------------------
  local vline ver kind sha want head
  vline="$("$ORACLE" --version 2>/dev/null | head -1)"
  ver="$(printf '%s' "$vline" | awk '{print $2}')"
  kind="$(printf '%s' "$vline" | sed -n 's/.*(\([a-z]*\).*/\1/p')"
  sha="$(printf '%s' "$vline" | sed -n 's/.*(dev, \([0-9a-f]*\)).*/\1/p')"
  want="$(tree_version)"
  head="$(git rev-parse --short=9 HEAD 2>/dev/null || printf 'unknown')"
  if [ "$ver" != "$want" ]; then
    bad=1
    echo "::error::ORACLE is \`$vline\` but this tree is version $want (Cargo.toml) — a released or other-tree binary, not the CLI built from this tree. Build here (make install) and point ORACLE at target/release/almide: $ORACLE" >&2
  elif [ "$kind" != "dev" ]; then
    bad=1
    echo "::error::ORACLE is \`$vline\` — a release binary comes from the release workflow, never from this tree. The oracle is the CLI built from this tree (make install → target/release/almide): $ORACLE" >&2
  elif [ -n "$sha" ]; then
    if [ "$sha" != "$head" ]; then
      bad=1
      echo "::error::ORACLE was built at $sha (\`$vline\`) but this tree is at $head — rebuild (make install) before recording goldens: $ORACLE" >&2
    fi
  else
    # Unstamped: mtime, and only for a binary built in this tree. Both sides
    # physical (`pwd -P`): a symlinked tmp or home would otherwise never match.
    local oracle_phys
    oracle_phys="$(cd "$(dirname "$ORACLE")" 2>/dev/null && pwd -P)/$(basename "$ORACLE")"
    case "$oracle_phys" in
      "$(pwd -P)"/target/*)
        local newer
        newer="$(find crates src stdlib runtime build.rs Cargo.toml Cargo.lock -type f -newer "$ORACLE" 2>/dev/null \
          | grep -v -e '/tests/' -e '/target/' -e '/benches/' -e '/golden/' -e '\.md$' | sort | head -20)"
        if [ -n "$newer" ]; then
          bad=1
          echo "::error::ORACLE predates the sources: $(printf '%s\n' "$newer" | wc -l | tr -d ' ')+ compiler file(s) are newer than $ORACLE (built $(date -r "$ORACLE" '+%Y-%m-%d %H:%M:%S')). Rebuild before recording goldens (make install stamps the commit into --version, which is the stronger check). Newest first:" >&2
          find crates src stdlib runtime build.rs Cargo.toml Cargo.lock -type f -newer "$ORACLE" 2>/dev/null \
            | grep -v -e '/tests/' -e '/target/' -e '/benches/' -e '/golden/' -e '\.md$' \
            | xargs ls -t 2>/dev/null | head -5 | sed 's/^/  /' >&2
        fi ;;
      *)
        bad=1
        echo "::error::ORACLE carries no build sha (\`$vline\`) and was not built in this tree, so nothing ties it to these sources: $ORACLE. Build it here (make install stamps HEAD into --version; a plain cargo build --release under \$PWD/target is checked by mtime)" >&2 ;;
    esac
  fi

  # -- the tree ----------------------------------------------------------------
  # Behind the tracked upstream (or origin/develop when the branch has none —
  # a worktree cut for one fix rarely has one, and that is exactly the case
  # the issue met: 22 commits behind, two of them in the emitter). Local refs
  # only: a `git fetch` makes the answer current. The message names the
  # intervening commits that touch compiler sources, which is what separates
  # urgent from pedantic.
  local ref behind touching
  ref="$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || true)"
  [ -n "$ref" ] || { git rev-parse --verify --quiet origin/develop >/dev/null 2>&1 && ref="origin/develop"; }
  if [ -n "$ref" ]; then
    behind="$(git rev-list --count "HEAD..$ref" 2>/dev/null || printf 0)"
    if [ "${behind:-0}" -gt 0 ]; then
      bad=1
      touching="$(git log --format='  %h %s' "HEAD..$ref" -- crates src stdlib runtime build.rs Cargo.toml Cargo.lock 2>/dev/null | grep -v -e '/tests/' || true)"
      echo "::error::this worktree is $behind commit(s) behind $ref (local ref — git fetch to be sure). Goldens recorded here would come from a build that exists on no branch. Rebase (or merge) first, rebuild, then regenerate." >&2
      if [ -n "$touching" ]; then
        echo "  $(printf '%s\n' "$touching" | wc -l | tr -d ' ') of them touch compiler sources:" >&2
        printf '%s\n' "$touching" | head -20 >&2
      else
        echo "  none of them touch compiler sources (the rows may well be identical; the tree still is not the one CI will judge)" >&2
      fi
    fi
  fi

  if [ "$bad" -ne 0 ]; then
    echo "::error::refusing to record goldens from this tree (#2405); nothing was written. ALMIDE_MANIFEST_TREE_CHECK=off overrides, and leaves only the # oracle: line as the trace." >&2
    return 1
  fi
  return 0
}

#!/usr/bin/env bash
# Workflow-file parse gate (#1234): a schedule-only workflow that cannot parse
# fails SILENTLY — Actions never validates workflow files until a trigger
# fires, so a broken file ships through green CI and the scheduled instrument
# goes dark (the 0.57.0 release-gate incident: a duplicate `if:` key killed
# fuzz-nightly's parse; only a manual dispatch surfaced it, #1225).
#
# STRICT parse: `yaml.safe_load` alone accepts the exact duplicate-key case
# that bit, so the loader here rejects duplicate mapping keys explicitly.
# The 計器の計器 principle: every gate that only runs on a schedule needs a
# parse/health check that runs on every PR.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$ROOT" <<'PYEOF'
import glob
import sys

import yaml


class StrictLoader(yaml.SafeLoader):
    pass


def no_duplicate_keys(loader, node, deep=False):
    mapping = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise yaml.constructor.ConstructorError(
                None,
                None,
                f"duplicate mapping key {key!r}",
                key_node.start_mark,
            )
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


StrictLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, no_duplicate_keys
)

root = sys.argv[1]
files = sorted(glob.glob(f"{root}/.github/workflows/*.yml")) + sorted(
    glob.glob(f"{root}/.github/workflows/*.yaml")
)
if not files:
    print("WORKFLOW PARSE FAIL — no workflow files found (the glob moved?)", file=sys.stderr)
    sys.exit(1)

fail = 0
for f in files:
    try:
        with open(f) as fh:
            doc = yaml.load(fh, Loader=StrictLoader)
    except yaml.YAMLError as e:
        print(f"  {f}: {e}", file=sys.stderr)
        fail = 1
        continue
    # An Actions workflow must at least be a mapping with jobs; a file that
    # parses to a scalar/list would also die at trigger time.
    if not isinstance(doc, dict) or "jobs" not in doc:
        print(f"  {f}: parses but has no `jobs` mapping — not a valid workflow", file=sys.stderr)
        fail = 1

if fail:
    print("WORKFLOW PARSE FAIL — a broken workflow file is invisible until its trigger fires.", file=sys.stderr)
    sys.exit(1)
print(f"workflows OK: {len(files)} file(s) parse strictly (duplicate keys rejected)")
PYEOF

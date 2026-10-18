#!/usr/bin/env bash
# research/benchmark/perf/gce_ab.sh <ref-a> <ref-b> <outdir> [benches] [rounds]
# A/B of the native perf rows on ONE single-use VM: builds almide at both refs,
# then runs bench.py (full workloads, native + rust legs) alternately A, B, A, B
# for <rounds> rounds over the SAME bench sources (taken from ref A), so drift
# on the VM lands on both sides. Results JSON per round come back to <outdir>.
#   env: MACHINE (c4-standard-4) ZONE (asia-northeast1-b) PROJECT (almide-perf) RUNS (5)
set -uo pipefail
A_REF="${1:?ref a}"; B_REF="${2:?ref b}"; OUT="${3:?out dir}"
BENCHES="${4:-strchurn,fasta,decode}"; ROUNDS="${5:-3}"
MACHINE="${MACHINE:-c4-standard-4}"; ZONE="${ZONE:-asia-northeast1-b}"; PROJECT="${PROJECT:-almide-perf}"; RUNS="${RUNS:-5}"
GCLOUD="${GCLOUD:-$(command -v gcloud)}"
GACCOUNT="${GACCOUNT:-almide-insp-runner@almide-perf.iam.gserviceaccount.com}"
g() { "$GCLOUD" --project "$PROJECT" --account "$GACCOUNT" --quiet "$@"; }
mkdir -p "$OUT"
full() { gh api "repos/almide/almide/commits/$1" -q .sha 2>/dev/null; }
A_REF=$(full "$A_REF") && [ ${#A_REF} = 40 ] || { echo "cannot resolve ref a"; exit 2; }
B_REF=$(full "$B_REF") && [ ${#B_REF} = 40 ] || { echo "cannot resolve ref b"; exit 2; }
name="almide-perf-$(date +%Y%m%d-%H%M%S)-$$"
trap 'g compute instances delete "'"$name"'" --zone "'"$ZONE"'" >/dev/null 2>&1; echo "deleted '"$name"'"' EXIT
g compute instances create "$name" --zone "$ZONE" --machine-type "$MACHINE" \
  --provisioning-model=SPOT --instance-termination-action=DELETE --max-run-duration=6h \
  --image-family ubuntu-2404-lts-amd64 --image-project ubuntu-os-cloud \
  --boot-disk-size=60GB --boot-disk-type=hyperdisk-balanced >/dev/null || { echo "create failed"; exit 1; }
for _ in $(seq 1 30); do g compute ssh "$name" --zone "$ZONE" --command true >/dev/null 2>&1 && break; sleep 10; done
g compute ssh "$name" --zone "$ZONE" --command "cat > run.sh" <<EOF
set -uo pipefail
exec > run.log 2>&1
sudo apt-get update -q >/dev/null && sudo apt-get install -y -q build-essential git curl python3 >/dev/null
curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal >/dev/null
source \$HOME/.cargo/env
fetch() { git init -q "\$1" && git -C "\$1" remote add origin https://github.com/almide/almide.git && git -C "\$1" fetch -q --depth 1 origin "\$2" && git -C "\$1" checkout -q FETCH_HEAD; }
fetch a "$A_REF" || { echo "FATAL: fetch a"; touch \$HOME/done; exit 1; }
fetch b "$B_REF" || { echo "FATAL: fetch b"; touch \$HOME/done; exit 1; }
echo "== env a=$A_REF b=$B_REF machine=$MACHINE zone=$ZONE runs=$RUNS rounds=$ROUNDS benches=$BENCHES"
lscpu | grep -E 'Model name|^CPU\(s\)'; rustc --version; uname -sr
(cd a && cargo build --release -q 2>&1 | tail -3); (cd b && cargo build --release -q 2>&1 | tail -3)
[ -x a/target/release/almide ] && [ -x b/target/release/almide ] || { echo "FATAL: build produced no binary"; touch \$HOME/done; exit 1; }
echo "== a \$(a/target/release/almide --version) / b \$(b/target/release/almide --version)"
mkdir -p \$HOME/res
for r in \$(seq 1 $ROUNDS); do
  for side in a b; do
    echo "=== round \$r side \$side \$(date +%H:%M:%S)"
    (cd a && ALMIDE_BIN=\$HOME/\$side/target/release/almide python3 research/benchmark/perf/bench.py --runs $RUNS --legs native,rust --bench $BENCHES --label \$side-\$r --out \$HOME/res/\$side-\$r.json 2>&1 | tail -$((4 + 2))) || echo "FATAL: bench \$side \$r"
  done
done
echo "== done \$(date +%H:%M:%S)"
touch \$HOME/done
EOF
g compute ssh "$name" --zone "$ZONE" --command "nohup bash run.sh >/dev/null 2>&1 &" >/dev/null
echo "running on $name"
while :; do
  sleep 120
  if g compute ssh "$name" --zone "$ZONE" --command "test -f done" >/dev/null 2>&1; then break; fi
  g compute instances describe "$name" --zone "$ZONE" --format='value(status)' 2>/dev/null | grep -q RUNNING || { echo "VM gone (spot preemption?)"; break; }
done
g compute scp "$name:run.log" "$OUT/run.log" --zone "$ZONE" >/dev/null && echo "log copied"
g compute ssh "$name" --zone "$ZONE" --command "tar -C \$HOME -czf res.tgz res" >/dev/null && g compute scp "$name:res.tgz" "$OUT/res.tgz" --zone "$ZONE" >/dev/null && tar -C "$OUT" -xzf "$OUT/res.tgz" && echo "results copied"

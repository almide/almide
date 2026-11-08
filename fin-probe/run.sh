#!/usr/bin/env bash
# Item-3 probe: which allocations of the strs oracle program outlive __almide_main?
set -x
OUT=/tmp/out/p3; mkdir -p $OUT
ALM=$PWD/target/release/almide
# the generated programs: a copy of the oracle test without its inner doc comments, plus a dumper
grep -v '^//!' tests/native_borrow_oracle_test.rs > tests/probe_oracle_dump.rs
cat >> tests/probe_oracle_dump.rs <<'RS'
#[test]
fn probe_dump_programs() {
    for t in TYPES { std::fs::write(format!("/tmp/out/p3/oracle_{}.almd", t.tag), program(t)).unwrap(); }
    std::fs::write("/tmp/out/p3/baseline.almd", "fn main() -> Unit = println(\"baseline\")\n").unwrap();
}
RS
cargo test --release --test probe_oracle_dump probe_dump_programs 2>&1 | tail -3
export ALMIDE_NO_VERIFIED_OK=1
for leg in v1 v0; do
  flag=""; [ $leg = v0 ] && flag="--no-verified"
  for prog in baseline oracle_strs oracle_str oracle_list; do
    touch /tmp/marker; sleep 1
    ALMIDE_ALLOC_COUNT=1 $ALM run $flag $OUT/$prog.almd > $OUT/$leg-$prog.stdout 2> $OUT/$leg-$prog.stderr
    grep __ALMD_ALLOC $OUT/$leg-$prog.stderr | sed "s/^/$leg $prog: /" | tee -a $OUT/counts.txt
    main=$(find / -xdev -name main.rs -path '*/src/main.rs' -newer /tmp/marker 2>/dev/null | head -1)
    echo "$leg $prog main.rs: $main" | tee -a $OUT/counts.txt
    [ -n "$main" ] || continue
    proj=$(dirname $(dirname $main)); work=/tmp/pb-$leg-$prog; rm -rf $work; mkdir -p $work
    cp $proj/Cargo.toml $work/; mkdir -p $work/src; cp -r $proj/src/. $work/src/
    [ -d $proj/native ] && cp -r $proj/native $work/ || true
    cp $work/src/main.rs $OUT/$leg-$prog.rs
    python3 fin-probe/patch_alloc.py $work/src/main.rs || { echo "patch failed $leg $prog"; continue; }
    (cd $work && CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none cargo build --release -q 2>&1 | tail -20)
    bin=$(find $work/target/release -maxdepth 1 -type f -perm -u+x | head -1)
    $bin > /dev/null 2> $OUT/$leg-$prog.live.txt
    grep -E '__ALMD_ALLOC|__PB_LIVE count' $OUT/$leg-$prog.live.txt | sed "s/^/$leg $prog traced: /" | tee -a $OUT/counts.txt
  done
done

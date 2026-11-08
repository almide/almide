set +e
python3 research/benchmark/perf/bench.py --quick --runs 5 --legs native,wasm --bench mutread-arg,mutread-var,mutread-let --label t --out /tmp/mr.json 2>&1 | grep "mutread-.*(.:"

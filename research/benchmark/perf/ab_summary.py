"""Summarise a gce_ab.sh run: per bench, the median native time and the median
almide/rust ratio of each side over its rounds, B against A, and the spread of
the ratio over all rounds (the run's own noise).

usage: python3 -I research/benchmark/perf/ab_summary.py <outdir>/res
"""
import json
import os
import statistics
import sys

REFS = {"fasta": "rust:fasta", "strchurn": "rust:strchurn", "decode": "rust:decode"}


def main(d):
    rounds = sorted({f.split("-")[1].split(".")[0] for f in os.listdir(d) if f.endswith(".json")})
    for bench, ref in REFS.items():
        nat = {"a": [], "b": []}
        rat = {"a": [], "b": []}
        for side in "ab":
            for r in rounds:
                path = os.path.join(d, f"{side}-{r}.json")
                variants = json.load(open(path))["results"].get(bench, {}).get("variants")
                if not variants:
                    continue
                n = variants[f"{bench}/native"]["median"]
                nat[side].append(n)
                rat[side].append(n / variants[f"{bench}/{ref}"]["median"])
        if not nat["a"] or not nat["b"]:
            print(f"{bench}: not measured")
            continue
        ma, mb = statistics.median(nat["a"]), statistics.median(nat["b"])
        ra, rb = statistics.median(rat["a"]), statistics.median(rat["b"])
        every = rat["a"] + rat["b"]
        print(
            f"{bench:9s} native a={ma:.4f}s b={mb:.4f}s ({mb / ma - 1:+.2%}) "
            f"ratio a={ra:.3f} b={rb:.3f} ({rb / ra - 1:+.2%}) spread {max(every) / min(every) - 1:.2%}"
        )


if __name__ == "__main__":
    main(sys.argv[1])

#!/usr/bin/env bash
# Find which renderable programs cover the shapes gate's unexercised regions.
set -uo pipefail
export LC_ALL=C
OUT=probe-out; mkdir -p $OUT
W="$(mktemp -d)"
SYSROOT="$(rustc --print sysroot)"
LLVM_BIN="$(echo "$SYSROOT"/lib/rustlib/*/bin | awk '{print $1}')"
TARGET=target/shapes-cov
RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="$W/build-%m-%p.profraw" \
  cargo build -q --release -p almide-mir --example render_corpus --target-dir "$TARGET" || exit 2
BIN="$TARGET/release/examples/render_corpus"
SRCS=$(ls crates/almide-mir/src/render_native*.rs)
{ grep -E '^spec/' proofs/native-verified-floor.txt; ls research/benchmark/perf/v1-shapes/*.almd; } > "$W/corpus.txt"
mkdir -p "$W/gate"
LLVM_PROFILE_FILE="$W/gate/run-%m.profraw" "$BIN" < "$W/corpus.txt" > "$OUT/gate-verdicts.txt"
"$LLVM_BIN/llvm-profdata" merge -sparse "$W"/gate/*.profraw -o "$W/gate.profdata"
"$LLVM_BIN/llvm-cov" export "$BIN" -instr-profile="$W/gate.profdata" -skip-expansions $SRCS > "$W/gate.json"
# Candidates: every other .almd that renders.
find spec examples research stdlib tests -name '*.almd' 2>/dev/null | sort | grep -vxFf "$W/corpus.txt" > "$W/cands.txt"
mkdir -p "$W/c"
awk '{print NR, $0}' "$W/cands.txt" > "$W/index.txt"
run_one() {
  i=$1; p=$2
  d="$W/c/$i"; mkdir -p $d
  echo "$p" | LLVM_PROFILE_FILE="$d/r-%m.profraw" timeout 60 "$BIN" --reasons > "$d/v.txt" 2>/dev/null
  [ -s "$d/v.txt" ] || printf '%s\tPANIC\n' "$p" > "$d/v.txt"
  "$LLVM_BIN/llvm-profdata" merge -sparse "$d"/*.profraw -o "$d/p.profdata" 2>/dev/null
  "$LLVM_BIN/llvm-cov" export "$BIN" -instr-profile="$d/p.profdata" -skip-expansions $SRCS 2>/dev/null \
    | python3 -c '
import json,sys
d=json.load(sys.stdin)["data"][0]
hit=set()
for f in d["functions"]:
  fs=f["filenames"]
  for r in f["regions"]:
    if r[7]==0 and r[4]>0 and "/render_native" in fs[r[5]]: hit.add("%s:%d:%d:%d:%d"%(fs[r[5]].split("/src/")[-1],r[0],r[1],r[2],r[3]))
print("\n".join(sorted(hit)))' > "$d/hit.txt"
  rm -f "$d"/*.profraw "$d/p.profdata"
}
export -f run_one; export W BIN LLVM_BIN SRCS
awk '{print $1, $2}' "$W/index.txt" | xargs -P "$(nproc)" -n 2 bash -c 'run_one "$0" "$1"'
tar czf "$OUT/hits.tgz" -C "$W" c index.txt gate.json
python3 - "$W" "$OUT" "$(git rev-parse --show-toplevel)" <<'PY'
import json,sys,os,re,collections
W,OUT,root=sys.argv[1],sys.argv[2],sys.argv[3]+"/"
d=json.load(open(W+"/gate.json"))["data"][0]
texts={}
def text_of(path,l0,c0,l1,c1):
    if path not in texts: texts[path]=open(path).read().split("\n")
    ls=texts[path][l0-1:l1]
    if not ls: return ""
    if len(ls)==1: return ls[0][c0-1:c1-1]
    return "\n".join([ls[0][c0-1:]]+ls[1:-1]+[ls[-1][:c1-1]])
regs=[]
for f in d["functions"]:
    for r in f["regions"]:
        if r[7]!=0: continue
        path=f["filenames"][r[5]]
        if not re.search(r"crates/almide-mir/src/render_native[a-z_]*\.rs$", path): continue
        regs.append((path,(r[0],r[1]),(r[2],r[3]),r[4],f["name"]))
by=collections.defaultdict(list)
for g in regs: by[g[0]].append(g)
def own(g):
    path,a,b=g[0],g[1],g[2]
    lines=[list(l) for l in text_of(path,a[0],a[1],b[0],b[1]).split("\n")]
    for o in by[path]:
        if o is g or not (a<=o[1] and o[2]<=b) or (o[1],o[2])==(a,b): continue
        for ln in range(o[1][0],o[2][0]+1):
            row=lines[ln-a[0]]
            lo=(o[1][1]-(a[1] if ln==a[0] else 1)) if ln==o[1][0] else 0
            hi=(o[2][1]-(a[1] if ln==a[0] else 1)) if ln==o[2][0] else len(row)
            for k in range(max(lo,0),min(hi,len(row))): row[k]=" "
    return "\n".join("".join(l) for l in lines)
targets={}
for g in regs:
    if g[3]!=0: continue
    t=own(g)
    if "wall(" in t or not re.search(r"[A-Za-z_]",t): continue
    key="%s:%d:%d:%d:%d"%(g[0].split("/src/")[-1],g[1][0],g[1][1],g[2][0],g[2][1])
    targets[key]=t.strip().replace("\n"," ")[:120]
# per-candidate hits
idx={}
for l in open(W+"/index.txt"):
    i,p=l.split()[:2]
    vp=W+"/c/"+i+"/v.txt"
    v=open(vp).read().split("\t")[1].strip() if os.path.exists(vp) else "NONE"
    idx[i]=(p,v)
with open(OUT+"/cand-verdicts.txt","w") as o:
    for i,(p,v) in sorted(idx.items(),key=lambda x:int(x[0])): o.write(f"{p}\t{v}\n")
cover=collections.defaultdict(list)
progs={}
for i,(p,v) in idx.items():
    hp=W+"/c/"+i+"/hit.txt"
    if not os.path.exists(hp): continue
    h=set(open(hp).read().split())&set(targets)
    if h: progs[p]=(v,h)
    for k in h: cover[k].append((v,p))
with open(OUT+"/targets.txt","w") as o:
    for k in sorted(targets):
        cs=cover.get(k,[])
        ok=[p for v,p in cs if v=="OK"]
        o.write(f"{k}\tOKcov={len(ok)}\tanycov={len(cs)}\t{targets[k]}\t{' '.join(ok[:6])}\n")
# greedy cover with OK programs
left=set(targets); pick=[]
okp={p:h for p,(v,h) in progs.items() if v=="OK"}
while True:
    best=max(okp,key=lambda p:len(okp[p]&left),default=None)
    if not best or not (okp[best]&left): break
    pick.append((best,len(okp[best]&left))); left-=okp[best]
with open(OUT+"/greedy.txt","w") as o:
    for p,n in pick: o.write(f"{n}\t{p}\n")
    o.write(f"# uncovered by any OK candidate: {len(left)} of {len(targets)}\n")
print(f"targets={len(targets)} coverable_by_OK={len(targets)-len(left)}")
PY

import sys, re, os, subprocess
src = open(sys.argv[1]).read()
head, body = src.split("fn main() -> Unit = {\n", 1)
blocks = re.findall(r"^  \{\n.*?^  \}\n", body, re.S | re.M)
os.makedirs("/tmp/split", exist_ok=True)
for k, b in enumerate(blocks):
    p = f"/tmp/split/b{k}.almd"
    open(p, "w").write(head + "fn main() -> Unit = {\n" + b + "}\n")
    r = subprocess.run(["almide", "run", p], env=dict(os.environ, ALMIDE_ALLOC_COUNT="1"), capture_output=True, text=True)
    line = [l for l in r.stderr.splitlines() if l.startswith("__ALMD_ALLOC")]
    m = re.search(r"allocs=(\d+) deallocs=(\d+)", line[0]) if line else None
    if not m or m.group(1) != m.group(2):
        print("LEAK", k, line, b.strip().splitlines()[:8])
print("blocks", len(blocks))

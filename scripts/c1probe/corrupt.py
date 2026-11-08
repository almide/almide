# PROBE ONLY: drop one release ('d') from the `main` ownership witness of a bundle.
import sys
src, dst = sys.argv[1:3]
b = open(src, "rb").read()
i = b.index(b"\n") + 1
out = [b[:i]]
done = False
while i < len(b):
    j = b.index(b"\n", i)
    line = b[i:j].decode()
    i = j + 1
    if line.startswith("witness "):
        _, prop, n, fn = line.split(" ", 3)
        n = int(n)
        w = b[i:i + n].decode()
        i += n + 1
        if not done and prop == "ownership" and "d" in w:
            k = w.rindex("d")
            w = w[:k] + w[k + 1:]
            done = True
            print(f"corrupted ownership witness of {fn}: dropped the release at {k}")
        out.append(f"witness {prop} {len(w.encode())} {fn}\n".encode() + w.encode() + b"\n")
    else:
        out.append((line + "\n").encode())
assert done, "no ownership witness with a release"
open(dst, "wb").write(b"".join(out))

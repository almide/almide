#!/usr/bin/env python3
"""The artifact-size probes behind docs/benchmarks/float-printer.md. Writes them next to this
file; build each with `almide build <probe>.almd --target wasm -o /tmp/x.wasm` (the build line
prints the byte count).

  hello.almd      the 1,337 B baseline
  one_float.almd  `println("${1.5}")` — the float printer's whole cost over hello
  int_only.almd   int.to_string alone (what a List[Int] probe pays before its literal)
  list200.almd    a 200-element List[Int] literal: ~17 B per element on the structural leg
  str0.almd / str1600.almd   a 1 B vs 1,600 B string literal: 1 B per byte of data segment
"""
import os
d = os.path.dirname(os.path.abspath(__file__))
def w(name, s): open(os.path.join(d, name), "w").write(s)
w("hello.almd", 'fn main() -> Unit = { println("Hello, World!") }\n')
w("one_float.almd", 'fn main() -> Unit = { println("${1.5}") }\n')
w("int_only.almd", 'fn main() -> Unit = { println(int.to_string(7)) }\n')
vals = ", ".join(str(0x4000000000000000 + i * 0x123456789ABCD) for i in range(200))
w("list200.almd", "fn main() -> Unit = {\n  let t: List[Int] = [" + vals + "]\n  println(int.to_string(list.get(t, 7) ?? 0))\n}\n")
alpha = [c for c in map(chr, range(0x30, 0x7A)) if c not in '"\\']
s = "".join(alpha[i % len(alpha)] for i in range(1600))
w("str1600.almd", 'fn main() -> Unit = {\n  let t = "' + s + '"\n  println(int.to_string(string.len(t)))\n}\n')
w("str0.almd", 'fn main() -> Unit = {\n  let t = "x"\n  println(int.to_string(string.len(t)))\n}\n')

# Almide v1 — the trust layer for machine-written code

AI writes the code now. The question is no longer *"can it write it?"* — frontier models
write plausible code all day. The question is **"can you trust what it wrote?"**

Almide v1 answers it: **when the AI is wrong, you find out clearly and can fix it; when
it's right, it's proven safe.**

## See it — one command

```
cd demo/make-verify && ./run.sh
```

Three realistic AI modification mistakes (a forgotten match case, a missed call site, an
unhandled `None`). Almide **catches each at compile** with an actionable diagnostic that
hands you the fix. Mainstream languages pass their compile gate and **ship the bug** —
one as a silently wrong value, two as deferred runtime crashes.

## The number (unbiased, model-agnostic)

We injected **8 realistic AI mistakes**, each authored by a *separate* process in both
Almide and Python, and asked the *language* — not the model — whether the failure is
caught-and-recoverable or silent:

- **Almide: 6 / 8 caught at compile**, each with an actionable diagnostic.
- **Python's compile gate (`py_compile`): 0 / 8.** Every mistake shipped.

The differentiator is a property of the **language + compiler + a per-build, Coq-proven
checker** — it does not evaporate as models get stronger (a strong model makes fewer
mistakes; *when it does*, only the language decides whether you see them).

## Honest scope — what's real today, what's maturing

- **Real today:** the trust experience above, and a per-build **proof spine** that
  re-verifies ownership / name-resolution / capability bounds and emits a certificate —
  Almide aims to ship a *proof*, not just an artifact.
- **Runtime today:** the shipping Almide compiler runs your programs now.
- **Maturing (in the open):** the **verified-wasm execution path** that renders your code
  to wasm *byte-for-byte identical to the reference*, carrying the proof. It is real,
  gated in CI, and **not yet at full coverage** — the last mile (byte-level binding) is
  the hard, deliberate, soundness-critical work, and we will not rush it.

We say this plainly because the whole point is trust: we won't claim "v1 runs everything"
before it does.

## The direction — the destination

**Full parity: every program and every library that runs today runs on the verified path
— with proof.** When that lands, *unverified* machine-written code becomes the exception,
and the old unverified path becomes obsolete as a byproduct. That is the north-star, and
the verified-execution work is the climb toward it.

## Who it's for

Teams building with AI-written code — agent platforms, and anyone shipping software a
machine wrote — who need the modification to **fail loudly and fixably when it's wrong**,
and to be **provably safe when it's right**, independent of which model wrote it.

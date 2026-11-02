# Decoder scratch ownership (#2046)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](wasm-decoder-reclaim-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-decoder-reclaim-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-decoder-reclaim-review.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original remains authoritative and is not replaced as a machine input or executable gate. No automatic synchronization. Recorded test results, counts, implementation status and proof claims are source-scoped, not independently rerun or established as current guarantees here. The bounded measured cases and retained refusal set do not prove absence of every leak or universal memory safety. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm-decoder-reclaim-review.md)

The structural WASM decoder leaked temporary Result wrappers extracted by
`Try`/`Unwrap`. A two-field record decode retained 128 bytes per call: the
heap watermark grew from 196544 bytes at 1000 calls to 1092544 at 8000.

The reference compiler is Koka, cloned in `../almide-references/koka` at
`3ac4f001ab7277b484d661fdbada1aaf8d01ecbf`. In
`src/Backend/C/Parc.hs`, `parcBorrowArg` names a produced borrowed argument
and arranges its drop after consumption, preserving evaluation order.
Almide already has the corresponding temporary-binding pass. Extraction
now participates in that pass: the wrapper gets an ordinary local owner,
and the existing exit plan releases it on success or propagation. The
payload remains a view; retaining or returning it acquires a separate
credit through the existing routes.

Tail extraction keeps the existing carrier-transfer route, so naming a
temporary cannot hide a tail call. Raw payload annotations introduced by
move-mode effect lowering are left to that ABI's extraction route. The
regressions also check 200000 tail calls and mutable-argument writeback.
Callback tails and their `ok(unwrap(call))` carrier wrappers likewise keep
their shape, preserving fan's prefetch and first-error forwarding routes.

Error-path measurements exposed two additional leaks. A match returning
a borrowed success value beside a newly constructed error gave the fresh
error an unnecessary reference at the function return. Value match arms
now each deliver one owned credit to the join; borrowed arms acquire it,
fresh arms transfer it. Missing-field error construction also left the
intermediate concatenated prefix allocated. It now releases that string
after the final concatenation copies it.

The regression compares the actual exported heap watermark at 1000 and
8000 calls, checks output, and covers flat records, nested record lists,
type errors after a successfully decoded field, and missing fields.
All four cases stay flat. The existing 22 call-result ownership tests and
exit validation also pass. Codopsy reports 91/A for the WASM source
directory with the existing complexity thresholds (20/30).

The allocation ledger regeneration reduces 79 corpus watermarks, increases
none, and preserves the existing structural refusal set. Native-result
ownership and the structural witness floor both pass without relaxing
their baselines. The original 100000-call reproducer completes with the
same 4 MiB heap cap that previously exhausted memory.

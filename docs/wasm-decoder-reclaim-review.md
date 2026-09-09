# Decoder scratch ownership (#2046)

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

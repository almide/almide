# Native projection ownership (#2067, #2069, #2070)

Reading `o.hit` through a match cloned the whole Option payload, and
reading `list.get(xs, i)` or `xs[i].pos` copied an entire record. Record
rebuilding also cloned fields even when the input was owned and dead.

The reference compiler is Rust, cloned in `../almide-references/rust` at
`0d31508599a7814a7044e9a7a871e3dc5f037753`. Its
`compiler/rustc_mir_build/src/builder/expr/as_operand.rs` distinguishes
places and rvalues and carries the operand's temporary lifetime; moves
operate on a Place, including projections. This review keeps those two
questions separate: whether a place can be borrowed for the consumer's
duration, and whether its owner can transfer the projected fields.

Read-only Option/Result matches borrow stable field subjects or use a
reference-returning list lookup. Pattern bindings must only be read;
captures, mutations, and arm uses of the original source decline the
optimization. Borrowed pattern bindings are removed from the owned set.
An adjacent single-use alias before a terminal match folds back to the
original projection before liveness counting. A mutation or other
statement between binding and match prevents that fold.

Member access through a stable indexed list borrows the indexed record.
Both bounds and negative-index behavior remain unchanged. Calls producing
temporary lists retain the original owned indexing macro: moving the
clone outside that macro would outlive its temporary input. String
matching retains the existing MatchSubject coercion.

An owned final field read moves. Rebuilding a record may move several
distinct fields when every remaining root use is one of those direct
projections. Repeated fields, later whole-record uses, loop iterations,
and reusable captures keep the necessary copies. Borrow inference marks
heap-field rebuild inputs as consuming, and caller liveness preserves
the original when it is reused. TCO-managed parameters remain under
TCO's own move plan.

The existing stream-fusion PR removes the Rc callback from the reported
pure list.map shape. This change removes its per-element input-field
clone. Each output record still needs its own captured String value;
that per-output clone is required by the current owned String layout.

Regression coverage checks emitted Rust and executes both native and
WASM: direct/aliased matches, indexed reads, missing/negative optional
indices, owned reconstruction, repeated fields, source reuse, source
mutation, captured records, temporary list inputs, and string matches.

Validation: all 439 native test files pass, as do all 439 default-target
files (427 WASM and 12 native fallback). The 43 nanopass tests, 13 codegen
snapshots, 11 loop-ownership tests, projection regressions and borrow/
capture regressions pass. Codegen remains 90/A under the existing
codopsy thresholds; the commissioned WASM crate also remains 90/A.

The CI cross-target fixture `mg_rebuild_two_phase` exposed a further lifetime
boundary: a module-global `Var` renders a cloned snapshot, not a stable Rust
place. Shared captured cells have the same boundary. Indexed borrows retain the
owned-index path for those roots; ordinary local/parameter projections keep the
reference path. Rust's pinned `as_operand` implementation documents that an
operand is only valid through its temporary scope. The regression exercises both
call-argument and scalar-field reads across two global-list rebuilds.

<!-- description: Proposed where-clause syntax and type-definition constraints beyond existing inline generic bounds -->
# Type Where Constraints

> **Status: Design; standalone `where` syntax is proposed, not current syntax.**

## Already supported

Generic functions already express protocol and structural bounds inline:

```almide
fn display[T: Showable](item: T) -> String = item.show()
fn describe[T: { name: String, .. }](item: T) -> String = item.name
```

`Showable` in the first fragment is a user-declared protocol. Multiple protocol
bounds use `+`. The current rules and complete examples live in
[type-system.md](../../specs/type-system.md#protocol-bounds-on-generics), with
coverage in [protocol_generics_test.almd](../../../spec/lang/protocol_generics_test.almd)
and [open_record_test.almd](../../../spec/lang/open_record_test.almd).
It is therefore incorrect to describe generic constraints as wholly absent.

## Remaining proposal

Evaluate separate `where` clauses on function and type definitions, including
how constraints on a generic type definition are checked at its uses. The
syntax below is a design sketch, not a compilable example or a release promise:

```almide
type Set[T]
  where T: Hash + Eq
= { items: Map[T, Unit] }

fn max[T](items: List[T]) -> Option[T]
  where T: Ord
= list.fold(items, none, (best, x) => ...)
```

The work is to specify the relationship to existing inline bounds, implement
parser/checker support for any adopted extension, and add acceptance and
rejection tests. It is not to reimplement the inline function-generic bounds
that already ship. No release target is assigned here.

A test-context `where` form was also considered in the original design. Sharing
a keyword does not establish that either proposal is implemented; keep that
separate from the current generic-bound syntax.

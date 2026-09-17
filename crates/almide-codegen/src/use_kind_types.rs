//! The vocabulary of the use walk (`use_kind.rs`): what a callee does with a
//! slot ([`SlotMode`]), the position an occurrence sits in ([`Site`] and its
//! constructor kinds), a place-projection chain, and the [`Use`] record one
//! occurrence produces. Split from the walk so each file stays readable on
//! its own; the walk re-exports everything here.

use almide_ir::*;

/// What the callee does with one argument slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotMode {
    /// The slot is `&T` / `&[T]` / `&str`.
    Borrow,
    /// The slot is `&mut T`.
    Mut,
    /// The slot takes the value, or nothing is known about the callee.
    Consume,
}

/// Which constructor an operand feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ctor {
    /// A record literal field.
    Record,
    /// The base of a record update (`{ ...base, f: v }`).
    SpreadBase,
    /// An overriding field of a record update.
    SpreadField,
    List,
    Tuple,
    MapKey,
    MapValue,
    Ok,
    Err,
    Some,
    /// A `${…}` interpolation part.
    Interp,
    /// A `fan` arm.
    Fan,
}

/// The syntactic position an occurrence sits in: what the ENCLOSING node
/// does with the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    /// The value flows out: the analysed body's tail, a block tail, an `if`
    /// branch, a match arm body, either arm of `??`, a lambda body.
    Result,
    /// The subject of a `match`.
    Scrutinee,
    /// A concatenation operand.
    Concat,
    /// A constructor operand.
    Construct(Ctor),
    /// An argument of a call, with the callee slot's mode.
    Arg(SlotMode),
    /// A method call's receiver.
    Receiver,
    /// The callee of a computed call.
    Callee,
    /// A function value handed to an iterator step or collector.
    Callback,
    /// A loop's iterable, or an iterator chain's source.
    Iterable { consumed: bool },
    /// The seed of a fold collector.
    FoldInit,
    /// The operand of a `Borrow` node.
    Borrow { mutable: bool },
    /// The operand of a `Clone` node.
    Clone,
    /// The object of a field read.
    Member,
    /// The object of a tuple-index read.
    TupleIndex,
    /// The object of a list index read.
    Index,
    /// The object of a map lookup.
    MapKeyed,
    /// The operand of a `Deref` node.
    Deref,
    /// Any other read that keeps the value where it is: a non-concat binary
    /// or unary operand, a range bound, an index or key, a condition, a
    /// guard, a macro or inline-template argument, a `take` count, the
    /// operand of `?` / `!` / `?.` / `Box::new` / `to_vec` / `RcWrap`, an
    /// expression statement, a `ListCopySlice` source.
    Operand,
    /// The value of a `let` / `var` / destructuring bind, or of any
    /// assignment statement.
    Assigned,
    /// The variable an `Assign` statement rebinds (a statement target, not
    /// a `Var` node).
    Reassign,
    /// The container an in-place write statement mutates (`xs[i] = v`,
    /// `r.f = v`, `m[k] = v`, the list peepholes) — a statement target.
    InPlace,
}

/// The occurrence is the base of a place projection (`v.a.b`, `v.0`, `*v`):
/// the position of the WHOLE chain, its depth, and whether the chain's value
/// is heap-typed (clones rather than copies).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chain {
    pub top: Site,
    pub len: u32,
    pub heap: bool,
}

/// One occurrence of a local.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Use {
    pub var: VarId,
    pub site: Site,
    /// Set when `site` is `Member` / `TupleIndex` / `Deref` — the projection
    /// chain this occurrence roots.
    pub chain: Option<Chain>,
    /// Lambda nesting below the analysed root: `0` outside every closure,
    /// so `depth > 0` means the occurrence is a capture.
    pub depth: u32,
    /// Inside an iterator chain node (its source included).
    pub in_chain: bool,
    /// Inside the operand of a `&mut` borrow (the operand itself included):
    /// the value is reachable through a live mutable borrow.
    pub in_mut: bool,
    /// Inside a `for` / `while` body below the analysed root: the occurrence
    /// runs once per iteration, so a variable bound outside the loop has a
    /// "later" use at every one of its own occurrences in the body.
    pub in_loop: bool,
    /// The OUTERMOST lambda (its `lambda_id`) this occurrence sits in, when
    /// `depth > 0`: the closure that captures the variable from the analysed
    /// body. `None` outside every closure, or for a lambda without an id.
    pub outer_lambda: Option<u32>,
    /// The statement the occurrence belongs to, as an ordinal over the walk:
    /// two occurrences with the same ordinal evaluate inside one statement
    /// (or one block tail), so a borrow one of them holds can still be live
    /// when the other runs.
    pub stmt: u32,
    /// The ordinal of the OUTERMOST statement enclosing this occurrence —
    /// the one at the top level of the analysed body. A `Block` nested in an
    /// expression (a capture-clone binding hoisted in front of its closure,
    /// an inlined `let`) numbers its own statements, so `stmt` splits one
    /// evaluation into several ordinals; a borrow the outer statement holds
    /// (a `format_args!` part, a `&v` argument) is live across all of them.
    pub top_stmt: u32,
    /// The innermost conditional ARM the occurrence sits in (an `if` branch,
    /// a `match` arm, a loop body, a lambda body), `0` for none. Arms form a
    /// tree ([`UseSites::keeps_live`]): an occurrence in an ancestor arm
    /// or the same arm runs whenever this one does; one in a sibling arm may
    /// not run at all.
    pub arm: u32,
    /// The occurrence sits among the arguments of a call that also passes a
    /// direct `&v` of this same variable: the clone pass's E0505 guard
    /// (`call_borrowed_vars`, #809 / #866) forces a clone here regardless of
    /// last use, so this occurrence can never be a move.
    pub guard_forced: bool,
    /// For an occurrence inside a closure (`depth > 0`): a call, `match` or
    /// loop enclosing the OUTERMOST closure holds a borrow of this variable
    /// while that closure is built — a direct `&v` / `&v.f` argument or
    /// receiver, a match subject, a by-reference iterable. The closure cannot
    /// move the variable then (E0505); a borrow a sibling subexpression took
    /// has ended by then, and one inside the closure body happens when the
    /// closure runs. `false` outside closures — except inside a `fan` arm,
    /// where it says the same of the nodes enclosing the fan (#2239).
    pub held_across: bool,
    /// The OUTERMOST `fan` arm this occurrence sits in, at lambda depth 0:
    /// the fan node's span and the arm's index in evaluation order. A fan
    /// arm is an implicit move closure without a `lambda_id`, so this is the
    /// identity the capture-move rule keys on (#2239); `None` outside every
    /// fan, or for a fan node without a span.
    pub fan_arm: Option<(almide_base::span::Span, u32)>,
}

impl Use {
    /// Does this occurrence write the variable? `through_chain` also counts a
    /// `&mut` of a projection rooted at it (`&mut r.items`).
    pub fn is_write(&self, through_chain: bool) -> bool {
        match self.site {
            Site::Reassign | Site::InPlace | Site::Borrow { mutable: true } => true,
            _ => through_chain && matches!(self.chain, Some(Chain { top: Site::Borrow { mutable: true }, .. })),
        }
    }

    /// Is this occurrence a `Var` node — as opposed to a statement target,
    /// which names the variable without an expression node?
    pub fn is_node(&self) -> bool {
        !matches!(self.site, Site::Reassign | Site::InPlace)
    }
}

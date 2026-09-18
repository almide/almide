//! What can go wrong, in two disjoint kinds: a module the VM refuses to load
//! (REQ-VM-2: everything outside the closed instruction and feature set is
//! refused before any code runs), and a trap while running an accepted one.

use std::fmt;

/// A module the VM will not load or instantiate. `offset` is the byte offset
/// in the binary where the refusal was decided, when one byte decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub offset: Option<usize>,
    pub reason: String,
}

impl LoadError {
    pub fn new(offset: usize, reason: impl Into<String>) -> Self {
        LoadError { offset: Some(offset), reason: reason.into() }
    }

    /// A refusal about the module as a whole (a cross-section check, or a
    /// limit the instance could not honour).
    pub fn whole(reason: impl Into<String>) -> Self {
        LoadError { offset: None, reason: reason.into() }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(at) => write!(f, "module refused at byte {at:#x}: {}", self.reason),
            None => write!(f, "module refused: {}", self.reason),
        }
    }
}

impl std::error::Error for LoadError {}

/// A trap: the run stops, and no further instruction executes (REQ-VM-7).
/// Each one displays as the reason the embedded host's runtime spells for the
/// same trap, so a trapped run's one stderr line is the same on both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    Unreachable,
    IntegerDivideByZero,
    IntegerOverflow,
    MemoryOutOfBounds,
    IndirectCallToNull,
    IndirectCallOutOfTable,
    IndirectCallTypeMismatch,
    /// A call would exceed the fixed operand-stack capacity or the fixed call
    /// depth (REQ-VM-5).
    CallStackExhausted,
    /// The fuel budget ran out (REQ-VM-6).
    OutOfFuel,
    /// A host import this VM declares but does not serve was called.
    HostCallNotServed(&'static str),
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Trap::Unreachable => "wasm `unreachable` instruction executed",
            Trap::IntegerDivideByZero => "integer divide by zero",
            Trap::IntegerOverflow => "integer overflow",
            Trap::MemoryOutOfBounds => "out of bounds memory access",
            Trap::IndirectCallToNull => "uninitialized element",
            Trap::IndirectCallOutOfTable => "undefined element: out of bounds table access",
            Trap::IndirectCallTypeMismatch => "indirect call type mismatch",
            Trap::CallStackExhausted => "call stack exhausted",
            Trap::OutOfFuel => "all fuel consumed by WebAssembly",
            Trap::HostCallNotServed(name) => return write!(f, "host call `{name}` is not served by this VM"),
        };
        f.write_str(text)
    }
}

impl std::error::Error for Trap {}

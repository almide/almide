/// Global string interner for identifiers (variable names, type names, field names, etc.).
///
/// `Sym` is a Copy handle into the global interner. Interning the same string twice
/// returns the same `Sym`, making equality checks O(1) and clones free.
///
/// Uses a global `ThreadedRodeo` so that `resolve()` returns `&'static str`.

use lasso::{ThreadedRodeo, Spur};
use std::fmt;
use std::sync::LazyLock;

/// An interned identifier. Copy, Eq, Hash — zero-cost clone.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sym(Spur);

static INTERNER: LazyLock<ThreadedRodeo> = LazyLock::new(ThreadedRodeo::default);

/// Intern a string, returning a `Sym` handle.
pub fn sym(s: &str) -> Sym {
    Sym(INTERNER.get_or_intern(s))
}

/// Resolve a `Sym` back to `&'static str`.
pub fn resolve(s: Sym) -> &'static str {
    // SAFETY: INTERNER is a global static that lives for the entire program.
    // ThreadedRodeo never moves or deallocates interned strings.
    // The returned &str has the same lifetime as the interner: 'static.
    let interner: &ThreadedRodeo = &INTERNER;
    let resolved: &str = interner.resolve(&s.0);
    // Extend lifetime — safe because the interner (and its strings) are 'static.
    unsafe { &*(resolved as *const str) }
}

impl Sym {
    /// Get the interned string as `&'static str`.
    pub fn as_str(self) -> &'static str {
        resolve(self)
    }

    /// Check if this sym matches a string without allocating.
    pub fn eq_str(&self, s: &str) -> bool {
        resolve(*self) == s
    }
}

impl fmt::Debug for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", resolve(*self))
    }
}

impl fmt::Display for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(resolve(*self))
    }
}

impl From<&str> for Sym {
    fn from(s: &str) -> Self {
        sym(s)
    }
}

impl From<String> for Sym {
    fn from(s: String) -> Self {
        sym(&s)
    }
}

impl From<&String> for Sym {
    fn from(s: &String) -> Self {
        sym(s)
    }
}

impl AsRef<str> for Sym {
    fn as_ref(&self) -> &str {
        resolve(*self)
    }
}

impl std::ops::Deref for Sym {
    type Target = str;
    fn deref(&self) -> &str {
        resolve(*self)
    }
}

// NOTE: We intentionally do NOT implement Borrow<str> for Sym.
// Sym::Hash is based on Spur's integer ID (fast), while str::Hash is content-based.
// Implementing Borrow<str> would violate the Hash consistency requirement.
// For HashMap<Sym, V> lookups with &str, use: map.get(&sym(s))

impl serde::Serialize for Sym {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(resolve(*self))
    }
}

impl<'de> serde::Deserialize<'de> for Sym {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(sym(&s))
    }
}

impl PartialEq<str> for Sym {
    fn eq(&self, other: &str) -> bool {
        resolve(*self) == other
    }
}

impl PartialEq<&str> for Sym {
    fn eq(&self, other: &&str) -> bool {
        resolve(*self) == *other
    }
}

impl PartialEq<String> for Sym {
    fn eq(&self, other: &String) -> bool {
        resolve(*self) == other.as_str()
    }
}

impl PartialEq<Sym> for str {
    fn eq(&self, other: &Sym) -> bool {
        self == resolve(*other)
    }
}

impl PartialEq<Sym> for &str {
    fn eq(&self, other: &Sym) -> bool {
        *self == resolve(*other)
    }
}

impl PartialEq<Sym> for String {
    fn eq(&self, other: &Sym) -> bool {
        self.as_str() == resolve(*other)
    }
}

impl Default for Sym {
    fn default() -> Self {
        sym("")
    }
}
impl PartialOrd for Sym {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Sym {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 == other.0 {
            std::cmp::Ordering::Equal
        } else {
            resolve(*self).cmp(resolve(*other))
        }
    }
}

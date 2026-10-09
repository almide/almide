/// Global string interner for identifiers (variable names, type names, field names, etc.).
///
/// `Sym` is a Copy handle into the global interner. Interning the same string twice
/// returns the same `Sym`, making equality checks O(1) and clones free.
///
/// Uses a global `ThreadedRodeo` so that `resolve()` returns `&'static str`.

use lasso::{Key, ThreadedRodeo, Spur};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::LazyLock;

/// An interned identifier. Copy, Eq, Hash — zero-cost clone.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sym(Spur);

static INTERNER: LazyLock<ThreadedRodeo> = LazyLock::new(ThreadedRodeo::default);

// Per-thread front caches (#3509). The shared interner takes a shard lock and
// a SipHash on every call; nothing is ever removed from it, so a thread may
// remember any answer it has seen and never consult the interner for it again.
thread_local! {
    static RESOLVED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static INTERNED: RefCell<HashMap<&'static str, Sym, BuildHasherDefault<WordHasher>>> =
        RefCell::new(HashMap::default());
}

/// Multiply-rotate word hash for the per-thread cache: keys are short
/// identifiers this process already holds, so no DoS resistance is needed.
#[derive(Default)]
struct WordHasher(u64);

impl Hasher for WordHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        let mut word = [0u8; 8];
        for c in &mut chunks {
            word.copy_from_slice(c);
            self.add(u64::from_le_bytes(word));
        }
        let mut tail = [0u8; 8];
        let rest = chunks.remainder();
        tail[..rest.len()].copy_from_slice(rest);
        self.add(u64::from_le_bytes(tail) ^ ((rest.len() as u64) << 56));
    }
    fn write_u8(&mut self, b: u8) {
        self.add(b as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

impl WordHasher {
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

/// Intern a string, returning a `Sym` handle.
pub fn sym(s: &str) -> Sym {
    if let Some(hit) = INTERNED.with(|m| m.borrow().get(s).copied()) {
        return hit;
    }
    let k = Sym(INTERNER.get_or_intern(s));
    INTERNED.with(|m| m.borrow_mut().insert(resolve_shared(k), k));
    k
}

/// Resolve a `Sym` back to `&'static str`.
pub fn resolve(s: Sym) -> &'static str {
    let i = s.0.into_usize();
    if let Some(hit) = RESOLVED.with(|v| v.borrow().get(i).copied()) {
        return hit;
    }
    let text = resolve_shared(s);
    RESOLVED.with(|v| {
        let mut v = v.borrow_mut();
        // Fill every index up to `i` so the cache stays a dense prefix.
        while v.len() < i {
            let gap = Spur::try_from_usize(v.len()).expect("a smaller key of a live key exists");
            v.push(resolve_shared(Sym(gap)));
        }
        v.push(text);
    });
    text
}

fn resolve_shared(s: Sym) -> &'static str {
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
    /// Compares by string content (O(n)) for deterministic ordering across
    /// compiler invocations. Internal data structures sort Syms for canonical
    /// field order in generated code, so the ordering must be stable.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 == other.0 {
            std::cmp::Ordering::Equal
        } else {
            resolve(*self).cmp(resolve(*other))
        }
    }
}

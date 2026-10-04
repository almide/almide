use std::collections::{HashMap, HashSet};

/// Effect categories — mapped from stdlib module usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Effect {
    IO,
    Net,
    Env,
    Time,
    Rand,
    Fan,
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Effect::IO => write!(f, "IO"),
            Effect::Net => write!(f, "Net"),
            Effect::Env => write!(f, "Env"),
            Effect::Time => write!(f, "Time"),
            Effect::Rand => write!(f, "Rand"),
            Effect::Fan => write!(f, "Fan"),
        }
    }
}

impl Effect {
    /// Every category, in declaration order. This is the whole vocabulary of
    /// `almide.toml [permissions].allow`: a name is valid there exactly when
    /// it is the `Display` of one of these (#3247).
    pub const ALL: [Effect; 6] = [Effect::IO, Effect::Net, Effect::Env, Effect::Time, Effect::Rand, Effect::Fan];

    /// The category a `[permissions].allow` name spells, or `None`.
    pub fn from_name(name: &str) -> Option<Effect> {
        Self::ALL.into_iter().find(|e| e.to_string() == name)
    }
}

/// Result of effect inference for a single function.
#[derive(Debug, Clone, Default)]
pub struct FunctionEffects {
    pub direct: HashSet<Effect>,
    pub transitive: HashSet<Effect>,
    pub is_effect: bool,
    /// The fn-typed parameters whose closures this function may run (#3268),
    /// as `f (arg 1)`, in parameter order. What they do is charged at each
    /// call site, with the closure that call site passes (ADR-0026 D1): a
    /// bare fn-type parameter is transparent for the category set. A
    /// callee is never invented for them (ADR-0026 D4).
    pub indirect: Vec<String>,
    /// The categories of the closure this function returns, when it returns
    /// one: a returned fn value keeps the set of the value returned.
    pub returns: HashSet<Effect>,
    /// Parameters whose closures the returned closure may run.
    pub returns_indirect: Vec<String>,
    /// One representative path per category in `transitive`, from this
    /// function to the operation (ADR-0026 D4): `main → use_it → f (arg 1 of
    /// use_it) → closure (line 4:42 in make) → fs.read_text (line 4:51)`.
    pub paths: HashMap<Effect, String>,
}

impl FunctionEffects {
    /// No category of its own, but it runs closures it does not create.
    pub fn is_callback_dependent(&self) -> bool {
        self.transitive.is_empty() && !self.indirect.is_empty()
    }

    /// The `almide check --effects` cell: `{IO}`, `{}`, or
    /// `{} + whatever f (arg 1) does`.
    pub fn report(&self) -> String {
        let set = EffectMap::format_effects(&self.transitive);
        match self.indirect.as_slice() {
            [] => set,
            [one] => format!("{set} + whatever {one} does"),
            many => format!("{set} + whatever {} do", many.join(", ")),
        }
    }

    /// `returns a closure doing {IO}`, when the function returns an
    /// effectful closure (or one that runs a closure it was handed).
    pub fn returns_report(&self) -> Option<String> {
        if self.returns.is_empty() && self.returns_indirect.is_empty() {
            return None;
        }
        let set = EffectMap::format_effects(&self.returns);
        Some(match self.returns_indirect.as_slice() {
            [] => format!("returns a closure doing {set}"),
            many => format!("returns a closure doing {set} + whatever {} does", many.join(", ")),
        })
    }
}

/// Effect analysis results for the entire program.
#[derive(Debug, Clone, Default)]
pub struct EffectMap {
    pub functions: HashMap<String, FunctionEffects>,
}

impl EffectMap {
    /// `(pure, callback-dependent, with effects)` over `functions`: a
    /// function that only calls closures it is handed is not counted pure.
    pub fn summary_counts<'a>(functions: impl Iterator<Item = &'a FunctionEffects>) -> (usize, usize, usize) {
        functions.fold((0, 0, 0), |(p, d, e), fe| match (fe.transitive.is_empty(), fe.indirect.is_empty()) {
            (true, true) => (p + 1, d, e),
            (true, false) => (p, d + 1, e),
            (false, _) => (p, d, e + 1),
        })
    }

    pub fn format_effects(effects: &HashSet<Effect>) -> String {
        if effects.is_empty() {
            return "{}".to_string();
        }
        let mut sorted: Vec<_> = effects.iter().collect();
        sorted.sort();
        format!("{{{}}}", sorted.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "))
    }
}

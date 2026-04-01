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

/// Result of effect inference for a single function.
#[derive(Debug, Clone, Default)]
pub struct FunctionEffects {
    pub direct: HashSet<Effect>,
    pub transitive: HashSet<Effect>,
    pub is_effect: bool,
}

/// Effect analysis results for the entire program.
#[derive(Debug, Clone, Default)]
pub struct EffectMap {
    pub functions: HashMap<String, FunctionEffects>,
}

impl EffectMap {
    pub fn format_effects(effects: &HashSet<Effect>) -> String {
        if effects.is_empty() {
            return "{}".to_string();
        }
        let mut sorted: Vec<_> = effects.iter().collect();
        sorted.sort();
        format!("{{{}}}", sorted.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "))
    }
}

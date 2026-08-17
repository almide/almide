//! The generative engine: type-directed synthesis + corpus mutation +
//! the self-checking identity family.
//!
//! Public entry point is [`Engine::generate`], which deterministically
//! produces one Almide program from `(seed, index)`. The split between the
//! families is a named weight table so the campaign can be re-tuned
//! without touching call sites.
//!
//! The three families differ in what ORACLE judges them:
//!
//! | family     | oracle |
//! |------------|--------|
//! | synthesis  | differential (native ⇄ wasm), plus the interp when it votes |
//! | mutation   | same |
//! | identity   | **by construction** — the program's expected stdout is a literal in its own source (#1332) |
//!
//! Only the third can catch a bug that both backends share, because only
//! it needs no second execution to know the right answer.

mod catalogue;
mod denylist;
pub mod identity;
mod mutate;
mod pools;
mod program;
mod sig_type;
mod term;
mod types;

pub use catalogue::{build as build_catalogue, Signature};

use crate::rng::SplitMix64;

/// Relative weight of type-directed synthesis.
const SYNTHESIS_WEIGHT: u32 = 7;
/// Relative weight of corpus mutation.
const MUTATION_WEIGHT: u32 = 3;
/// Relative weight of the self-checking identity family (#1332), against
/// the OTHER two combined — i.e. `3 / (10 + 3)` ≈ 23% of a mixed campaign.
const IDENTITY_WEIGHT: u32 = 3;

/// Salt for the identity family's own RNG sub-stream.
///
/// The family decision is drawn from `for_program(seed ^ SALT, index)`,
/// NOT from the program's main stream, for one reason: every draw added
/// to the main stream re-keys it, so `(seed, index)` would stop
/// regenerating the program a previously recorded finding was minimized
/// from. Splitting the decision off keeps the pre-#1332 synthesis and
/// mutation streams byte-identical, so archived seeds still replay.
const IDENTITY_STREAM_SALT: u64 = 0x1332_0AC1_E5EE_D501;

/// Which families a campaign draws from. Recorded in every finding's
/// `meta.txt`, because `(seed, index)` only reproduces a program under
/// the SAME family setting: `All` spends one draw on the family roll
/// before generating, `Identity` spends none, so the two disagree from
/// the first byte at the same index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Every family, at the weights above (the nightly default).
    All,
    /// The self-checking identity family only — a campaign run entirely
    /// under the by-construction oracle.
    Identity,
    /// Type-directed synthesis only.
    Synthesis,
}

impl Family {
    pub fn parse(s: &str) -> Option<Family> {
        match s {
            "all" => Some(Family::All),
            "identity" => Some(Family::Identity),
            "synthesis" => Some(Family::Synthesis),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Family::All => "all",
            Family::Identity => "identity",
            Family::Synthesis => "synthesis",
        }
    }
}

/// A generated program plus the metadata needed to reproduce and triage
/// it.
pub struct Generated {
    /// The Almide source text.
    pub source: String,
    /// How it was produced (for the findings report).
    pub origin: Origin,
    /// The stdout this program MUST produce, known by construction.
    /// `Some` only for the identity family; every other family is judged
    /// differentially and leaves this `None`.
    pub expected_stdout: Option<String>,
    /// The structured plan behind an identity program, so a finding can be
    /// shrunk WITHIN the family (text-level shrinking would break the
    /// identity invariant and invalidate the oracle).
    pub plan: Option<identity::Plan>,
}

impl Generated {
    fn plain(source: String, origin: Origin) -> Self {
        Generated { source, origin, expected_stdout: None, plan: None }
    }
}

/// Provenance of a generated program.
#[derive(Debug, Clone)]
pub enum Origin {
    /// Built from scratch by the type-directed term generator.
    Synthesis,
    /// Produced by mutating a corpus file (path recorded for triage).
    Mutation { corpus_file: String },
    /// Built backwards from a known answer by the identity family.
    Identity { blocks: usize },
}

/// Everything the generator needs that is constant across the campaign.
pub struct Engine {
    catalogue: Vec<Signature>,
    corpus: Vec<mutate::CorpusEntry>,
    family: Family,
}

impl Engine {
    /// Build the engine once: parse the stdlib catalogue and load the
    /// mutation corpus from `spec/`.
    pub fn new(corpus_root: &std::path::Path) -> Self {
        Engine::with_family(corpus_root, Family::All)
    }

    /// Build the engine restricted to one family.
    pub fn with_family(corpus_root: &std::path::Path, family: Family) -> Self {
        Engine {
            catalogue: build_catalogue(),
            corpus: mutate::load_corpus(corpus_root),
            family,
        }
    }

    pub fn family(&self) -> Family {
        self.family
    }

    /// Number of catalogued stdlib signatures (diagnostics).
    pub fn catalogue_len(&self) -> usize {
        self.catalogue.len()
    }

    /// Number of parseable corpus programs available for mutation.
    pub fn corpus_len(&self) -> usize {
        self.corpus.len()
    }

    /// Deterministically generate program `index` of campaign `seed`.
    pub fn generate(&self, seed: u64, index: u64) -> Generated {
        // The identity family draws from its OWN sub-stream — see
        // IDENTITY_STREAM_SALT for why the main stream must stay untouched.
        let mut alt = SplitMix64::for_program(seed ^ IDENTITY_STREAM_SALT, index);

        match self.family {
            Family::Identity => return self.identity(&mut alt),
            Family::Synthesis => {
                let mut rng = SplitMix64::for_program(seed, index);
                return program::synthesize(&mut rng, &self.catalogue);
            }
            Family::All => {
                if alt.pick_weighted(&[SYNTHESIS_WEIGHT + MUTATION_WEIGHT, IDENTITY_WEIGHT]) == 1 {
                    return self.identity(&mut alt);
                }
            }
        }

        // Byte-for-byte what it was before #1332: the synthesis / mutation
        // split reads the main stream in exactly the same order, so an
        // archived `(seed, index)` still regenerates its program.
        let mut rng = SplitMix64::for_program(seed, index);
        let use_mutation =
            !self.corpus.is_empty() && rng.pick_weighted(&[SYNTHESIS_WEIGHT, MUTATION_WEIGHT]) == 1;

        if use_mutation {
            if let Some(g) = mutate::mutate_one(&mut rng, &self.corpus, &self.catalogue) {
                return g;
            }
            // Fall through to synthesis on mutation failure.
        }

        program::synthesize(&mut rng, &self.catalogue)
    }

    /// One program of the self-checking identity family, carrying both its
    /// by-construction oracle and the plan the shrinker needs.
    fn identity(&self, rng: &mut SplitMix64) -> Generated {
        let plan = identity::plan(rng);
        let (source, expected) = identity::render(&plan);
        Generated {
            source,
            origin: Origin::Identity { blocks: plan.size() },
            expected_stdout: Some(expected),
            plan: Some(plan),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-parse a generated program; returns whether the parser accepted
    /// it. (Full type-checking is exercised by the oracle's `check` rung,
    /// not in a unit test — the parser gate alone catches the bulk of
    /// generator syntax bugs.)
    fn parses(src: &str) -> bool {
        let tokens = almide::lexer::Lexer::tokenize(src);
        let mut parser = almide::parser::Parser::new(tokens);
        parser.parse().is_ok()
    }

    /// The catalogue must extract a substantial signature surface from
    /// the bundled stdlib — a regression to near-zero would silently
    /// gut detection power.
    #[test]
    fn catalogue_is_populated() {
        let catalogue = build_catalogue();
        assert!(
            catalogue.len() > 100,
            "catalogue unexpectedly small: {}",
            catalogue.len()
        );
        // Spot-check a divergence-prone signature is present and weighted.
        let to_upper = catalogue
            .iter()
            .find(|s| s.module == "string" && s.func == "to_upper")
            .expect("string.to_upper missing from catalogue");
        assert!(to_upper.weight > 1, "to_upper should be boosted");
    }

    /// Synthesis-only generation must always emit parseable source. This
    /// runs against a catalogue but an empty corpus so only the
    /// type-directed path is exercised.
    #[test]
    fn synthesized_programs_parse() {
        let catalogue = build_catalogue();
        for index in 0..200u64 {
            let mut rng = SplitMix64::for_program(0xF00D, index);
            let g = program::synthesize(&mut rng, &catalogue);
            assert!(
                parses(&g.source),
                "synthesized program {index} did not parse:\n{}",
                g.source
            );
        }
    }

    /// The mixed campaign must actually draw the self-checking family at
    /// roughly its declared weight. A regression to zero would leave the
    /// nightly differential-only again — green, and blind to exactly the
    /// class #1332 exists to catch — with nothing else failing.
    #[test]
    fn the_mixed_campaign_draws_the_identity_family() {
        // No corpus root ⇒ mutation is unavailable, which is fine: the
        // question is only whether the identity interception fires, and it
        // is decided before the corpus is ever consulted.
        let engine = Engine::with_family(std::path::Path::new("/nonexistent"), Family::All);
        const N: u64 = 600;
        let mut identity = 0usize;
        for index in 0..N {
            let g = engine.generate(1332, index);
            if matches!(g.origin, Origin::Identity { .. }) {
                identity += 1;
                assert!(
                    g.expected_stdout.is_some() && g.plan.is_some(),
                    "an identity program must carry its oracle AND its plan"
                );
            } else {
                assert!(
                    g.expected_stdout.is_none(),
                    "only the identity family may claim a by-construction oracle"
                );
            }
        }
        let share = identity as f64 / N as f64;
        let declared = IDENTITY_WEIGHT as f64
            / (SYNTHESIS_WEIGHT + MUTATION_WEIGHT + IDENTITY_WEIGHT) as f64;
        assert!(
            (share - declared).abs() < 0.06,
            "identity share {share:.3} strayed from the declared {declared:.3}"
        );
    }

    /// `(seed, index)` must map to a byte-identical program every time —
    /// the reproducibility contract the whole findings pipeline rests on.
    #[test]
    fn generation_is_deterministic() {
        let catalogue = build_catalogue();
        for index in [0u64, 1, 7, 99] {
            let mut a = SplitMix64::for_program(123, index);
            let mut b = SplitMix64::for_program(123, index);
            let pa = program::synthesize(&mut a, &catalogue);
            let pb = program::synthesize(&mut b, &catalogue);
            assert_eq!(pa.source, pb.source, "non-deterministic at index {index}");
        }
    }
}

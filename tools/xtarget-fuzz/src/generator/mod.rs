//! The generative engine: type-directed synthesis (70%) + corpus
//! mutation (30%).
//!
//! Public entry point is [`generate`], which deterministically produces
//! one Almide program from `(seed, index)`. The split between synthesis
//! and mutation is a named ratio (`SYNTHESIS_WEIGHT` / `MUTATION_WEIGHT`)
//! so the campaign can be re-tuned without touching call sites.

mod catalogue;
mod denylist;
mod mutate;
mod pools;
mod program;
mod sig_type;
mod term;
mod types;

pub use catalogue::{build as build_catalogue, Signature};

use crate::rng::SplitMix64;

/// Relative weight of type-directed synthesis (the target ~70%).
const SYNTHESIS_WEIGHT: u32 = 7;
/// Relative weight of corpus mutation (the target ~30%).
const MUTATION_WEIGHT: u32 = 3;

/// A generated program plus the metadata needed to reproduce and triage
/// it.
pub struct Generated {
    /// The Almide source text.
    pub source: String,
    /// How it was produced (for the findings report).
    pub origin: Origin,
}

/// Provenance of a generated program.
#[derive(Debug, Clone)]
pub enum Origin {
    /// Built from scratch by the type-directed term generator.
    Synthesis,
    /// Produced by mutating a corpus file (path recorded for triage).
    Mutation { corpus_file: String },
}

/// Everything the generator needs that is constant across the campaign.
pub struct Engine {
    catalogue: Vec<Signature>,
    corpus: Vec<mutate::CorpusEntry>,
}

impl Engine {
    /// Build the engine once: parse the stdlib catalogue and load the
    /// mutation corpus from `spec/`.
    pub fn new(corpus_root: &std::path::Path) -> Self {
        Engine {
            catalogue: build_catalogue(),
            corpus: mutate::load_corpus(corpus_root),
        }
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
        let mut rng = SplitMix64::for_program(seed, index);

        // Choose synthesis vs mutation. Mutation is only available when
        // the corpus loaded; otherwise we always synthesize.
        let use_mutation = !self.corpus.is_empty()
            && rng.pick_weighted(&[SYNTHESIS_WEIGHT, MUTATION_WEIGHT]) == 1;

        if use_mutation {
            if let Some(g) = mutate::mutate_one(&mut rng, &self.corpus, &self.catalogue) {
                return g;
            }
            // Fall through to synthesis on mutation failure.
        }

        program::synthesize(&mut rng, &self.catalogue)
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

//! The language dialect epoch.
//!
//! A file may carry `@dialect(N)` recording the dialect it was last verified
//! against. `N` counts BREAKING language-surface changes, not releases: see
//! `proofs/dialect-epochs.toml`, which is the normative record of what each
//! epoch changed and is cross-checked against the constant below by
//! `scripts/check-dialect-epochs.sh` (the same shape as rustc's
//! `CURRENT_RUSTC_VERSION` placeholder check — a version that is maintained
//! in two places by hand is a version that will disagree with itself).

/// The dialect this compiler speaks. Bump ONLY together with a new
/// `[[epoch]]` entry in `proofs/dialect-epochs.toml`; the gate fails if the
/// two disagree.
pub const CURRENT_DIALECT: u32 = 3;

/// What a stamp means relative to this compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialectStanding {
    /// No stamp. Legal and silent — every file written before stamps existed
    /// is unstamped, and demanding one would be the breaking change stamps
    /// exist to avoid.
    Unstamped,
    /// Written for this dialect.
    Current,
    /// Written for an older dialect. Not an error on its own: the file may
    /// use nothing that moved. The value is what a future migration
    /// diagnostic joins against to say WHAT changed since `epoch`.
    Stale { epoch: u32 },
    /// Written for a dialect this compiler does not know. This IS an error:
    /// the file was verified against a newer language than the one being
    /// asked to compile it, so any success here would be an accident.
    Ahead { epoch: u32 },
}

pub fn standing(stamp: Option<u32>) -> DialectStanding {
    match stamp {
        None => DialectStanding::Unstamped,
        Some(n) if n == CURRENT_DIALECT => DialectStanding::Current,
        Some(n) if n < CURRENT_DIALECT => DialectStanding::Stale { epoch: n },
        Some(n) => DialectStanding::Ahead { epoch: n },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standings_partition_the_stamp_space() {
        assert_eq!(standing(None), DialectStanding::Unstamped);
        assert_eq!(standing(Some(CURRENT_DIALECT)), DialectStanding::Current);
        assert_eq!(
            standing(Some(CURRENT_DIALECT - 1)),
            DialectStanding::Stale { epoch: CURRENT_DIALECT - 1 }
        );
        assert_eq!(
            standing(Some(CURRENT_DIALECT + 1)),
            DialectStanding::Ahead { epoch: CURRENT_DIALECT + 1 }
        );
    }

    /// A stale stamp must never be an error — the whole point is that a file
    /// written for an older dialect keeps compiling until it actually uses
    /// something that moved.
    #[test]
    fn every_older_epoch_is_stale_not_ahead() {
        for n in 1..CURRENT_DIALECT {
            assert!(matches!(standing(Some(n)), DialectStanding::Stale { .. }));
        }
    }
}

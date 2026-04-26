// region:    --- Modules

mod complete;
mod matchers;
mod parse;
mod types;

pub use complete::complete;
pub use parse::{has_actionable_hunks, has_tilde_ranges, split_raw_hunks};
pub use types::MatchTier;

// endregion: --- Modules

// region:    --- Constants

/// Maximum lines to search away from the expected position for lenient (Resilient/Fuzzy) matches.
/// This prevents a hunk from "drifting" too far and causing subsequent hunks to fail.
const MAX_PROXIMITY_FOR_LENIENT: usize = 1000;

/// Minimum length for a patch context fragment to be eligible for suffix matching.
/// This prevents very short strings (e.g., `"x"`) from false-positive matching.
const SUFFIX_MATCH_MIN_LEN: usize = 10;

/// Minimum number of `-` lines required above and below a `~` range-remove marker.
const TILDE_MIN_ANCHOR_LINES: usize = 2;

// endregion: --- Constants

// region:    --- Tests

#[cfg(test)]
mod tests;

// endregion: --- Tests

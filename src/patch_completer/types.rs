// region:    --- Types

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
	Strict,
	Resilient,
	Fuzzy,
}

pub(super) struct HunkBounds {
	pub(super) old_start: usize,
	pub(super) old_count: usize,
	pub(super) new_count: usize,
	pub(super) final_hunk_lines: Vec<String>,
	pub(super) tier: Option<MatchTier>,
}

/// Contextual hints derived from adjacent hunks for disambiguation scoring.
#[derive(Default)]
pub(super) struct AdjacentHints<'a> {
	/// Content of the last context/removal line from the previous hunk (without prefix).
	pub(super) prev_hint: Option<&'a str>,
	/// Content of the first context/removal line from the next hunk (without prefix).
	pub(super) next_hint: Option<&'a str>,
}

/// Represents a parsed `~` range-remove segment within a hunk.
/// The top anchors and bottom anchors are indices into the hunk_lines array.
#[derive(Debug, Clone)]
pub(super) struct TildeRange {
	/// Indices of the `-` lines above the `~` marker (the top anchors).
	pub(super) top_anchor_hl_indices: Vec<usize>,
	/// Index of the `~` line itself in hunk_lines.
	pub(super) tilde_hl_index: usize,
	/// Indices of the `-` lines below the `~` marker (the bottom anchors).
	pub(super) bottom_anchor_hl_indices: Vec<usize>,
}

/// Represents a candidate match found during hunk position search.
pub(super) struct CandidateMatch {
	pub(super) idx: usize,
	pub(super) tier: MatchTier,
	pub(super) overhang_hl_indices: Vec<usize>,
	pub(super) skipped_hl_indices: Vec<usize>,
	pub(super) converted_to_add_indices: Vec<usize>,
	pub(super) matched_orig_indices: Vec<(usize, usize)>,
	pub(super) skipped_blank_orig_indices: Vec<usize>,

	/// Number of context/removal lines that matched without needing normalization or suffix.
	pub(super) exact_ws_count: usize,

	/// Whether all non-blank matched lines have a uniform leading-whitespace delta.
	/// Used as a scoring boost at the Resilient tier.
	pub(super) uniform_indent: bool,

	/// Number of adjacent context hints that matched (0, 1, or 2).
	/// Derived from checking original lines immediately before/after the matched region.
	pub(super) adjacent_hint_matches: usize,
}

// endregion: --- Types

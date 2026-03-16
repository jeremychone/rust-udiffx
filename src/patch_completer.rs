use crate::{Error, Result};
use std::borrow::Cow;

// region:    --- Public Helpers

/// Returns `true` if the raw patch text contains at least one actionable hunk
/// (i.e., a hunk with at least one `+` or `-` line).
pub fn has_actionable_hunks(patch_raw: &str) -> bool {
	let patch_raw: Cow<'_, str> = if patch_raw.contains("\r\n") {
		Cow::Owned(patch_raw.replace("\r\n", "\n"))
	} else {
		Cow::Borrowed(patch_raw)
	};

	let raw_hunks = collect_raw_hunks(&patch_raw);
	if !raw_hunks.is_empty() {
		return true;
	}

	// Retry with sanitized content
	let sanitized = sanitize_wrapper_meta_lines(&patch_raw);
	let raw_hunks = collect_raw_hunks(&sanitized);
	!raw_hunks.is_empty()
}

/// Splits a raw simplified patch (numberless `@@` hunks) into individual hunk strings.
///
/// Each returned `String` contains a single `@@` header followed by its body lines.
/// The splitting reuses the same parsing logic as `complete`: CRLF normalization,
/// sanitize wrapper meta lines, trailing whitespace stripping, and the actionable
/// check (only hunks with at least one `+` or `-` line are included).
pub fn split_raw_hunks(patch_raw: &str) -> Vec<String> {
	let patch_raw: Cow<'_, str> = if patch_raw.contains("\r\n") {
		Cow::Owned(patch_raw.replace("\r\n", "\n"))
	} else {
		Cow::Borrowed(patch_raw)
	};

	let raw_hunks = collect_raw_hunks(&patch_raw);

	if !raw_hunks.is_empty() {
		// Reconstruct each hunk as a self-contained patch string with its @@ header
		return raw_hunks
			.into_iter()
			.map(|lines| {
				let mut hunk_str = String::from("@@\n");
				for line in lines {
					hunk_str.push_str(line);
					hunk_str.push('\n');
				}
				hunk_str
			})
			.collect();
	}

	// If strict parse produced no actionable hunks, retry with sanitized content
	let sanitized = sanitize_wrapper_meta_lines(&patch_raw);
	let raw_hunks = collect_raw_hunks(&sanitized);

	// Reconstruct each hunk as a self-contained patch string with its @@ header
	raw_hunks
		.into_iter()
		.map(|lines| {
			let mut hunk_str = String::from("@@\n");
			for line in lines {
				hunk_str.push_str(line);
				hunk_str.push('\n');
			}
			hunk_str
		})
		.collect()
}

// endregion: --- Public Helpers

// region:    --- Types

/// Maximum lines to search away from the expected position for lenient (Resilient/Fuzzy) matches.
/// This prevents a hunk from "drifting" too far and causing subsequent hunks to fail.
const MAX_PROXIMITY_FOR_LENIENT: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
	Strict,
	Resilient,
	Fuzzy,
}

struct HunkBounds {
	old_start: usize,
	old_count: usize,
	new_count: usize,
	final_hunk_lines: Vec<String>,
	tier: Option<MatchTier>,
}

/// Contextual hints derived from adjacent hunks for disambiguation scoring.
#[derive(Default)]
struct AdjacentHints<'a> {
	/// Content of the last context/removal line from the previous hunk (without prefix).
	prev_hint: Option<&'a str>,
	/// Content of the first context/removal line from the next hunk (without prefix).
	next_hint: Option<&'a str>,
}

// endregion: --- Types

// region:    --- Internal Parsing

/// Collects raw hunk bodies from patch text, returning each hunk as a `Vec<&str>` of body lines.
///
/// Shared by both `split_raw_hunks` and `complete` to avoid duplicating the parsing logic.
fn collect_raw_hunks(patch_text: &str) -> Vec<Vec<&str>> {
	let mut raw_hunks: Vec<Vec<&str>> = Vec::new();
	let mut lines = patch_text.lines().peekable();

	while let Some(line) = lines.next() {
		let trimmed = line.trim();

		if trimmed.starts_with("@@") {
			let mut hunk_lines = Vec::new();
			while let Some(next_line) = lines.peek() {
				let next_trimmed = next_line.trim();
				if next_trimmed.starts_with("@@") {
					break;
				}
				hunk_lines.push(lines.next().unwrap());
			}

			// Strip trailing empty lines that lack a valid diff prefix.
			// These are artefacts of the raw patch text (e.g. a trailing newline)
			// and would otherwise be mis-counted as context lines.
			while hunk_lines.last().is_some_and(|l| l.trim().is_empty()) {
				hunk_lines.pop();
			}

			let has_add = hunk_lines.iter().any(|l| l.starts_with('+'));
			let has_remove = hunk_lines.iter().any(|l| l.starts_with('-'));
			let is_actionable = has_add || has_remove;

			if is_actionable {
				raw_hunks.push(hunk_lines);
			}
		}
	}

	raw_hunks
}

// endregion: --- Internal Parsing

/// Collapses runs of whitespace into a single space for normalized comparison.
fn normalize_ws(s: &str) -> String {
	s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_wrapper_meta_line(trimmed: &str) -> bool {
	trimmed == "*** Begin Patch" || trimmed == "*** End Patch" || trimmed.starts_with("*** Update File:")
}

fn sanitize_wrapper_meta_lines(patch_raw: &str) -> String {
	let mut out = String::new();
	for line in patch_raw.lines() {
		if is_wrapper_meta_line(line.trim()) {
			continue;
		}
		out.push_str(line);
		out.push('\n');
	}
	out
}

/// Completes a raw simplified patch (numberless `@@` hunks) into a fully valid unified diff
/// that can be applied by `diffy`.
///
/// This handles the following:
/// - Locates each hunk's context/removal lines in the original content via greedy search,
///   with resilient matching (trimmed comparison, substring containment) to tolerate
///   LLM whitespace and truncation inaccuracies.
/// - Computes the `@@ -start,len +start,len @@` header for each hunk based on the
///   matched position, tracking cumulative line-count deltas across hunks.
/// - Reconstructs hunk body lines using the original file content (so context/removal
///   lines match the file exactly), while preserving addition lines as-is.
/// - Handles edge cases: blank context lines that don't align with the original are
///   skipped; blank context lines at/beyond EOF are converted to additions to preserve
///   spacing; context that extends past the file is treated as overhang and dropped;
///   and hunks with no context/removal lines are treated as appends to the end of the file.
pub fn complete(original_content: &str, patch_raw: &str) -> Result<(String, Option<MatchTier>)> {
	// Normalize CRLF to LF to prevent subtle mismatches with mixed line endings.
	let original_content: Cow<'_, str> = if original_content.contains("\r\n") {
		Cow::Owned(original_content.replace("\r\n", "\n"))
	} else {
		Cow::Borrowed(original_content)
	};
	let patch_raw: Cow<'_, str> = if patch_raw.contains("\r\n") {
		Cow::Owned(patch_raw.replace("\r\n", "\n"))
	} else {
		Cow::Borrowed(patch_raw)
	};
	let sanitized_patch_raw = sanitize_wrapper_meta_lines(&patch_raw);

	let orig_lines: Vec<&str> = original_content.lines().collect();
	let mut max_tier: Option<MatchTier> = None;

	// -- First pass: collect all hunk bodies as raw line slices using shared helper.
	let mut raw_hunks = collect_raw_hunks(&patch_raw);
	let mut non_hunk_prefix: Vec<&str> = Vec::new();

	// Collect non-hunk prefix lines (e.g. file headers) from before first @@
	for line in patch_raw.lines() {
		let trimmed = line.trim();
		if trimmed.starts_with("@@") {
			break;
		}
		if !is_wrapper_meta_line(trimmed) {
			non_hunk_prefix.push(line);
		}
	}

	// -- If strict parse produced no actionable hunks, retry parse with sanitized content
	// for resilient/fuzzy recovery from wrapper/meta lines.
	if raw_hunks.is_empty() {
		non_hunk_prefix.clear();
		raw_hunks = collect_raw_hunks(&sanitized_patch_raw);

		for line in sanitized_patch_raw.lines() {
			let trimmed = line.trim();
			if trimmed.starts_with("@@") {
				break;
			}
			if !is_wrapper_meta_line(trimmed) {
				non_hunk_prefix.push(line);
			}
		}
	}

	// -- Second pass: compute adjacent hints and process each hunk.
	let mut completed_patch = String::new();
	let mut total_delta: isize = 0;
	let mut search_from: usize = 0;

	// -- Pre-sort hunks by file position to handle out-of-order LLM output.
	// Only reorder when hunks have confident (Strict) position estimates and are out of order.
	let raw_hunks = presort_hunks_by_position(&orig_lines, raw_hunks);

	// Emit any non-hunk prefix lines (e.g. file headers)
	for pline in &non_hunk_prefix {
		completed_patch.push_str(pline);
		completed_patch.push('\n');
	}

	for hunk_idx in 0..raw_hunks.len() {
		// Build adjacent hints for disambiguation
		let hints = build_adjacent_hints(&raw_hunks, hunk_idx);

		let hunk_lines = &raw_hunks[hunk_idx];
		let hunk_bounds = compute_hunk_bounds(&orig_lines, hunk_lines, search_from, &hints)?;
		let old_start = hunk_bounds.old_start;
		let old_count = hunk_bounds.old_count;
		let new_count = hunk_bounds.new_count;
		let final_hunk_lines = hunk_bounds.final_hunk_lines;
		let new_start = (old_start as isize + total_delta) as usize;

		if let Some(t) = hunk_bounds.tier {
			max_tier = Some(max_tier.map(|m| m.max(t)).unwrap_or(t));
		}

		// Update state for next hunk
		search_from = old_start + old_count.saturating_sub(1) - 1;
	total_delta += new_count as isize - old_count as isize;

		// Standard Unified Diff: @@ -start,len +start,len @@
		completed_patch.push_str(&format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"));
		for h_line in final_hunk_lines {
			if h_line.is_empty() {
				completed_patch.push(' ');
			} else {
				completed_patch.push_str(&h_line);
			}
			completed_patch.push('\n');
		}
	}

	if raw_hunks.is_empty() && non_hunk_prefix.is_empty() {
		return Ok((String::new(), None));
	}

	Ok((completed_patch, max_tier))
}

// region:    --- Support

/// Estimates the file position of a hunk by finding the first context/removal line
/// using Strict (exact) matching. Returns `None` if no strict match is found.
fn estimate_hunk_position<'a>(orig_lines: &[&str], hunk_lines: &[&'a str]) -> Option<usize> {
	// Extract the first non-blank context or removal line content
	let first_content = hunk_lines.iter().find_map(|l| {
		if l.starts_with('+') {
			return None;
		}
		let content = if l.len() > 1 { &l[1..] } else { "" };
		if content.trim().is_empty() {
			return None;
		}
		Some(content)
	})?;

	// Search for an exact match in the original lines.
	// If there are multiple exact matches, return None (ambiguous position)
	// to avoid incorrect reordering when duplicate code blocks exist.
	let mut found_idx: Option<usize> = None;
	for (i, orig_line) in orig_lines.iter().enumerate() {
		if *orig_line == first_content {
			if found_idx.is_some() {
				// Multiple matches: ambiguous, bail out
				return None;
			}
			found_idx = Some(i);
		}
	}
	found_idx
}

/// Pre-sorts raw hunks by their estimated file position when out-of-order hunks are detected.
/// Uses only Strict matching for position estimation to avoid false anchoring.
/// Hunks without a confident position estimate keep their original relative order (pushed to end).
fn presort_hunks_by_position<'a>(orig_lines: &[&str], raw_hunks: Vec<Vec<&'a str>>) -> Vec<Vec<&'a str>> {
	if raw_hunks.len() <= 1 {
		return raw_hunks;
	}

	// Estimate positions for each hunk
	let positions: Vec<Option<usize>> = raw_hunks
		.iter()
		.map(|hunk_lines| estimate_hunk_position(orig_lines, hunk_lines))
		.collect();

	// Check if hunks are already in ascending order (considering only those with positions)
	let mut is_ordered = true;
	let mut last_pos: Option<usize> = None;
	for pos in &positions {
		if let Some(p) = pos {
			if let Some(lp) = last_pos {
				if *p < lp {
					is_ordered = false;
					break;
				}
			}
			last_pos = Some(*p);
		}
	}

	if is_ordered {
		return raw_hunks;
	}

	// Stable sort by position; hunks without a position get usize::MAX
	let mut indexed: Vec<(usize, Vec<&'a str>, usize)> = raw_hunks
		.into_iter()
		.enumerate()
		.map(|(i, hunk)| {
			let sort_key = positions[i].unwrap_or(usize::MAX);
			(i, hunk, sort_key)
		})
		.collect();

	indexed.sort_by_key(|(orig_idx, _, sort_key)| (*sort_key, *orig_idx));

	indexed.into_iter().map(|(_, hunk, _)| hunk).collect()
}

/// Extracts the content (without prefix) of the last context/removal line in a hunk.
fn last_context_or_removal_content<'a>(hunk: &[&'a str]) -> Option<&'a str> {
	hunk.iter()
		.rev()
		.find(|l| l.starts_with(' ') || l.starts_with('-'))
		.map(|l| if l.len() > 1 { &l[1..] } else { "" })
}

/// Extracts the content (without prefix) of the first context/removal line in a hunk.
fn first_context_or_removal_content<'a>(hunk: &[&'a str]) -> Option<&'a str> {
	hunk.iter()
		.find(|l| l.starts_with(' ') || l.starts_with('-'))
		.map(|l| if l.len() > 1 { &l[1..] } else { "" })
}

/// Builds adjacent hints for the hunk at `hunk_idx` from the collected raw hunks.
fn build_adjacent_hints<'a>(raw_hunks: &[Vec<&'a str>], hunk_idx: usize) -> AdjacentHints<'a> {
	let prev_hint = if hunk_idx > 0 {
		last_context_or_removal_content(&raw_hunks[hunk_idx - 1])
	} else {
		None
	};

	let next_hint = if hunk_idx + 1 < raw_hunks.len() {
		first_context_or_removal_content(&raw_hunks[hunk_idx + 1])
	} else {
		None
	};

	AdjacentHints { prev_hint, next_hint }
}

/// Checks if a trimmed line is a Markdown heading.
fn is_markdown_heading(s: &str) -> bool {
	s.starts_with('#')
}

/// Strips the leading `#` characters and subsequent whitespace from a Markdown heading.
fn strip_markdown_heading(s: &str) -> &str {
	s.trim_start_matches('#').trim_start()
}

/// Minimum length for a patch context fragment to be eligible for suffix matching.
/// This prevents very short strings (e.g., `"x"`) from false-positive matching.
const SUFFIX_MATCH_MIN_LEN: usize = 10;

/// Checks if a string looks like a comment marker prefix (e.g., "//", "#", "<!--").
/// Used by `suffix_match` to reject false positives where the non-matching prefix
/// is actually a comment marker, preventing non-comment lines from matching
/// comment lines via suffix.
fn is_comment_marker_prefix(prefix: &str) -> bool {
	prefix == "//"
		|| prefix == "#"
		|| prefix == "<!--"
		|| prefix.starts_with("//")
		|| (prefix.starts_with('#') && !prefix.starts_with("#!") && !prefix.starts_with("##"))
		|| prefix.starts_with("<!--")
}

/// Strips a recognized comment marker from a trimmed line and returns the remaining content.
/// Returns `None` if the line does not start with a recognized comment marker.
///
/// Supported markers: `//`, `#` (but not `#!` or `##`), `<!--` (with optional trailing `-->`).
fn strip_comment_marker(trimmed: &str) -> Option<&str> {
	if let Some(rest) = trimmed.strip_prefix("//") {
		return Some(rest.trim());
	}
	if trimmed.starts_with('#') && !trimmed.starts_with("#!") && !trimmed.starts_with("##") {
		return Some(trimmed[1..].trim());
	}
	if let Some(rest) = trimmed.strip_prefix("<!--") {
		let rest = rest.trim();
		let rest = rest.strip_suffix("-->").unwrap_or(rest);
		return Some(rest.trim());
	}
	None
}

/// Strips underscore separators from numeric literals in a string.
/// Removes `_` characters that are immediately preceded and followed by a hex digit
/// (0-9, a-f, A-F). This normalizes `1_000` to `1000` and `0xFF_FF` to `0xFFFF`.
fn strip_numeric_underscores(s: &str) -> String {
	let chars: Vec<char> = s.chars().collect();
	let mut result = String::with_capacity(s.len());
	for (i, &ch) in chars.iter().enumerate() {
		if ch == '_' && i > 0 && i + 1 < chars.len() {
			let prev = chars[i - 1];
			let next = chars[i + 1];
			if prev.is_ascii_hexdigit() && next.is_ascii_hexdigit() {
				continue; // skip this underscore
			}
		}
		result.push(ch);
	}
	result
}

/// Strips all whitespace characters from a string.
/// Used as a last-resort comparison in the Fuzzy tier for multi-line string resilience.
fn strip_all_ws(s: &str) -> String {
	s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Normalizes inline formatting tokens for fuzzy comparison.
/// Removes backticks and canonicalizes both single and double quotes to single quote.
fn normalize_inline_fuzzy(s: &str) -> String {
	s.chars()
		.filter(|c| *c != '`')
		.map(|c| if c == '"' { '\'' } else { c })
		.collect()
}

/// Represents a candidate match found during hunk position search.
struct CandidateMatch {
	idx: usize,
	tier: MatchTier,
	overhang_hl_indices: Vec<usize>,
	skipped_hl_indices: Vec<usize>,
	converted_to_add_indices: Vec<usize>,
	matched_orig_indices: Vec<(usize, usize)>,
	skipped_blank_orig_indices: Vec<usize>,

	/// Number of context/removal lines that matched without needing normalization or suffix.
	exact_ws_count: usize,

	/// Whether all non-blank matched lines have a uniform leading-whitespace delta.
	/// Used as a scoring boost at the Resilient tier.
	uniform_indent: bool,

	/// Number of adjacent context hints that matched (0, 1, or 2).
	/// Derived from checking original lines immediately before/after the matched region.
	adjacent_hint_matches: usize,
}

/// Checks whether one trimmed line is a suffix of the other.
/// Only applies when the shorter fragment is long enough to be meaningful,
/// preventing false positives from very short context lines.
fn suffix_match(orig_trimmed: &str, patch_trimmed: &str, case_insensitive: bool) -> bool {
	let orig_norm = if case_insensitive {
		normalize_ws(orig_trimmed).to_lowercase()
	} else {
		normalize_ws(orig_trimmed)
	};
	let patch_norm = if case_insensitive {
		normalize_ws(patch_trimmed).to_lowercase()
	} else {
		normalize_ws(patch_trimmed)
	};
	if patch_norm.len() >= SUFFIX_MATCH_MIN_LEN && orig_norm.ends_with(&patch_norm) {
		// Reject if the non-matching prefix is a comment marker (e.g., "// " or "# ").
		// This prevents "do something" from suffix-matching "// do something".
		let prefix = orig_norm[..orig_norm.len() - patch_norm.len()].trim();
		if !prefix.is_empty() && is_comment_marker_prefix(prefix) {
			return false;
		}
		return true;
	}
	if orig_norm.len() >= SUFFIX_MATCH_MIN_LEN && patch_norm.ends_with(&orig_norm) {
		let prefix = patch_norm[..patch_norm.len() - orig_norm.len()].trim();
		if !prefix.is_empty() && is_comment_marker_prefix(prefix) {
			return false;
		}
		return true;
	}
	false
}

/// Scores a candidate match. Higher is better.
/// Criteria:
///   - Prefer more exact whitespace matches (no normalization needed).
///   - Prefer match closest to the expected location (`search_from`).
fn score_candidate(candidate: &CandidateMatch, search_from: usize) -> (usize, isize) {
	let distance = match candidate.idx >= search_from {
		true => candidate.idx - search_from,
		false => search_from - candidate.idx,
	};
	// Primary: exact whitespace count (higher is better).
	// Secondary: adjacent hint matches (0-2, higher is better).
	// Tertiary: uniform indent bonus (1 if uniform, 0 otherwise).
	// Quaternary: negative distance (closer is better, so negate).
	let uniform_bonus: usize = if candidate.uniform_indent { 1 } else { 0 };
	let hint_bonus: usize = candidate.adjacent_hint_matches;
	(
		candidate.exact_ws_count,
		(hint_bonus as isize * 10_000) + (uniform_bonus as isize * 1000) - distance as isize,
	)
}

/// Checks whether an original line matches a patch line at the given tier.
///
/// - **Strict**: Character-for-character exact match. No trimming or normalization.
/// - **Resilient**: Trimmed comparison, normalized whitespace, and suffix match (case-sensitive).
/// - **Fuzzy**: Same as Resilient but all comparisons are case-insensitive.
fn line_matches(orig_line: &str, p_line: &str, tier: MatchTier) -> bool {
	match tier {
		MatchTier::Strict => orig_line == p_line,
		MatchTier::Resilient => {
			let orig_trimmed = orig_line.trim();
			let p_trimmed = p_line.trim();
			if orig_trimmed.is_empty() || p_trimmed.is_empty() {
				return orig_trimmed == p_trimmed;
			}
			orig_trimmed == p_trimmed
				|| normalize_ws(orig_trimmed) == normalize_ws(p_trimmed)
				|| (is_markdown_heading(orig_trimmed)
					&& is_markdown_heading(p_trimmed)
					&& normalize_ws(strip_markdown_heading(orig_trimmed))
						== normalize_ws(strip_markdown_heading(p_trimmed)))
				|| suffix_match(orig_trimmed, p_trimmed, false)
				|| {
					// Trailing semicolon/comma tolerance: strip a single trailing `;` or `,`
					// from both lines and re-compare. This handles common LLM formatting
					// differences in code without going fully fuzzy.
					let o_stripped = orig_trimmed.trim_end_matches([',', ';']);
					let p_stripped = p_trimmed.trim_end_matches([',', ';']);
					!o_stripped.is_empty()
						&& !p_stripped.is_empty()
						&& (o_stripped != orig_trimmed || p_stripped != p_trimmed)
						&& (o_stripped == p_stripped || normalize_ws(o_stripped) == normalize_ws(p_stripped))
				} || {
				// Comment-only line tolerance: when both lines are comment-only,
				// strip the comment marker and compare remaining content with
				// normalized whitespace. This handles minor wording/spacing
				// differences in comments without affecting non-comment lines.
				if let (Some(o_body), Some(p_body)) =
					(strip_comment_marker(orig_trimmed), strip_comment_marker(p_trimmed))
				{
					!o_body.is_empty() && !p_body.is_empty() && normalize_ws(o_body) == normalize_ws(p_body)
				} else {
					false
				}
			}
		}
		MatchTier::Fuzzy => {
			let o_t = orig_line.trim();
			let p_t = p_line.trim();
			if o_t.is_empty() || p_t.is_empty() {
				return o_t == p_t;
			}
			let o_l = o_t.to_lowercase();
			let p_l = p_t.to_lowercase();

			o_l == p_l
				|| normalize_ws(&o_l) == normalize_ws(&p_l)
				|| (is_markdown_heading(o_t)
					&& is_markdown_heading(p_t)
					&& normalize_ws(strip_markdown_heading(o_t)).to_lowercase()
						== normalize_ws(strip_markdown_heading(p_t)).to_lowercase())
				|| suffix_match(o_t, p_t, true)
				// Also check if they match ignoring backticks (common Markdown LLM variance)
				|| o_l.replace('`', "") == p_l.replace('`', "")
				|| normalize_ws(&o_l.replace('`', "")) == normalize_ws(&p_l.replace('`', ""))
				// Also check via full inline-format normalization (backticks + quote canonicalization)
				|| {
					let o_norm = normalize_inline_fuzzy(&o_l);
					let p_norm = normalize_inline_fuzzy(&p_l);
					!o_norm.trim().is_empty()
						&& !p_norm.trim().is_empty()
						&& (o_norm == p_norm || normalize_ws(&o_norm) == normalize_ws(&p_norm))
				}
				// Also check if they match ignoring trailing punctuation (common LLM error),
				// with quote normalization applied as well.
				|| o_l.trim_end_matches(|c: char| c.is_ascii_punctuation())
					== p_l.trim_end_matches(|c: char| c.is_ascii_punctuation())
				|| {
					let o_punct = normalize_inline_fuzzy(&o_l).trim_end_matches(|c: char| c.is_ascii_punctuation()).to_string();
					let p_punct = normalize_inline_fuzzy(&p_l).trim_end_matches(|c: char| c.is_ascii_punctuation()).to_string();
					!o_punct.trim().is_empty() && !p_punct.trim().is_empty() && o_punct == p_punct
				}
				// Also check if they match after stripping numeric literal underscores
				|| normalize_ws(&strip_numeric_underscores(&o_l))
					== normalize_ws(&strip_numeric_underscores(&p_l))
				// Last resort: strip ALL whitespace for multi-line string resilience.
				// This handles cases where the LLM reformats internal whitespace in
				// string literals or similar content.
				|| (!o_l.is_empty()
					&& strip_all_ws(&o_l) == strip_all_ws(&p_l)
					&& strip_all_ws(&o_l).len() >= 4)
		}
	}
}

/// Returns the number of leading whitespace characters (spaces and tabs) in a line.
fn leading_ws_len(line: &str) -> usize {
	line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Checks whether all non-blank matched pairs have a uniform leading-whitespace delta.
/// Returns `true` if the delta is the same for every pair (or if there are no non-blank pairs).
fn has_uniform_indent_delta(orig_lines: &[&str], hunk_lines: &[&str], matched_orig_indices: &[(usize, usize)]) -> bool {
	let mut delta: Option<isize> = None;

	for &(hl_idx, orig_idx) in matched_orig_indices {
		let p_line = if hunk_lines[hl_idx].len() > 1 {
			&hunk_lines[hl_idx][1..]
		} else {
			""
		};
		// Skip blank lines; they carry no indentation signal.
		if p_line.trim().is_empty() {
			continue;
		}
		let orig_ws = leading_ws_len(orig_lines[orig_idx]) as isize;
		let patch_ws = leading_ws_len(p_line) as isize;
		let d = orig_ws - patch_ws;
		match delta {
			None => delta = Some(d),
			Some(prev) if prev != d => return false,
			_ => {}
		}
	}

	true
}

/// Checks whether an original line at a given index matches a hint line,
/// using Resilient-tier matching for flexibility.
fn hint_line_matches(orig_lines: &[&str], orig_idx: usize, hint: &str) -> bool {
	if orig_idx >= orig_lines.len() {
		return false;
	}
	let orig_line = orig_lines[orig_idx];
	// Use Resilient matching for hint comparison (trimmed, normalized ws)
	line_matches(orig_line, hint, MatchTier::Resilient)
}

/// Computes the number of adjacent hint matches for a candidate.
fn compute_adjacent_hint_matches(
	orig_lines: &[&str],
	candidate_start: usize,
	candidate_old_count: usize,
	hints: &AdjacentHints<'_>,
) -> usize {
	let mut count = 0;

	// Check previous hint: the original line immediately before candidate start
	if let Some(prev_hint) = hints.prev_hint
		&& !prev_hint.trim().is_empty()
		&& candidate_start > 0
		&& hint_line_matches(orig_lines, candidate_start - 1, prev_hint)
	{
		count += 1;
	}

	// Check next hint: the original line immediately after the candidate's matched region
	if let Some(next_hint) = hints.next_hint
		&& !next_hint.trim().is_empty()
	{
		let after_idx = candidate_start + candidate_old_count;
		if hint_line_matches(orig_lines, after_idx, next_hint) {
			count += 1;
		}
	}

	count
}

/// Searches for candidate matches at a given tier, returning all found candidates.
fn search_candidates_for_tier(
	orig_lines: &[&str],
	hunk_lines: &[&str],
	search_from: usize,
	tier: MatchTier,
	hints: &AdjacentHints<'_>,
) -> Vec<CandidateMatch> {
	let mut candidates: Vec<CandidateMatch> = Vec::new();

	for i in 0..=orig_lines.len() {
		// -- Proximity Check: For lenient tiers, skip candidates that are too far
		// from the expected position (in either direction).
		let distance = i.abs_diff(search_from);
		let max_proximity = if search_from == 0 {
			5000
		} else {
			MAX_PROXIMITY_FOR_LENIENT
		};

		if tier > MatchTier::Strict && distance > max_proximity {
			continue;
		}

		let mut matches = true;
		let mut current_overhang = Vec::new();
		let current_skipped = Vec::new();
		let mut current_converted_to_add = Vec::new();
		let mut current_matches = Vec::new();
		let mut current_skipped_blanks_all = Vec::new();
		let mut current_exact_ws_count: usize = 0;
		let mut orig_off = 0; // offset in orig_lines from i

		for (hl_idx, hl_line) in hunk_lines.iter().enumerate() {
			if hl_line.starts_with('+') {
				continue;
			}

			let p_line = if hl_line.len() > 1 { &hl_line[1..] } else { "" };

			let mut target_idx = i + orig_off;

			// -- Blank line skipping for Resilient/Fuzzy tiers
			// This allows matching even when the original file has more blank lines than the LLM context.
			if tier >= MatchTier::Resilient && !p_line.trim().is_empty() {
				while target_idx < orig_lines.len() && orig_lines[target_idx].trim().is_empty() {
					current_skipped_blanks_all.push(target_idx);
					target_idx += 1;
					orig_off += 1;
				}
			}

			if p_line.trim().is_empty() {
				// If the patch has a blank line...
				if target_idx < orig_lines.len() && orig_lines[target_idx].trim().is_empty() {
					// ... and original has a blank line: Match.
					current_matches.push((hl_idx, target_idx));
					orig_off += 1;
				} else if target_idx >= orig_lines.len() {
					// ... and we're at/beyond EOF: convert to addition to preserve spacing.
					current_converted_to_add.push(hl_idx);
				} else {
					// ... and original doesn't have a blank line: skip this hunk line
					// without advancing the original offset. The LLM may have inserted
					// a cosmetic blank line for readability that doesn't exist in the
					// original. We convert it to an addition so it appears in the output
					// without disrupting alignment of subsequent context/removal lines.
					current_converted_to_add.push(hl_idx);
				}
			} else if target_idx < orig_lines.len() {
				let orig_line = orig_lines[target_idx];

				if line_matches(orig_line, p_line, tier) {
					// Track whether this was an exact whitespace match (no normalization needed)
					if orig_line == p_line {
						current_exact_ws_count += 1;
					}
					current_matches.push((hl_idx, target_idx));
					orig_off += 1;
				} else {
					matches = false;
					break;
				}
			} else {
				// Pattern goes beyond EOF: only allow if it's trailing context.
				// If it's a removal line, it's not a match.
				if hl_line.starts_with('-') {
					matches = false;
					break;
				}
				current_overhang.push(hl_idx);
			}
		}

		if matches && !current_matches.is_empty() {
			// -- Validation: Ensure at least one non-blank line matched in-file.
			// This prevents matching only cosmetic blank lines followed by EOF overhang.
			let significant_in_file_match_count = current_matches
				.iter()
				.filter(|(hl_idx, _)| !hunk_lines[*hl_idx].trim().is_empty())
				.count();

			if significant_in_file_match_count == 0 {
				continue;
			}

			// -- Validation: If we have overhang, ensure we have more in-file matches than overhang.
			// This prevents matching a single line near EOF and treating the rest of the hunk as overhang.
			if !current_overhang.is_empty() {
				// We require at least 2 in-file matches for any overhang,
				// and in-file matches must be greater than overhang.
				if significant_in_file_match_count < 2 || current_overhang.len() >= significant_in_file_match_count {
					continue;
				}
			}

			let uniform_indent = has_uniform_indent_delta(orig_lines, hunk_lines, &current_matches);

			// Compute old_count for this candidate to determine the matched region span.
			let candidate_old_count = {
				let mut oc: usize = 0;
				for (hl_idx, hl_line) in hunk_lines.iter().enumerate() {
					if current_overhang.contains(&hl_idx) || current_skipped.contains(&hl_idx) {
						continue;
					}
					if current_converted_to_add.contains(&hl_idx) {
						continue;
					}
					if hl_line.starts_with('+') {
						continue;
					}
					// context or removal line that matched in-file
					if current_matches.iter().any(|(h, _)| *h == hl_idx) {
						oc += 1;
					}
				}
				// Also count skipped blank orig lines
				oc += current_skipped_blanks_all.len();
				oc
			};

			let adjacent_hint_matches = compute_adjacent_hint_matches(orig_lines, i, candidate_old_count, hints);

			candidates.push(CandidateMatch {
				idx: i,
				tier,
				overhang_hl_indices: current_overhang,
				skipped_hl_indices: current_skipped,
				converted_to_add_indices: current_converted_to_add,
				matched_orig_indices: current_matches,
				skipped_blank_orig_indices: current_skipped_blanks_all,
				exact_ws_count: current_exact_ws_count,
				uniform_indent,
				adjacent_hint_matches,
			});
		}
	}

	candidates
}

fn compute_hunk_bounds(
	orig_lines: &[&str],
	hunk_lines: &[&str],
	search_from: usize,
	hints: &AdjacentHints<'_>,
) -> Result<HunkBounds> {
	// -- Empty original bootstrapping
	// When the original content is empty (or only blank lines), auto-convert all
	// context/removal lines to additions so that a FILE_PATCH against a non-existent
	// or empty file succeeds instead of failing to find context.
	let orig_is_empty = orig_lines.is_empty() || orig_lines.iter().all(|l| l.trim().is_empty());

	if orig_is_empty {
		let has_context_or_removal = hunk_lines.iter().any(|l| !l.starts_with('+'));
		// If there are context/removal lines, convert them all to additions.
		// If there are only addition lines, fall through to the normal append logic below.
		if has_context_or_removal {
			let mut final_hunk_lines = Vec::new();
			let mut new_count = 0;

			for hl in hunk_lines {
				if hl.starts_with('+') {
					final_hunk_lines.push(hl.to_string());
				} else {
					// Convert context (' ') or removal ('-') to addition ('+')
					let content = if hl.len() > 1 { &hl[1..] } else { "" };
					final_hunk_lines.push(format!("+{content}"));
				}
				new_count += 1;
			}

			return Ok(HunkBounds {
				old_start: 1,
				old_count: 0,
				new_count,
				final_hunk_lines,
				tier: None,
			});
		}
	}

	// -- Pre-check for pattern existence
	let context_lines_count = hunk_lines.iter().filter(|l| !l.starts_with('+')).count();

	// -- If no context/removal lines, assume append to end
	if context_lines_count == 0 {
		// Count trailing blank lines in the original
		let trailing_blank_count = orig_lines.iter().rev().take_while(|l| l.trim().is_empty()).count();

		// Count leading blank addition lines in the hunk
		let leading_blank_add_count = hunk_lines
			.iter()
			.take_while(|l| {
				let content = if l.len() > 1 { &l[1..] } else { "" };
				l.starts_with('+') && content.trim().is_empty()
			})
			.count();

		// Overlap: convert leading blank additions into context lines that anchor
		// against the existing trailing blanks, preventing duplication.
		let overlap = trailing_blank_count.min(leading_blank_add_count);

		// Count trailing blank addition lines in the hunk
		let trailing_blank_add_count = hunk_lines
			.iter()
			.rev()
			.take_while(|l| {
				let content = if l.len() > 1 { &l[1..] } else { "" };
				l.starts_with('+') && content.trim().is_empty()
			})
			.count();

		// Trailing overlap: remaining original trailing blanks not consumed by leading overlap
		// can absorb trailing blank additions to prevent duplication.
		let remaining_trailing_blanks = trailing_blank_count.saturating_sub(overlap);
		let trailing_overlap = remaining_trailing_blanks.min(trailing_blank_add_count);

		let mut final_hunk_lines = Vec::new();
		let mut old_count = 0;
		let mut new_count = 0;
		let hunk_len = hunk_lines.len();

		for (i, hl) in hunk_lines.iter().enumerate() {
			if i < overlap {
				// Convert this leading blank addition to a context line
				final_hunk_lines.push(" ".to_string());
				old_count += 1;
				new_count += 1;
			} else if trailing_overlap > 0 && i >= hunk_len - trailing_overlap {
				// Convert trailing blank addition to a context line
				final_hunk_lines.push(" ".to_string());
				old_count += 1;
				new_count += 1;
			} else {
				final_hunk_lines.push(hl.to_string());
				new_count += 1;
			}
		}

		let old_start = if overlap > 0 {
			// Anchor at the first trailing blank line we're using as context
			orig_lines.len() - (overlap + trailing_overlap).max(trailing_blank_count).min(trailing_blank_count) + 1
		} else if trailing_overlap > 0 {
			// Anchor at the trailing blank lines used as context
			orig_lines.len() - trailing_overlap + 1
		} else {
			orig_lines.len() + 1
		};

		return Ok(HunkBounds {
			old_start,
			old_count,
			new_count,
			final_hunk_lines,
			tier: None,
		});
	}

	// -- Tiered search: stop at the first tier that yields candidates
	let tiers = [MatchTier::Strict, MatchTier::Resilient, MatchTier::Fuzzy];
	let mut candidates: Vec<CandidateMatch> = Vec::new();

	for tier in tiers {
		candidates = search_candidates_for_tier(orig_lines, hunk_lines, search_from, tier, hints);
		if !candidates.is_empty() {
			break;
		}
	}

	// -- Select the best candidate by score
	let best = candidates.into_iter().max_by(|a, b| {
		let sa = score_candidate(a, search_from);
		let sb = score_candidate(b, search_from);
		sa.cmp(&sb)
	});

	let best = best.ok_or_else(|| {
		let context_pattern: Vec<String> = hunk_lines
			.iter()
			.filter(|l| l.starts_with(' ') || l.starts_with('-'))
			.map(|l| if l.is_empty() { "" } else { &l[1..] }.to_string())
			.collect();

		Error::patch_completion(format!(
			"Could not find patch context in original file (starting search from line {})\nContext lines:\n{}",
			search_from + 1,
			context_pattern.join("\n")
		))
	})?;

	let idx = best.idx;
	let tier = best.tier;
	let overhang_hl_indices = best.overhang_hl_indices;
	let skipped_hl_indices = best.skipped_hl_indices;
	let converted_to_add_indices = best.converted_to_add_indices;
	let matched_orig_indices = best.matched_orig_indices;
	let skipped_blank_orig_indices = best.skipped_blank_orig_indices;

	// -- Reconstruct final hunk lines and calculate counts
	let mut final_hunk_lines = Vec::new();
	let mut old_count = 0;
	let mut new_count = 0;
	let mut last_orig_idx: Option<usize> = None;

	for (hl_idx, line) in hunk_lines.iter().enumerate() {
		if overhang_hl_indices.contains(&hl_idx) || skipped_hl_indices.contains(&hl_idx) {
			continue;
		}

		// Blank context lines at EOF are converted to addition lines to preserve spacing
		if converted_to_add_indices.contains(&hl_idx) {
			final_hunk_lines.push("+".to_string());
			new_count += 1;
			continue;
		}

		// If this was a matched context/removal line, use the original file content for the hunk.
		// This ensures that the generated patch matches the file exactly (needed for diffy).
		if let Some((_, orig_idx)) = matched_orig_indices.iter().find(|(h_idx, _)| *h_idx == hl_idx) {
			// Emit skipped blanks before this match to maintain alignment
			for &s_idx in &skipped_blank_orig_indices {
				if s_idx < *orig_idx && (last_orig_idx.is_none() || s_idx > last_orig_idx.unwrap()) {
					final_hunk_lines.push(format!(" {}", orig_lines[s_idx]));
					old_count += 1;
					new_count += 1;
				}
			}

			let orig_content = orig_lines[*orig_idx];
			let prefix = if line.starts_with('-') { '-' } else { ' ' };
			final_hunk_lines.push(format!("{prefix}{orig_content}"));

			if prefix == '-' {
				old_count += 1;
			} else {
				old_count += 1;
				new_count += 1;
			}
			last_orig_idx = Some(*orig_idx);
		}
		// If it's an addition line, use it as is
		else if line.starts_with('+') {
			final_hunk_lines.push(line.to_string());
			new_count += 1;
		}
	}

	Ok(HunkBounds {
		old_start: idx + 1,
		old_count,
		new_count,
		final_hunk_lines,
		tier: Some(tier),
	})
}

// endregion: --- Support

// region:    --- Tests

#[cfg(test)]
#[path = "patch_completer_tests.rs"]
mod tests;

// endregion: --- Tests

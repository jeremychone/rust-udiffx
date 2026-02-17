use crate::{Error, Result};
use std::borrow::Cow;

// region:    --- Types

/// Maximum lines to search away from the expected position for lenient (Resilient/Fuzzy) matches.
/// This prevents a hunk from "drifting" too far and causing subsequent hunks to fail.
const MAX_PROXIMITY_FOR_LENIENT: usize = 100;

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

// endregion: --- Types

/// Collapses runs of whitespace into a single space for normalized comparison.
fn normalize_ws(s: &str) -> String {
	s.split_whitespace().collect::<Vec<_>>().join(" ")
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

	let mut lines = patch_raw.lines().peekable();
	let mut completed_patch = String::new();
	let orig_lines: Vec<&str> = original_content.lines().collect();
	let mut total_delta: isize = 0;
	let mut search_from: usize = 0;
	let mut max_tier: Option<MatchTier> = None;

	while let Some(line) = lines.next() {
		let trimmed = line.trim();

		// If it's a hunk header (recompute even if complete)
		if trimmed.starts_with("@@") {
			let mut hunk_lines = Vec::new();
			while let Some(next_line) = lines.peek() {
				let next_trimmed = next_line.trim();
				if next_trimmed.starts_with("@@") {
					break;
				}
				hunk_lines.push(lines.next().unwrap());
			}

			// Compute line numbers
			let hunk_bounds = compute_hunk_bounds(&orig_lines, &hunk_lines, search_from)?;
			let old_start = hunk_bounds.old_start;
			let old_count = hunk_bounds.old_count;
			let new_count = hunk_bounds.new_count;
			let final_hunk_lines = hunk_bounds.final_hunk_lines;
			let new_start = (old_start as isize + total_delta) as usize;

			if let Some(t) = hunk_bounds.tier {
				max_tier = Some(max_tier.map(|m| m.max(t)).unwrap_or(t));
			}

			// Update state for next hunk
			search_from = old_start + old_count - 1;
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
		} else {
			completed_patch.push_str(line);
			completed_patch.push('\n');
		}
	}

	Ok((completed_patch, max_tier))
}

// region:    --- Support

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

/// Represents a candidate match found during hunk position search.
struct CandidateMatch {
	idx: usize,
	tier: MatchTier,
	overhang_hl_indices: Vec<usize>,
	skipped_hl_indices: Vec<usize>,
	converted_to_add_indices: Vec<usize>,
	matched_orig_lines: Vec<(usize, String)>,

	/// Number of context/removal lines that matched without needing normalization or suffix.
	exact_ws_count: usize,
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
		return true;
	}
	if orig_norm.len() >= SUFFIX_MATCH_MIN_LEN && patch_norm.ends_with(&orig_norm) {
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
	// Primary: exact whitespace count (higher is better)
	// Secondary: negative distance (closer is better, so negate)
	(candidate.exact_ws_count, -(distance as isize))
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
				// Also check if they match ignoring trailing punctuation (common LLM error)
				|| o_l.trim_end_matches(|c: char| c.is_ascii_punctuation())
					== p_l.trim_end_matches(|c: char| c.is_ascii_punctuation())
		}
	}
}

/// Searches for candidate matches at a given tier, returning all found candidates.
fn search_candidates_for_tier(
	orig_lines: &[&str],
	hunk_lines: &[&str],
	search_from: usize,
	tier: MatchTier,
) -> Vec<CandidateMatch> {
	let mut candidates: Vec<CandidateMatch> = Vec::new();

	for i in search_from..=orig_lines.len() {
		// -- Proximity Check: If we've drifted too far in a lenient tier, skip this candidate.
		let distance = match i >= search_from {
			true => i - search_from,
			false => search_from - i,
		};
		if tier > MatchTier::Strict && distance > MAX_PROXIMITY_FOR_LENIENT {
			continue;
		}

		let mut matches = true;
		let mut current_overhang = Vec::new();
		let current_skipped = Vec::new();
		let mut current_converted_to_add = Vec::new();
		let mut current_matches = Vec::new();
		let mut current_exact_ws_count: usize = 0;
		let mut orig_off = 0; // offset in orig_lines from i

		for (hl_idx, hl_line) in hunk_lines.iter().enumerate() {
			if hl_line.starts_with('+') {
				continue;
			}

			let p_line = if hl_line.len() > 1 { &hl_line[1..] } else { "" };

			let target_idx = i + orig_off;

			if p_line.trim().is_empty() {
				// If the patch has a blank line...
				if target_idx < orig_lines.len() && orig_lines[target_idx].trim().is_empty() {
					// ... and original has a blank line: Match.
					current_matches.push((hl_idx, orig_lines[target_idx].to_string()));
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
					current_matches.push((hl_idx, orig_line.to_string()));
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

			candidates.push(CandidateMatch {
				idx: i,
				tier,
				overhang_hl_indices: current_overhang,
				skipped_hl_indices: current_skipped,
				converted_to_add_indices: current_converted_to_add,
				matched_orig_lines: current_matches,
				exact_ws_count: current_exact_ws_count,
			});
		}
	}

	candidates
}

fn compute_hunk_bounds(orig_lines: &[&str], hunk_lines: &[&str], search_from: usize) -> Result<HunkBounds> {
	// -- Pre-check for pattern existence
	let context_lines_count = hunk_lines.iter().filter(|l| !l.starts_with('+')).count();

	// -- If no context/removal lines, assume append to end
	if context_lines_count == 0 {
		let added_count = hunk_lines.len();
		let final_hunk_lines = hunk_lines.iter().map(|s| s.to_string()).collect();
		return Ok(HunkBounds {
			old_start: orig_lines.len() + 1,
			old_count: 0,
			new_count: added_count,
			final_hunk_lines,
			tier: None,
		});
	}

	// -- Tiered search: stop at the first tier that yields candidates
	let tiers = [MatchTier::Strict, MatchTier::Resilient, MatchTier::Fuzzy];
	let mut candidates: Vec<CandidateMatch> = Vec::new();

	for tier in tiers {
		candidates = search_candidates_for_tier(orig_lines, hunk_lines, search_from, tier);
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
	let matched_orig_lines = best.matched_orig_lines;

	// -- Reconstruct final hunk lines and calculate counts
	let mut final_hunk_lines = Vec::new();
	let mut old_count = 0;
	let mut new_count = 0;

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
		if let Some((_, orig_content)) = matched_orig_lines.iter().find(|(h_idx, _)| *h_idx == hl_idx) {
			let prefix = if line.starts_with('-') { '-' } else { ' ' };
			final_hunk_lines.push(format!("{prefix}{orig_content}"));

			if prefix == '-' {
				old_count += 1;
			} else {
				old_count += 1;
				new_count += 1;
			}
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
mod tests {
	type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

	use super::*;

	#[test]
	fn test_patch_completer_complete_simple() -> Result<()> {
		// -- Setup & Fixtures
		let original = "line 1\nline 2\nline 3\n";
		let patch = "@@\n line 2\n+line 2.5\n line 3\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -2,2 +2,3 @@"));
		assert!(completed.contains(" line 2\n+line 2.5\n line 3"));

		Ok(())
	}

	#[test]
	fn test_patch_completer_complete_partial_suffix() -> Result<()> {
		// -- Setup & Fixtures
		let original = "This is a long line with some suffix.\nAnother line.\n";
		// The LLM only provides the suffix as context
		let patch = "@@\n some suffix.\n+New line after suffix.\n Another line.\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -1,2 +1,3 @@"));
		assert!(completed.contains(" some suffix.\n+New line after suffix.\n Another line."));

		Ok(())
	}

	#[test]
	fn test_patch_completer_complete_whitespace_mismatch() -> Result<()> {
		// -- Setup & Fixtures
		let original = "    Indented line\n";
		// LLM might not preserve indentation in context lines
		let patch = "@@\n Indented line\n+    New indented line\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -1,1 +1,2 @@"));

		Ok(())
	}

	/// Verifies that a short substring like "x" does not false-positive match a longer line.
	/// The old `contains`-based logic would have matched "x" against "box of foxes".
	#[test]
	fn test_patch_completer_complete_no_false_positive_contains_short() -> Result<()> {
		// -- Setup & Fixtures
		let original = "box of foxes\nthe letter x\nanother line\n";
		// Context line "x" should only match "the letter x", not "box of foxes"
		let patch = "@@\n the letter x\n+inserted after x\n another line\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// Should match starting at line 2 ("the letter x"), not line 1 ("box of foxes")
		assert!(completed.contains("@@ -2,2 +2,3 @@"));
		assert!(completed.contains("+inserted after x"));

		Ok(())
	}

	/// Verifies that a line which is a substring of another does not false-positive match.
	/// E.g., context "name" should not match original "namespace" via contains.
	#[test]
	fn test_patch_completer_complete_no_false_positive_contains_substring() -> Result<()> {
		// -- Setup & Fixtures
		let original = "namespace\nname\nvalue\n";
		// Context "name" should match line 2, not line 1 ("namespace")
		let patch = "@@\n name\n+new name line\n value\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -2,2 +2,3 @@"));
		assert!(completed.contains("+new name line"));

		Ok(())
	}

	/// Verifies normalized whitespace equality works (multiple spaces collapsed).
	#[test]
	fn test_patch_completer_complete_normalized_ws_equality() -> Result<()> {
		// -- Setup & Fixtures
		let original = "fn   main()  {\n    println!(\"hello\");\n}\n";
		// LLM collapses multiple spaces to single space
		let patch = "@@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"world\");\n }\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -1,3 +1,3 @@"));
		assert!(completed.contains("+    println!(\"world\");"));

		Ok(())
	}

	/// Verifies that when duplicate patterns exist, the scoring system prefers
	/// the match with exact whitespace over a normalized match.
	#[test]
	fn test_patch_completer_complete_scoring_exact_ws_preferred() -> Result<()> {
		// -- Setup & Fixtures
		// Two blocks that match trimmed, but only the second has exact whitespace.
		let original = "\
    fn hello() {
        println!(\"hello\");
    }
fn hello() {
    println!(\"hello\");
}
";
		// Patch context uses no leading indentation, matching the second block exactly.
		let patch = "@@\n fn hello() {\n-    println!(\"hello\");\n+    println!(\"world\");\n }\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// Should match the second block (line 4), not the first (line 1).
		assert!(completed.contains("@@ -4,3 +4,3 @@"));
		assert!(completed.contains("+    println!(\"world\");"));

		Ok(())
	}

	/// Verifies that when two identical blocks exist, the match closest to
	/// search_from (i.e., the first one) is preferred.
	#[test]
	fn test_patch_completer_complete_scoring_proximity_preferred() -> Result<()> {
		// -- Setup & Fixtures
		// Two identical blocks; scoring should prefer the first (closer to search_from=0).
		let original = "\
fn greet() {
    println!(\"hi\");
}
fn other() {}
fn greet() {
    println!(\"hi\");
}
";
		let patch = "@@\n fn greet() {\n-    println!(\"hi\");\n+    println!(\"hey\");\n }\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// Both blocks are identical (same exact_ws_count), so proximity wins: line 1.
		assert!(completed.contains("@@ -1,3 +1,3 @@"));
		assert!(completed.contains("+    println!(\"hey\");"));

		Ok(())
	}

	/// Verifies that a blank context line in the patch that doesn't match a non-blank
	/// original line causes a match failure at that position, preventing alignment drift.
	#[test]
	fn test_patch_completer_complete_blank_context_no_skip() -> Result<()> {
		// -- Setup & Fixtures
		// Original has no blank line between line 2 and line 3.
		let original = "line 1\nline 2\nline 3\nline 4\n";
		// Patch has a blank context line between "line 2" and "line 3" that doesn't exist
		// in the original. This should NOT silently skip and cause drift.
		let patch = "@@\n line 2\n \n-line 3\n+line 3 modified\n line 4\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// The unmatched blank context line is converted to an addition, and the rest
		// of the hunk aligns correctly without drift.
		assert!(completed.contains("@@ -2,3 +2,4 @@"));
		assert!(completed.contains("+line 3 modified"));
		// "line 3" should be a removal line (not misaligned)
		assert!(completed.contains("-line 3\n"));

		Ok(())
	}

	/// Verifies that blank context lines match correctly when the original also has
	/// blank lines in the corresponding positions.
	#[test]
	fn test_patch_completer_complete_blank_context_matches_blank_original() -> Result<()> {
		// -- Setup & Fixtures
		let original = "line 1\nline 2\n\nline 4\nline 5\n";
		// Blank context line aligns with the blank line in original (line 3).
		let patch = "@@\n line 2\n \n-line 4\n+line 4 modified\n line 5\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -2,4 +2,4 @@"));
		assert!(completed.contains("+line 4 modified"));

		Ok(())
	}

	/// Verifies that when a blank context line doesn't match at one position,
	/// the search continues and finds the correct position where it does match.
	#[test]
	fn test_patch_completer_complete_blank_context_finds_correct_position() -> Result<()> {
		// -- Setup & Fixtures
		// First "line A" is followed by non-blank "line B", second "line A" is followed by blank.
		let original = "line A\nline B\nline C\nline A\n\nline D\n";
		// Patch expects blank line after "line A", so it should match the second occurrence.
		let patch = "@@\n line A\n \n-line D\n+line D modified\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// Should match the second "line A" at line 4, not the first at line 1.
		assert!(completed.contains("@@ -4,3 +4,3 @@"));
		assert!(completed.contains("+line D modified"));

		Ok(())
	}

	/// Verifies that when a strict match exists, it is preferred over a resilient match
	/// at a different position.
	#[test]
	fn test_patch_completer_complete_strict_match_preferred() -> Result<()> {
		// -- Setup & Fixtures
		// Line 1 has extra leading spaces (only matches via trimmed/resilient).
		// Line 4 matches strictly (exact same text as patch context).
		let original = "\
    fn do_work() {
    old_call();
    }
fn do_work() {
    old_call();
}
";
		// Patch context has no leading indentation, matching the second block strictly.
		let patch = "@@\n fn do_work() {\n-    old_call();\n+    new_call();\n }\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// Should match the second block (line 4) via strict, not the first (line 1) via resilient.
		assert!(completed.contains("@@ -4,3 +4,3 @@"));
		assert!(completed.contains("+    new_call();"));

		Ok(())
	}

	/// Verifies that a casing mismatch in context lines is resolved by the fuzzy tier.
	#[test]
	fn test_patch_completer_complete_case_insensitive_fallback() -> Result<()> {
		// -- Setup & Fixtures
		let original = "## Section Title\nSome content here.\nMore content.\n";
		// Patch context uses different casing ("section title" vs "Section Title").
		let patch = "@@\n ## section title\n-Some content here.\n+Replaced content here.\n More content.\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -1,3 +1,3 @@"));
		assert!(completed.contains("+Replaced content here."));

		Ok(())
	}

	/// Verifies that when a resilient match exists (whitespace difference), fuzzy is not needed.
	/// Indirectly confirmed by the correct match position and successful patch application.
	#[test]
	fn test_patch_completer_complete_fuzzy_not_used_when_resilient_matches() -> Result<()> {
		// -- Setup & Fixtures
		// Original has extra spaces; patch context has single spaces.
		// This should match at resilient tier (whitespace normalization), not fuzzy.
		let original = "fn   example()  {\n    let x = 1;\n    let y = 2;\n}\n";
		let patch = "@@\n fn example() {\n-    let x = 1;\n+    let x = 42;\n     let y = 2;\n }\n";

		// -- Exec
		let (completed, _) = complete(original, patch)?;

		// -- Check
		// Should match at line 1 via resilient tier (normalized whitespace).
		assert!(completed.contains("@@ -1,4 +1,4 @@"));
		assert!(completed.contains("+    let x = 42;"));

		Ok(())
	}
}

// endregion: --- Tests

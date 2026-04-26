use super::MAX_PROXIMITY_FOR_LENIENT;
use super::matchers::{has_uniform_indent_delta, line_matches, score_candidate};
use super::parse::{
	collect_raw_hunks, collect_raw_hunks_sanitized, is_wrapper_meta_line, sanitize_wrapper_meta_lines,
	validate_and_parse_tilde_ranges,
};
use super::types::{AdjacentHints, CandidateMatch, HunkBounds, MatchTier, TildeRange};
use crate::{Error, Result};
use std::borrow::Cow;

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

	// -- If actionable hunks were found but recognized wrapper/meta lines are present,
	// build a sanitized hunk view as a resilience path. This prevents wrapper artefacts
	// like `*** End Patch` from remaining inside a collected hunk body and breaking
	// later matching, while still preserving strict literal behavior when the raw view
	// already matches successfully.
	let sanitized_raw_hunks = if patch_raw.lines().any(|line| is_wrapper_meta_line(line.trim())) {
		Some(collect_raw_hunks_sanitized(&patch_raw))
	} else {
		None
	};

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

	let hunk_count = raw_hunks.len();
	for hunk_idx in 0..hunk_count {
		let raw_hints = build_adjacent_hints(&raw_hunks, hunk_idx);
		let raw_hunk_lines = &raw_hunks[hunk_idx];

		let hunk_bounds = match compute_hunk_bounds(&orig_lines, raw_hunk_lines, search_from, &raw_hints) {
			Ok(bounds) => bounds,
			Err(raw_err) => {
				let Some(sanitized_raw_hunks) = &sanitized_raw_hunks else {
					return Err(raw_err);
				};
				if sanitized_raw_hunks.len() != hunk_count {
					return Err(raw_err);
				}

				let sanitized_hunk_lines = &sanitized_raw_hunks[hunk_idx];
				let sanitized_hints = build_adjacent_hints(sanitized_raw_hunks, hunk_idx);
				match compute_hunk_bounds(&orig_lines, sanitized_hunk_lines, search_from, &sanitized_hints) {
					Ok(bounds) => bounds,
					Err(_) => return Err(raw_err),
				}
			}
		};
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
fn estimate_hunk_position(orig_lines: &[&str], hunk_lines: &[&str]) -> Option<usize> {
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
	for p in positions.iter().flatten() {
		if let Some(lp) = last_pos
			&& *p < lp
		{
			is_ordered = false;
			break;
		}
		last_pos = Some(*p);
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

/// Expands tilde ranges in hunk lines and orig_lines, producing the final set of
/// removal lines for the expanded range. This is called during `compute_hunk_bounds`
/// after the initial context/removal matching has located the hunk position.
///
/// Returns the expanded hunk lines (with `~` replaced by the intermediate removal lines)
/// and updates old_count/new_count accordingly.
fn expand_tilde_ranges(
	orig_lines: &[&str],
	hunk_lines: &[&str],
	tilde_ranges: &[TildeRange],
	matched_orig_indices: &[(usize, usize)],
) -> Result<Vec<String>> {
	if tilde_ranges.is_empty() {
		return Ok(hunk_lines.iter().map(|l| l.to_string()).collect());
	}

	let mut expanded: Vec<String> = Vec::new();
	let mut skip_set: std::collections::HashSet<usize> = std::collections::HashSet::new();

	// For each tilde range, record the hl_indices that are part of the tilde marker itself
	for range in tilde_ranges {
		skip_set.insert(range.tilde_hl_index);
	}

	let mut hl_idx = 0;
	while hl_idx < hunk_lines.len() {
		// Check if this is a tilde line
		if let Some(range) = tilde_ranges.iter().find(|r| r.tilde_hl_index == hl_idx) {
			// Find the last matched orig index from the top anchors
			let last_top_anchor_hl = *range.top_anchor_hl_indices.last().unwrap();
			let last_top_orig_idx = matched_orig_indices
				.iter()
				.find(|(h, _)| *h == last_top_anchor_hl)
				.map(|(_, o)| *o)
				.ok_or_else(|| {
					Error::patch_completion(
						"Tilde range-remove: could not find original index for last top anchor line",
					)
				})?;

			// Find the first matched orig index from the bottom anchors
			let first_bottom_anchor_hl = *range.bottom_anchor_hl_indices.first().unwrap();
			let first_bottom_orig_idx = matched_orig_indices
				.iter()
				.find(|(h, _)| *h == first_bottom_anchor_hl)
				.map(|(_, o)| *o)
				.ok_or_else(|| {
					Error::patch_completion(
						"Tilde range-remove: could not find original index for first bottom anchor line",
					)
				})?;

			// Emit removal lines for all original lines between top and bottom anchors
			// (exclusive of the anchors themselves, which are already in hunk_lines as `-` lines)
			let tmp = (last_top_orig_idx + 1)..first_bottom_orig_idx;
			for orig_idx in tmp {
				expanded.push(format!("-{}", orig_lines[orig_idx]));
			}

			hl_idx += 1;
			continue;
		}

		expanded.push(hunk_lines[hl_idx].to_string());
		hl_idx += 1;
	}

	Ok(expanded)
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

	// Pre-check: does this hunk contain tilde ranges?
	let has_tilde = hunk_lines.iter().any(|l| l.trim() == "~");

	// Parse tilde ranges (validation already done in compute_hunk_bounds, so unwrap is safe here
	// for the purpose of candidate search; but we handle parse failure gracefully).
	let tilde_ranges = if has_tilde {
		validate_and_parse_tilde_ranges(hunk_lines).unwrap_or_default()
	} else {
		Vec::new()
	};

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
			if hl_line.starts_with('+') || hl_line.trim() == "~" {
				// Skip addition lines and tilde markers during initial matching.
				// Tilde expansion happens later in compute_hunk_bounds.
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

			// -- Tilde range handling: if this removal line is a bottom anchor of a tilde range,
			// we need to search forward in orig_lines to find it (skipping intermediate lines).
			let is_bottom_anchor = tilde_ranges.iter().any(|r| r.bottom_anchor_hl_indices.contains(&hl_idx));

			let is_first_bottom_anchor =
				tilde_ranges.iter().any(|r| r.bottom_anchor_hl_indices.first() == Some(&hl_idx));

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

				if is_first_bottom_anchor {
					// For the first bottom anchor of a tilde range, search forward
					// from current position to find the matching line.
					let mut found = false;
					for search_idx in target_idx..orig_lines.len() {
						if line_matches(orig_lines[search_idx], p_line, tier) {
							// Check that remaining bottom anchors also match consecutively
							let range = tilde_ranges
								.iter()
								.find(|r| r.bottom_anchor_hl_indices.first() == Some(&hl_idx))
								.unwrap();
							let mut all_match = true;
							for (offset, &ba_hl_idx) in range.bottom_anchor_hl_indices.iter().enumerate() {
								let ba_orig_idx = search_idx + offset;
								if ba_orig_idx >= orig_lines.len() {
									all_match = false;
									break;
								}
								let ba_line = if hunk_lines[ba_hl_idx].len() > 1 {
									&hunk_lines[ba_hl_idx][1..]
								} else {
									""
								};
								if !line_matches(orig_lines[ba_orig_idx], ba_line, tier) {
									all_match = false;
									break;
								}
							}
							if all_match {
								// Account for skipped intermediate lines in orig_off
								let skipped = search_idx - target_idx;
								orig_off += skipped;
								if orig_lines[search_idx] == p_line {
									current_exact_ws_count += 1;
								}
								current_matches.push((hl_idx, search_idx));
								orig_off += 1;
								found = true;
								break;
							}
						}
					}
					if !found {
						matches = false;
						break;
					}
				} else if is_bottom_anchor {
					// Non-first bottom anchor: already verified consecutively when
					// the first bottom anchor was matched. Record match and advance.
					let target = i + orig_off;
					if target < orig_lines.len() && line_matches(orig_lines[target], p_line, tier) {
						if orig_lines[target] == p_line {
							current_exact_ws_count += 1;
						}
						current_matches.push((hl_idx, target));
						orig_off += 1;
					} else {
						matches = false;
						break;
					}
				} else if line_matches(orig_line, p_line, tier) {
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
					if hl_line.trim() == "~" {
						continue;
					}
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

				// Also count intermediate lines consumed by tilde ranges
				for range in &tilde_ranges {
					let last_top = range
						.top_anchor_hl_indices
						.last()
						.and_then(|h| current_matches.iter().find(|(hh, _)| hh == h).map(|(_, o)| *o));

					let first_bottom = range
						.bottom_anchor_hl_indices
						.first()
						.and_then(|h| current_matches.iter().find(|(hh, _)| hh == h).map(|(_, o)| *o));

					if let Some(gap) = last_top.zip(first_bottom).and_then(|(last_top, first_bottom)| {
						(first_bottom > last_top + 1).then_some(first_bottom - last_top - 1)
					}) {
						oc += gap;
					}
				}

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
	// -- Validate tilde ranges before any matching
	let tilde_ranges = validate_and_parse_tilde_ranges(hunk_lines)?;

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
			.filter(|l| l.starts_with(' ') || l.starts_with('-') || l.trim() == "~")
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

	// -- Expand tilde ranges if present
	let expanded_hunk_lines = if !tilde_ranges.is_empty() {
		let expanded = expand_tilde_ranges(orig_lines, hunk_lines, &tilde_ranges, &matched_orig_indices)?;
		// Re-parse matched_orig_indices for the expanded lines is not needed;
		// we handle the expanded lines directly below.
		Some(expanded)
	} else {
		None
	};

	// If tilde expansion happened, we need to recompute hunk bounds from the expanded lines.
	// The expanded lines have all `~` replaced with explicit `-` lines from the original.
	if let Some(ref expanded_lines) = expanded_hunk_lines {
		// We need to rebuild the hunk from scratch with the expanded lines.
		// The position (idx) is already determined. We walk the expanded lines
		// and reconstruct using orig_lines starting at idx.
		let mut final_hunk_lines = Vec::new();
		let mut old_count = 0;
		let mut new_count = 0;
		let mut orig_off = 0;

		for line in expanded_lines.iter() {
			if line.starts_with('+') {
				final_hunk_lines.push(line.clone());
				new_count += 1;
			} else if line.starts_with('-') {
				let target = idx + orig_off;
				if target < orig_lines.len() {
					final_hunk_lines.push(format!("-{}", orig_lines[target]));
					old_count += 1;
					orig_off += 1;
				}
			} else {
				// Context line (starts with ' ')
				let target = idx + orig_off;
				if target < orig_lines.len() {
					final_hunk_lines.push(format!(" {}", orig_lines[target]));
					old_count += 1;
					new_count += 1;
					orig_off += 1;
				}
			}
		}

		return Ok(HunkBounds {
			old_start: idx + 1,
			old_count,
			new_count,
			final_hunk_lines,
			tier: Some(tier),
		});
	}

	// -- Reconstruct final hunk lines and calculate counts (non-tilde path)
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

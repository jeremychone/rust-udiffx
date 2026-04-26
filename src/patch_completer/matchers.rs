use super::SUFFIX_MATCH_MIN_LEN;
use super::types::{CandidateMatch, MatchTier};

/// Collapses runs of whitespace into a single space for normalized comparison.
fn normalize_ws(s: &str) -> String {
	s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Checks if a trimmed line is a Markdown heading.
fn is_markdown_heading(s: &str) -> bool {
	s.starts_with('#')
}

/// Strips the leading `#` characters and subsequent whitespace from a Markdown heading.
fn strip_markdown_heading(s: &str) -> &str {
	s.trim_start_matches('#').trim_start()
}

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
pub(super) fn score_candidate(candidate: &CandidateMatch, search_from: usize) -> (usize, isize) {
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
pub(super) fn line_matches(orig_line: &str, p_line: &str, tier: MatchTier) -> bool {
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
pub(super) fn has_uniform_indent_delta(orig_lines: &[&str], hunk_lines: &[&str], matched_orig_indices: &[(usize, usize)]) -> bool {
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

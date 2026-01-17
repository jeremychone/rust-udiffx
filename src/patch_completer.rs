use crate::{Error, Result};

/// Completes a patch by filling in missing line numbers in hunk headers.
/// If a hunk starts with just `@@` (optionally with whitespace), this function
/// searches for the context in the original content and computes the line numbers.
pub fn complete(original_content: &str, patch_raw: &str) -> Result<String> {
	let mut lines = patch_raw.lines().peekable();
	let mut completed_patch = String::new();
	let orig_lines: Vec<&str> = original_content.lines().collect();
	let mut total_delta: isize = 0;
	let mut search_from: usize = 0;

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
			let (old_start, old_count, new_count, final_hunk_lines) =
				compute_hunk_bounds(&orig_lines, &hunk_lines, search_from)?;
			let new_start = (old_start as isize + total_delta) as usize;

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

	Ok(completed_patch)
}

// region:    --- Support

fn compute_hunk_bounds(
	orig_lines: &[&str],
	hunk_lines: &[&str],
	search_from: usize,
) -> Result<(usize, usize, usize, Vec<String>)> {
	// -- Pre-check for pattern existence
	let context_lines_count = hunk_lines.iter().filter(|l| !l.starts_with('+')).count();

	// -- If no context/removal lines, assume append to end
	if context_lines_count == 0 {
		let added_count = hunk_lines.len();
		let final_hunk_lines = hunk_lines.iter().map(|s| s.to_string()).collect();
		return Ok((orig_lines.len() + 1, 0, added_count, final_hunk_lines));
	}

	// -- Greedy search for the pattern
	let mut found_idx = None;
	let mut overhang_hl_indices = Vec::new();
	let mut skipped_hl_indices = Vec::new(); // Indices of hunk lines that don't exist in original
	let mut matched_orig_lines: Vec<(usize, String)> = Vec::new(); // (hl_idx, orig_content)

	for i in search_from..=orig_lines.len() {
		let mut matches = true;
		let mut current_overhang = Vec::new();
		let mut current_skipped = Vec::new();
		let mut current_matches = Vec::new();
		let mut orig_off = 0; // offset in orig_lines from i

		for (hl_idx, hl_line) in hunk_lines.iter().enumerate() {
			if hl_line.starts_with('+') {
				continue;
			}

			let p_line = if hl_line.len() > 1 { &hl_line[1..] } else { "" };
			let p_line_trimmed = p_line.trim();

			let target_idx = i + orig_off;

			if p_line_trimmed.is_empty() {
				// If the patch has a blank line...
				if target_idx < orig_lines.len() && orig_lines[target_idx].trim().is_empty() {
					// ... and original has a blank line: Match.
					current_matches.push((hl_idx, orig_lines[target_idx].to_string()));
					orig_off += 1;
				} else {
					// ... and original doesn't have a blank line: Skip this hunk line (resilience).
					current_skipped.push(hl_idx);
				}
			} else if target_idx < orig_lines.len() {
				let orig_line = orig_lines[target_idx];
				let orig_trimmed = orig_line.trim();

				// Resilient match logic:
				let line_match = if orig_trimmed.is_empty() || p_line_trimmed.is_empty() {
					orig_trimmed == p_line_trimmed
				} else {
					orig_trimmed == p_line_trimmed
						|| orig_trimmed.contains(p_line_trimmed)
						|| p_line_trimmed.contains(orig_trimmed)
				};

				if line_match {
					current_matches.push((hl_idx, orig_line.to_string()));
					orig_off += 1;
				} else {
					matches = false;
					break;
				}
			} else {
				// Pattern goes beyond EOF: only allow if it's trailing whitespace/empty context
				current_overhang.push(hl_idx);
			}
		}

		if matches && (!current_matches.is_empty() || !current_overhang.is_empty()) {
			found_idx = Some(i);
			overhang_hl_indices = current_overhang;
			skipped_hl_indices = current_skipped;
			matched_orig_lines = current_matches;
			break;
		}
	}

	let idx = found_idx.ok_or_else(|| {
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

	// -- Reconstruct final hunk lines and calculate counts
	let mut final_hunk_lines = Vec::new();
	let mut old_count = 0;
	let mut new_count = 0;

	for (hl_idx, line) in hunk_lines.iter().enumerate() {
		if overhang_hl_indices.contains(&hl_idx) || skipped_hl_indices.contains(&hl_idx) {
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

	Ok((idx + 1, old_count, new_count, final_hunk_lines))
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
		let completed = complete(original, patch)?;

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
		let completed = complete(original, patch)?;

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
		let completed = complete(original, patch)?;

		// -- Check
		assert!(completed.contains("@@ -1,1 +1,2 @@"));

		Ok(())
	}
}

// endregion: --- Tests

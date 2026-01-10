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

// region:    --- Support

fn compute_hunk_bounds(
	orig_lines: &[&str],
	hunk_lines: &[&str],
	search_from: usize,
) -> Result<(usize, usize, usize, Vec<String>)> {
	// -- Pre-check for pattern existence
	let has_pattern = hunk_lines.iter().any(|l| !l.starts_with('+'));
	if !has_pattern {
		return Err(Error::patch_completion(
			"No context or removal lines found in hunk to match original file",
		));
	}

	// -- Greedy search for the pattern
	let mut found_idx = None;
	let mut overhang_hl_indices = Vec::new();

	for i in search_from..=orig_lines.len() {
		let mut matches = true;
		let mut current_overhang = Vec::new();
		let mut p_idx = 0;

		for (hl_idx, hl_line) in hunk_lines.iter().enumerate() {
			if hl_line.starts_with('+') {
				continue;
			}

			let p_line = if hl_line.starts_with(' ') || hl_line.starts_with('-') {
				&hl_line[1..]
			} else {
				""
			};

			let target_idx = i + p_idx;
			if target_idx >= orig_lines.len() {
				// Allow match beyond EOF only if the pattern line is empty (common LLM sloppiness)
				if p_line.is_empty() {
					current_overhang.push(hl_idx);
				} else {
					matches = false;
					break;
				}
			} else if orig_lines[target_idx] != p_line {
				matches = false;
				break;
			}
			p_idx += 1;
		}

		if matches && p_idx > 0 {
			found_idx = Some(i);
			overhang_hl_indices = current_overhang;
			break;
		}
	}

	let idx = found_idx.ok_or_else(|| Error::patch_completion("Could not find patch context in original file"))?;

	// -- Reconstruct final hunk lines and calculate counts
	let mut final_hunk_lines = Vec::new();
	let mut old_count = 0;
	let mut new_count = 0;

	for (idx, line) in hunk_lines.iter().enumerate() {
		if overhang_hl_indices.contains(&idx) {
			continue;
		}
		final_hunk_lines.push(line.to_string());

		if line.starts_with('+') {
			new_count += 1;
		} else if line.starts_with('-') {
			old_count += 1;
		} else if line.starts_with(' ') || line.trim().is_empty() {
			old_count += 1;
			new_count += 1;
		}
	}

	Ok((idx + 1, old_count, new_count, final_hunk_lines))
}

// endregion: --- Support

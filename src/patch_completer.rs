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

		// If it's an incomplete hunk header
		if trimmed == "@@" {
			let mut hunk_lines = Vec::new();
			while let Some(next_line) = lines.peek() {
				let next_trimmed = next_line.trim();
				if next_trimmed == "@@" || next_trimmed.starts_with("@@ -") {
					break;
				}
				hunk_lines.push(lines.next().unwrap());
			}

			// Compute line numbers
			let (old_start, old_count, new_count) = compute_hunk_bounds(&orig_lines, &hunk_lines, search_from)?;
			let new_start = (old_start as isize + total_delta) as usize;

			// Update state for next hunk
			search_from = old_start + old_count - 1;
			total_delta += new_count as isize - old_count as isize;

			// Standard Unified Diff: @@ -start,len +start,len @@
			completed_patch.push_str(&format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"));
			for h_line in hunk_lines {
				completed_patch.push_str(h_line);
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

fn compute_hunk_bounds(orig_lines: &[&str], hunk_lines: &[&str], search_from: usize) -> Result<(usize, usize, usize)> {
	let mut pattern = Vec::new();
	let mut old_count = 0;
	let mut new_count = 0;

	for line in hunk_lines {
		if line.starts_with(' ') {
			pattern.push(&line[1..]);
			old_count += 1;
			new_count += 1;
		} else if line.starts_with('-') {
			pattern.push(&line[1..]);
			old_count += 1;
		} else if line.starts_with('+') {
			new_count += 1;
		}
		// Skip empty lines or unexpected prefixes
	}

	if pattern.is_empty() {
		return Err(Error::patch_completion(
			"No context or removal lines found in hunk to match original file",
		));
	}

	// Simple greedy search for the pattern
	let mut found_idx = None;
	for i in search_from..=orig_lines.len().saturating_sub(pattern.len()) {
		let mut matches = true;
		for (j, p_line) in pattern.iter().enumerate() {
			if orig_lines[i + j] != *p_line {
				matches = false;
				break;
			}
		}
		if matches {
			found_idx = Some(i);
			break;
		}
	}

	let idx = found_idx.ok_or_else(|| Error::patch_completion("Could not find patch context in original file"))?;

	Ok((idx + 1, old_count, new_count))
}

// endregion: --- Support

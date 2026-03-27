//! Integration tests that run against scenarios in tests/data/test-files/

use assertables::{assert_contains, assert_not_contains};
use simple_fs::SPath;
use udiffx::for_test::{apply_patch_incremental, split_raw_hunks};
use udiffx::{FileDirective, extract_file_changes};

mod test_support;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

#[test]
fn test_patches_test_01() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-01-crlf", false)?;

	// -- Check
	assert_contains!(content, "edition = \"2024\"");
	assert_contains!(content, "resolver = \"3\"");

	Ok(())
}

#[test]
fn test_patches_test_02() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-02-append", false)?;

	// -- Check
	assert!(content.contains("\n\nline 3"));

	Ok(())
}

#[test]
fn test_patches_test_03() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-03-no-matching-empty-line", false)?;

	// -- Check
	assert_contains!(content, "init_profiles_if_missing");

	Ok(())
}

#[test]
fn test_patches_test_04() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-04-no-end-line", false)?;

	// -- Check
	assert_contains!(content, " Improve Patch Completer");

	Ok(())
}

#[test]
fn test_patches_test_05() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-05-missplaced", false)?;

	// -- Check
	assert_contains!(content, "## Request: Ensure Alacritty");

	Ok(())
}

#[test]
fn test_patches_test_06() -> Result<()> {
	// -- Exec
	let res = run_test_scenario("test-06-no-match", true);

	// -- Check
	let _err = res.err().ok_or("Should have failed")?;

	Ok(())
}

#[test]
fn test_patches_test_07() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-07-new-line", false)?;

	// -- Check
	assert_contains!(content, "## Request: Unified Tool");

	Ok(())
}

#[test]
fn test_patches_test_08() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-08-missmatch", false)?;

	// -- Check
	assert_contains!(content, "WorkConfirm(Id), WorkCancel(Id), Run(RunArgs),");
	assert_contains!(content, "WorkConfirm(Id), WorkCancel(Id), WorkRun(Id), WorkClose(Id),");
	assert_contains!(
		content,
		"### Formatting & UI Getters (impl_fmt.rs & impl_model_state.rs)"
	);
	assert_contains!(content, "### Model State Helpers");
	assert_contains!(content, "### Lifecycle & State Processing");
	// Verify removals are gone
	assert!(
		!content.contains("- **Auto-dismiss (4s)**"),
		"Auto-dismiss line should have been removed"
	);
	assert!(
		!content.contains("### formatting & UI Getters"),
		"Lowercase 'formatting' heading should have been removed"
	);

	Ok(())
}

#[test]
fn test_patches_test_09() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-09-fuzzy-ticks", false)?;

	// -- Check
	assert_contains!(content, "**Stage Management**");
	assert_contains!(content, "remains active until the user confirms or closes the dialog");
	// Verify removals are gone
	assert!(
		!content.contains("Auto-dismiss (4s)"),
		"Auto-dismiss line should have been removed"
	);

	Ok(())
}

#[test]
fn test_patches_test_10() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-10", false)?;

	// -- Check
	assert_contains!(content, "local function sort_files_by_mtime");

	Ok(())
}

#[test]
fn test_patches_test_11() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-11-append-with-empty-surround", false)?;

	// -- Check
	assert_contains!(content, "line 3");
	assert!(
		!content.contains("\n\n\n\nline 3"),
		"unexpected extra blank lines were introduced before appended line"
	);
	assert!(
		!content.contains("line 3\n\n\n\n"),
		"unexpected extra blank lines were introduced after appended line"
	);

	Ok(())
}

#[test]
fn test_patches_test_12() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-12-append-with-white-end-surround", false)?;

	// -- Check
	assert_contains!(content, "line 3");
	assert_contains!(content, "line 4");

	Ok(())
}

#[test]
fn test_patches_test_13() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-13-append-trailing-blank-context", false)?;

	// -- Check
	assert_eq!(content, "line 3\nline 4\n");

	Ok(())
}

#[test]
fn test_patches_test_14() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-14-removal-suffix", false)?;

	// -- Check
	assert_contains!(content, "create_async_connection_pool");
	assert_contains!(content, "get_async_connection");
	// Verify the old lines are removed
	assert!(
		!content.contains("create_connection_pool"),
		"Old pool creation should have been removed"
	);
	assert!(
		!content.contains("pool.get_connection"),
		"Old get_connection should have been removed"
	);
	// Verify the rest of the file is intact
	assert_contains!(content, "shutdown_database_connection");

	Ok(())
}

#[test]
fn test_patches_test_15() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-15", false)?;

	// -- Check
	assert_contains!(content, "outputMeshes");
	assert_not_contains!(content, "predictLabels");
	assert_not_contains!(content, "outputLabelmap");

	Ok(())
}

#[test]
fn test_patches_test_16_partial_hunk() -> Result<()> {
	// -- Setup & Fixtures
	let folder = "test-16-partial-hunk";
	let folder_path = SPath::new(format!("tests/data/test-patches/{folder}"));
	let original_path = folder_path.join("original.txt");
	let original = std::fs::read_to_string(&original_path)?;
	let change_path = folder_path.join("changes.txt");
	let changes_str = std::fs::read_to_string(change_path)?;

	let normalized_changes_str = normalize_test_file_tags(&changes_str);

	let (changes, _) = extract_file_changes(&normalized_changes_str, false)?;

	// -- Exec
	// Apply the patch incrementally via apply_patch_incremental (through the public apply_patch path
	// which delegates to incremental when >1 hunk).
	let directive = changes.into_iter().next().ok_or("Should have at least one directive")?;
	let (content, hunk_errors) = match directive {
		FileDirective::Patch {
			content: patch_content, ..
		} => {
			let patch_raw = &patch_content.content;
			let raw_hunks = split_raw_hunks(patch_raw);

			// Verify we have 3 hunks in this scenario
			assert_eq!(raw_hunks.len(), 3, "Expected 3 hunks in test-16 scenario");

			// Apply hunks incrementally, simulating what apply_file_changes does internally
			let original_had_crlf = original.contains("\r\n");
			let mut working = if original_had_crlf {
				original.replace("\r\n", "\n")
			} else {
				original.clone()
			};
			if !working.is_empty() && !working.ends_with('\n') {
				working.push('\n');
			}

			let mut errors = Vec::new();
			let mut applied = 0usize;

			for raw_hunk in &raw_hunks {
				let res: std::result::Result<String, String> = (|| {
					let (completed, _tier) =
						udiffx::for_test::complete(&working, raw_hunk).map_err(|e| e.to_string())?;
					if completed.is_empty() {
						return Err("Empty completed patch".to_string());
					}
					let patch_obj = diffy::Patch::from_str(&completed).map_err(|e| format!("diffy parse: {e}"))?;
					let new_content = diffy::apply(&working, &patch_obj).map_err(|e| format!("diffy apply: {e}"))?;
					Ok(new_content)
				})();

				match res {
					Ok(new_content) => {
						working = new_content;
						applied += 1;
					}
					Err(cause) => {
						errors.push((raw_hunk.clone(), cause));
					}
				}
			}

			assert!(applied > 0, "At least one hunk should have applied");
			(working, errors)
		}
		_ => return Err("Expected FILE_PATCH directive".into()),
	};

	// -- Check
	// Hunk 1 applied: alpha_key updated
	assert_contains!(content, "alpha_key = \"alpha_updated\"");
	assert_not_contains!(content, "alpha_key = \"alpha_value\"");

	// Hunk 2 failed: beta section unchanged
	assert_contains!(content, "beta_key = \"beta_value\"");

	// Hunk 3 applied: gamma_key updated
	assert_contains!(content, "gamma_key = \"gamma_updated\"");
	assert_not_contains!(content, "gamma_key = \"gamma_value\"");

	// Exactly one hunk error (hunk 2)
	assert_eq!(hunk_errors.len(), 1, "Expected exactly 1 failed hunk");
	assert!(!hunk_errors[0].1.is_empty(), "Error cause should be non-empty");

	Ok(())
}

#[test]
fn test_patches_test_17_all_hunks_fail() -> Result<()> {
	// -- Setup & Fixtures
	let folder = "test-17-all-hunks-fail";
	let folder_path = SPath::new(format!("tests/data/test-patches/{folder}"));
	let original_path = folder_path.join("original.txt");
	let original = std::fs::read_to_string(&original_path)?;
	let change_path = folder_path.join("changes.txt");
	let changes_str = std::fs::read_to_string(change_path)?;

	let normalized_changes_str = normalize_test_file_tags(&changes_str);

	let (changes, _) = extract_file_changes(&normalized_changes_str, false)?;

	// -- Exec
	let directive = changes.into_iter().next().ok_or("Should have at least one directive")?;
	let result = match directive {
		FileDirective::Patch {
			content: patch_content, ..
		} => {
			let patch_raw = &patch_content.content;
			let raw_hunks = split_raw_hunks(patch_raw);

			assert_eq!(raw_hunks.len(), 2, "Expected 2 hunks in test-17 scenario");

			let original_had_crlf = original.contains("\r\n");
			let mut working = if original_had_crlf {
				original.replace("\r\n", "\n")
			} else {
				original.clone()
			};
			if !working.is_empty() && !working.ends_with('\n') {
				working.push('\n');
			}

			let mut applied = 0usize;

			for raw_hunk in &raw_hunks {
				let res: std::result::Result<String, String> = (|| {
					let (completed, _tier) =
						udiffx::for_test::complete(&working, raw_hunk).map_err(|e| e.to_string())?;
					if completed.is_empty() {
						return Err("Empty completed patch".to_string());
					}
					let patch_obj = diffy::Patch::from_str(&completed).map_err(|e| format!("diffy parse: {e}"))?;
					let new_content = diffy::apply(&working, &patch_obj).map_err(|e| format!("diffy apply: {e}"))?;
					Ok(new_content)
				})();

				if let Ok(new_content) = res {
					working = new_content;
					applied += 1;
				}
			}

			if applied == 0 {
				Err("All hunks failed".to_string())
			} else {
				Ok(working)
			}
		}
		_ => Err("Expected FILE_PATCH directive".to_string()),
	};

	// -- Check
	// All hunks should have failed
	let err = result.err().ok_or("Should have failed because all hunks have wrong context")?;
	assert_contains!(err, "All hunks failed");

	// Original content should be unchanged (we never modified the original variable)
	assert_contains!(original, "first_line = \"hello\"");
	assert_contains!(original, "second_line = \"world\"");

	Ok(())
}

#[test]
fn test_patches_test_18() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-18-out-of-order", false)?;

	// -- Check
	// Hunk 1 (file-order): additions after m_useFirstHitpointForDrag block
	assert_contains!(content, "m_dragStartedCallback");
	// Hunk 2 (file-order): additions after setBothPlanesChangedCallback
	assert_contains!(content, "setDragStartedCallback");
	assert_contains!(content, "setDragEndedCallback");
	// Original content still present
	assert_contains!(content, "setHitThreshold");
	assert_contains!(content, "setBothPlanesChangedCallback");

	Ok(())
}

#[test]
fn test_patches_test_19_tilde_range() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-19-tilde-range", false)?;

	// -- Check
	// All old dependencies should be removed
	assert_not_contains!(content, "dep_a");
	assert_not_contains!(content, "dep_b");
	assert_not_contains!(content, "dep_c");
	assert_not_contains!(content, "dep_d");
	assert_not_contains!(content, "dep_e");
	assert_not_contains!(content, "dep_f");
	assert_not_contains!(content, "dep_g");
	assert_not_contains!(content, "dep_h");
	// New dependency should be present
	assert_contains!(content, "new_dep = \"2.0\"");
	// Surrounding content should be intact
	assert_contains!(content, "[dependencies]");
	assert_contains!(content, "[settings]");
	assert_contains!(content, "debug = true");

	Ok(())
}

#[test]
fn test_patches_test_20() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-20-wrapper-lines", false)?;

	// -- Check
	assert_contains!(
		content,
		"- When `dev.spec` is enabled with the default path, the default spec file is `$coder_prompt_dir/dev/spec/spec.md`."
	);
	assert_contains!(
		content,
		"- Ensures the spec context file exists at the resolved spec file path."
	);
	assert_contains!(
		content,
		"- Appends the resolved chat path and `plan-*.md` glob to `context_globs_post` when missing (deduped)."
	);

	Ok(())
}

#[test]
fn test_patches_test_21() -> Result<()> {
	// -- Exec
	let res = run_test_scenario("test-21-silent-noop-hunk", true);

	// -- Check
	let err = res.err().ok_or("Should have failed")?;
	assert_contains!(err.to_string(), "Could not find patch context in original file");

	Ok(())
}

#[test]
fn test_patches_test_22() -> Result<()> {
	// -- Exec
	let res = run_test_scenario("test-22-not-matching", true);

	// -- Check
	assert!(res.is_err(), "should be error");

	Ok(())
}

// region:    --- Support

fn run_test_scenario(folder: &str, should_fail: bool) -> Result<String> {
	let folder_path = SPath::new(format!("tests/data/test-patches/{folder}"));
	let original_path = folder_path.join("original.txt");
	let original = std::fs::read_to_string(&original_path)?;
	let change_path = folder_path.join("changes.txt");
	let changes_str = std::fs::read_to_string(change_path)?;

	let normalized_changes_str = normalize_test_file_tags(&changes_str);

	let (changes, _) = extract_file_changes(&normalized_changes_str, false)?;
	let mut content = original;

	for change in changes {
		match change {
			FileDirective::Patch {
				content: patch_content, ..
			} => {
				content = match apply_patch_incremental(&content, &patch_content.content) {
					Ok((content, ..)) => content,
					Err(err) => {
						if !should_fail {
							println!("Error for {folder} scenario:\n{err}");
						}
						return Err(format!("scenario {folder} failed\n{err}").into());
					}
				};
			}
			_ => return Err("Only FILE_PATCH is supported in this in-memory test for now".into()),
		}
	}

	Ok(content)
}

/// Normalizes `TEST_FILE_*` tags to `FILE_*` tags so test fixture files
/// do not collide with the real tag names during extraction.
fn normalize_test_file_tags(input: &str) -> String {
	input
		.replace("TEST_FILE_CHANGES", "FILE_CHANGES")
		.replace("TEST_FILE_PATCH", "FILE_PATCH")
		.replace("TEST_FILE_NEW", "FILE_NEW")
		.replace("TEST_FILE_APPEND", "FILE_APPEND")
		.replace("TEST_FILE_RENAME", "FILE_RENAME")
		.replace("TEST_FILE_DELETE", "FILE_DELETE")
}

// endregion: --- Support

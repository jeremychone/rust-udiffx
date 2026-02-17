//! Integration tests that run against scenarios in tests/data/test-files/

use assertables::assert_contains;
use simple_fs::SPath;
use udiffx::for_test::apply_patch;
use udiffx::{FileDirective, extract_file_changes};

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

// region:    --- Support

fn run_test_scenario(folder: &str, should_fail: bool) -> Result<String> {
	let folder_path = SPath::new(format!("tests/data/test-patches/{folder}"));
	let original_path = folder_path.join("original.txt");
	let original = std::fs::read_to_string(&original_path)?;
	let change_path = folder_path.join("changes.txt");
	let changes_str = std::fs::read_to_string(change_path)?;

	let (changes, _) = extract_file_changes(&changes_str, false)?;
	let mut content = original;

	for change in changes {
		match change {
			FileDirective::Patch {
				content: patch_content, ..
			} => {
				content = match apply_patch(original_path.as_str(), &content, &patch_content.content) {
					Ok((content, _)) => content,
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

// endregion: --- Support

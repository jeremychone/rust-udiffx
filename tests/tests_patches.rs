//! Integration tests that run against scenarios in tests/data/test-files/

use assertables::assert_contains;
use simple_fs::SPath;
use udiffx::for_test::apply_patch;
use udiffx::{FileDirective, extract_file_changes};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

#[test]
fn test_patches_test_01() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-01-crlf")?;

	// -- Check
	assert_contains!(content, "edition = \"2024\"");
	assert_contains!(content, "resolver = \"3\"");

	Ok(())
}

#[test]
fn test_patches_test_02() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-02-append")?;

	// -- Check
	assert!(content.contains("\n\nline 3"));

	Ok(())
}

#[test]
fn test_patches_test_03() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-03-no-matching-empty-line")?;

	// -- Check
	assert_contains!(content, "init_profiles_if_missing");

	Ok(())
}

#[test]
fn test_patches_test_04() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-04-no-end-line")?;

	// -- Check
	assert_contains!(content, " Improve Patch Completer");

	Ok(())
}

#[test]
fn test_patches_test_05() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-05-missplaced")?;

	// -- Check
	assert_contains!(content, "## Request: Ensure Alacritty");

	Ok(())
}

#[test]
fn test_patches_test_06() -> Result<()> {
	// -- Exec
	let res = run_test_scenario("test-06-no-match");

	// -- Check
	let _err = res.err().ok_or("Should have failed")?;

	Ok(())
}

#[test]
fn test_patches_test_07() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-07-new-line")?;

	// -- Check
	assert_contains!(content, "## Request: Unified Tool");

	Ok(())
}

// region:    --- Support

fn run_test_scenario(folder: &str) -> Result<String> {
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
					Ok(content) => content,
					Err(err) => {
						// println!("Error for {folder} scenario:\n{err}");
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

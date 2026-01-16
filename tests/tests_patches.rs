//! Integration tests that run against scenarios in tests/data/test-files/

use udiffx::for_test::apply_patch;
use udiffx::{FileDirective, extract_file_changes};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

#[test]
fn test_patches_test_01() -> Result<()> {
	// -- Exec
	let content = run_test_scenario("test-01-crlf")?;

	// -- Check
	assert!(content.contains("edition = \"2024\""));
	assert!(content.contains("resolver = \"3\""));

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
	let content = run_test_scenario("test-03-multi-hunks")?;

	// -- Check
	assert!(content.contains("\n\nline 3"));

	Ok(())
}

// region:    --- Support

fn run_test_scenario(folder: &str) -> Result<String> {
	let folder_path = format!("tests/data/test-patches/{folder}");
	let original = std::fs::read_to_string(format!("{folder_path}/original.txt"))?;
	let changes_str = std::fs::read_to_string(format!("{folder_path}/changes.txt"))?;

	let (changes, _) = extract_file_changes(&changes_str, false)?;
	let mut content = original;

	for change in changes {
		match change {
			FileDirective::Patch {
				content: patch_content, ..
			} => {
				content = apply_patch(&content, &patch_content.content)?;
			}
			_ => return Err("Only FILE_PATCH is supported in this in-memory test for now".into()),
		}
	}

	Ok(content)
}

// endregion: --- Support

//! Integration tests for applying extracted FILE_CHANGES fixtures.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

use simple_fs::SPath;
use udiffx::{apply_file_changes, extract_file_changes};

mod test_support;

#[test]
fn test_changes_no_changes() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("tests_changes_no_changes")?;
	let input = include_str!("data/changes-no-changes.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	assert!(
		status.infos.is_empty() || status.infos.iter().all(|i| i.success()),
		"Expected no failures, got: {status:#?}"
	);

	Ok(())
}

#[test]
fn test_changes_simple() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("test_changes_simple")?;
	let input = include_str!("data/changes-simple.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	let len = status.infos.iter().count();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.infos.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");

	Ok(())
}

#[test]
fn test_changes_no_head_nums() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("test_changes_no_head_nums")?;
	let input = include_str!("data/changes-no-head-nums.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	let len = status.infos.iter().count();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.infos.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");
	// check main.rs
	let main_content = simple_fs::read_to_string(base_dir.join("src/main.rs"))?;
	assert!(
		main_content.contains("hello::hello()"),
		"main.rs should contain 'hello::hello()'"
	);

	Ok(())
}

#[test]
fn test_changes_with_code_fence() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("tests_changes_with_code_fence")?;
	let base_dir_spath = SPath::new(&base_dir);
	let input = include_str!("data/changes-with-code-fence.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir_spath, changes)?;

	// -- Check
	let len = status.infos.iter().count();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.infos.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");

	Ok(())
}

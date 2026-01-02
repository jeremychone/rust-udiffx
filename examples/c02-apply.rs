use simple_fs::{SPath, SaferRemoveOptions, ensure_dir, read_to_string, safer_remove_dir};
use udiffx::{apply_file_changes, extract_file_changes};

const FILE: &str = "tests/data/changes-with-code-fence.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let base_dir = SPath::new("examples/.out/c02-apply");

	// -- Setup & Clean
	// We clean the target directory to ensure a fresh application of the changes.
	if base_dir.exists() {
		safer_remove_dir(
			&base_dir,
			SaferRemoveOptions::default().with_must_contain_any(&["examples/"]),
		)?;
	}
	ensure_dir(&base_dir)?;

	// -- Load and Extract
	let md_content = read_to_string(FILE)?;
	let file_changes = extract_file_changes(&md_content, false)?.0;
	if file_changes.is_empty() {
		println!("No <FILE_CHANGES> block found in '{FILE}'");
		return Ok(());
	}

	let len = file_changes.iter().count();
	println!("Found {len} directives in {FILE}",);

	// -- Apply
	let info = apply_file_changes(&base_dir, file_changes)?;

	// -- Print Result
	println!("\nApplied changes to: {base_dir}");
	for dir_info in info.infos {
		println!(
			"  - {:>7}: {:<5} {}",
			dir_info.kind(),
			dir_info.success(),
			dir_info.file_path()
		);
		if let Some(err) = dir_info.error_msg() {
			println!("   Error: {}", err);
		}
	}

	Ok(())
}

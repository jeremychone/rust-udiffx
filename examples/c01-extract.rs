use std::fs;
use udiffx::extract_file_changes;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
	let file_path = "tests/data/changes-simple.md";
	let content = fs::read_to_string(file_path)?;

	let changes = extract_file_changes(&content, false)?.0;

	if !changes.is_empty() {
		println!("{changes:#?}");
	} else {
		println!("No changes found in {file_path}");
	}

	Ok(())
}

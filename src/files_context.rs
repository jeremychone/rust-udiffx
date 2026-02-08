use crate::Result;
use simple_fs::{SPath, list_files, read_to_string};

/// Gathers file contents based on globs relative to a `base_dir` and formats them
/// into a `<FILE_CONTENT path="...">content</FILE_CONTENT>` block.
pub fn load_files_context(base_dir: impl Into<SPath>, globs: &[&str]) -> Result<Option<String>> {
	let base_dir = base_dir.into();
	let files = list_files(&base_dir, Some(globs), None)?;

	let res = if !files.is_empty() {
		let mut out = String::new();

		for file in files {
			let rel_path = file.diff(base_dir.path()).ok_or_else(|| {
				crate::Error::Custom(format!("Could not get relative path for '{}'", file.path().as_str()))
			})?;
			let content = read_to_string(file.path()).map_err(crate::Error::simple_fs)?;

			out.push_str(&format!("<FILE_CONTENT path=\"{}\">\n", rel_path.as_str()));
			out.push_str(&content);
			if !content.ends_with('\n') {
				out.push('\n');
			}
			out.push_str("</FILE_CONTENT>\n\n");
		}
		Some(out)
	} else {
		None
	};

	Ok(res)
}

// region:    --- Tests

#[cfg(test)]
mod tests {
	type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

	use super::*;
	use std::fs;

	#[test]
	fn test_load_files_context_simple() -> Result<()> {
		// -- Setup & Fixtures
		let test_dir = SPath::new("tests/.out/test_load_files_context_simple");
		if test_dir.exists() {
			fs::remove_dir_all(test_dir.std_path())?;
		}
		fs::create_dir_all(test_dir.join("src").std_path())?;
		fs::write(test_dir.join("src/main.rs").std_path(), "fn main() {}")?;
		fs::write(test_dir.join("src/lib.rs").std_path(), "pub mod a;")?;

		// -- Exec
		let context = load_files_context(&test_dir, &["src/**/*.rs"])?.ok_or("Should have context")?;

		// -- Check
		assert!(context.contains("<FILE_CONTENT path=\"src/lib.rs\">"));
		assert!(context.contains("pub mod a;"));
		assert!(context.contains("<FILE_CONTENT path=\"src/main.rs\">"));
		assert!(context.contains("fn main() {}"));

		// Cleanup
		// fs::remove_dir_all(test_dir.std_path())?;

		Ok(())
	}
}

// endregion: --- Tests

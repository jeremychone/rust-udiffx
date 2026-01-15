use super::TestResult;
use simple_fs::SPath;
use std::path::{Path, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn list_test_files_dirs() -> TestResult<Vec<SPath>> {
	let base = Path::new("tests/data/test-files");
	let mut dirs = Vec::new();

	if base.exists() {
		for entry in fs::read_dir(base)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_dir() {
				dirs.push(SPath::try_from(path)?);
			}
		}
	}

	// Sort for deterministic test order
	dirs.sort_by(|a, b| a.as_str().cmp(b.as_str()));

	Ok(dirs)
}

pub fn new_out_dir_path(prefix: &str) -> TestResult<SPath> {
	let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
	let dir = PathBuf::from("tests/.out").join(format!("{prefix}_{now_ms}"));
	std::fs::create_dir_all(&dir)?;
	let dir = SPath::try_from(dir)?;

	Ok(dir)
}

pub fn delete_out_dir(dir: impl AsRef<Path>) -> TestResult<()> {
	let dir = dir.as_ref();

	let dir_s = dir.to_string_lossy();
	if !dir_s.contains("tests/.out") {
		return Err(format!("Refusing to delete dir outside tests/.out: {dir_s}").into());
	}

	let cwd = std::env::current_dir()?;
	if !dir.starts_with(&cwd) {
		return Err(format!(
			"Refusing to delete dir not under current directory, cwd: {}, dir: {}",
			cwd.to_string_lossy(),
			dir_s
		)
		.into());
	}

	// std::fs::remove_dir_all(dir)?;

	Ok(())
}

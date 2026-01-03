use super::TestResult;
use simple_fs::SPath;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

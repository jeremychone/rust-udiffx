use crate::Result;
use simple_fs::SPath;

/// Checks if the target path is safe to write, ensuring it remains within the base directory.
pub fn check_for_write(target: &SPath, base_dir: &SPath) -> Result<()> {
	check_in_base(target, base_dir)
}

/// Checks if the target path is safe to read, ensuring it remains within the base directory.
pub fn check_for_read(target: &SPath, base_dir: &SPath) -> Result<()> {
	check_in_base(target, base_dir)
}

// region:    --- Support

fn check_in_base(target: &SPath, base_dir: &SPath) -> Result<()> {
	let base_dir = base_dir.clone().into_collapsed();
	let target = target.clone().into_collapsed();

	if !target.as_str().starts_with(base_dir.as_str()) {
		return Err(crate::Error::security_violation(target.to_string(), base_dir.to_string()).into());
	}

	Ok(())
}

// endregion: --- Support

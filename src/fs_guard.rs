use crate::{Error, Result, SecurityPolicy};
use simple_fs::SPath;

/// Checks if the target path is safe to write, ensuring it remains within the base directory.
pub fn check_for_write(target: &SPath, base_dir: &SPath, policy: Option<&SecurityPolicy>) -> Result<()> {
	if let Some(policy) = policy
		&& policy.bypass_all_checks
	{
		return Ok(());
	}
	if is_under_dir(target, base_dir) {
		return Ok(());
	}
	if let Some(policy) = policy {
		for dir in &policy.writable_dirs {
			if is_under_dir(target, dir) {
				return Ok(());
			}
		}
	}
	Err(Error::security_violation(target.to_string(), base_dir.to_string()))
}

/// Checks if the target path is safe to read, ensuring it remains within the base directory.
pub fn check_for_read(target: &SPath, base_dir: &SPath, policy: Option<&SecurityPolicy>) -> Result<()> {
	if let Some(p) = policy {
		return p.assert_path_read_access(target, base_dir);
	}
	SecurityPolicy::default().assert_path_read_access(target, base_dir)
}

// region:    --- Support

fn is_under_dir(target: &SPath, dir: &SPath) -> bool {
	let dir = dir.clone().into_collapsed();
	let target = target.clone().into_collapsed();
	target.as_str().starts_with(dir.as_str())
}

// endregion: --- Support

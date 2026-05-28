use crate::{Error, Result};
use simple_fs::SPath;

/// A configurable, safe-by-default security policy that controls
/// path-traversal checks for file-change operations (read and write)
/// within the `udiffx` library.
///
/// Passed as an optional parameter to `apply_file_changes`.
/// When `None`, the behaviour is equivalent to `SecurityPolicy::default()`:
///
/// - `writable_dirs` is empty — writes are allowed only inside `base_dir`.
/// - `read_anywhere` is `false` — reads are also confined to `base_dir`.
/// - `bypass_all_checks` is `false` — all path checks are enforced.
///
/// To allow writes outside `base_dir`, populate `writable_dirs`.
/// To allow reading from anywhere, call `.with_read_anywhere()`.
/// To disable all checks entirely, call `.with_bypass_all_checks()`.
#[derive(Debug, Clone, Default)]
pub struct SecurityPolicy {
	/// Directories where writes are allowed.
	/// If empty, writes are restricted to the operation's `base_dir`.
	pub writable_dirs: Vec<SPath>,

	/// When `true`, allow reading from any path, even outside the
	/// writable directories. (default false)
	pub read_anywhere: bool,

	/// When `true`, **all** path checks are disabled. (default false)
	pub bypass_all_checks: bool,
}

/// Constructors
impl SecurityPolicy {
	pub fn trusted_cwd() -> Self {
		let mut policy = Self::default();
		if let Ok(cwd) = simple_fs::current_dir() {
			policy.writable_dirs.push(cwd);
		}
		policy
	}

	/// Construct a policy with `writable_dirs` populated from the iterator.
	/// Other fields remain at their default values.
	pub fn from_writable_dirs(dirs: impl IntoIterator<Item = impl Into<SPath>>) -> Self {
		Self {
			writable_dirs: dirs.into_iter().map(|d| d.into()).collect(),
			..Default::default()
		}
	}
}

impl From<Option<SecurityPolicy>> for SecurityPolicy {
	fn from(opt: Option<SecurityPolicy>) -> Self {
		opt.unwrap_or_default()
	}
}

/// Access assertion
impl SecurityPolicy {
	/// Asserts that a given directory `target` is allowed for write operations according to this policy.
	/// Used primarily to validate the base directory of a file-change operation.
	///
	/// The check is as follows:
	/// - If `bypass_all_checks` is set, always succeeds.
	/// - If `writable_dirs` contains a directory that is an ancestor of `target`, succeeds.
	/// - Otherwise (default), succeeds only if `target` is under the current working directory (CWD).
	pub fn assert_write_access(&self, target: &SPath) -> Result<()> {
		if self.bypass_all_checks {
			return Ok(());
		}
		let target_str = target.as_str();
		// Check explicit writable directories
		for wd in &self.writable_dirs {
			if target_str.starts_with(wd.as_str()) {
				return Ok(());
			}
		}
		// Fallback: default strict policy — target must be under current working directory.
		use std::env;
		let cwd = env::current_dir().map_err(|e| Error::io_read_file(".", e))?;
		let cwd_spath = SPath::from_std_path(cwd).map_err(|e| Error::custom(format!("invalid CWD: {e}")))?;
		if !target_str.starts_with(cwd_spath.as_str()) {
			return Err(Error::security_violation(target.to_string(), cwd_spath.to_string()));
		}
		Ok(())
	}

	/// Asserts that a given file or directory path is allowed for read operations,
	/// considering an operation’s base directory.
	///
	/// - If `read_anywhere` or `bypass_all_checks` is set, always succeeds.
	/// - Otherwise, succeeds if `target` is under `base_dir` or any `writable_dirs`.
	pub fn assert_path_read_access(&self, target: &SPath, base_dir: &SPath) -> Result<()> {
		if self.bypass_all_checks || self.read_anywhere {
			return Ok(());
		}
		// Check base_dir first (most common case)
		if target.as_str().starts_with(base_dir.as_str()) {
			return Ok(());
		}
		// Check explicit writable directories
		for wd in &self.writable_dirs {
			if target.as_str().starts_with(wd.as_str()) {
				return Ok(());
			}
		}
		Err(Error::security_violation(target.to_string(), base_dir.to_string()))
	}

	/// Asserts that a given directory `target` is allowed for read operations according to this policy.
	/// If `read_anywhere` or `bypass_all_checks` is set, reads are allowed anywhere.
	/// Otherwise, falls back to the write access check (i.e., the target must be in a writable directory).
	pub fn assert_read_access(&self, target: &SPath) -> Result<()> {
		if self.bypass_all_checks || self.read_anywhere {
			return Ok(());
		}
		self.assert_write_access(target)
	}
}

/// Fluid apis
impl SecurityPolicy {
	/// Allow reads from any path, even outside writable directories.
	pub fn with_read_anywhere(mut self) -> Self {
		self.read_anywhere = true;
		self
	}

	/// Disable all path checks.
	pub fn with_bypass_all_checks(mut self) -> Self {
		self.bypass_all_checks = true;
		self
	}
	/// Override the current writable directories with the given iterator.
	pub fn with_writable_dirs(mut self, dirs: impl IntoIterator<Item = impl Into<SPath>>) -> Self {
		self.writable_dirs = dirs.into_iter().map(|d| d.into()).collect();
		self
	}

	/// Append an additional writable directory to the policy.
	pub fn append_writable_dir(mut self, dir: impl Into<SPath>) -> Self {
		self.writable_dirs.push(dir.into());
		self
	}

	/// Append additional writable directories to the existing list.
	pub fn append_writable_dirs(mut self, dirs: impl IntoIterator<Item = impl Into<SPath>>) -> Self {
		self.writable_dirs.extend(dirs.into_iter().map(|d| d.into()));
		self
	}
}
// region:    --- Tests

#[cfg(test)]
mod tests {
	type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

	use super::*;

	#[test]
	fn test_security_policy_default() -> Result<()> {
		// -- Setup & Fixtures

		// -- Exec
		let policy = SecurityPolicy::default();

		// -- Check
		assert!(policy.writable_dirs.is_empty());
		assert!(!policy.read_anywhere);
		assert!(!policy.bypass_all_checks);

		Ok(())
	}

	#[test]
	fn test_security_policy_trusted_cwd() -> Result<()> {
		// -- Setup & Fixtures

		// -- Exec
		let policy = SecurityPolicy::trusted_cwd();

		// -- Check
		if let Ok(cwd) = std::env::current_dir() {
			let cwd_spath = SPath::from_std_path_ok(cwd).ok_or("should convert CWD to SPath")?;
			assert!(
				policy.writable_dirs.iter().any(|d| d == &cwd_spath),
				"trusted_cwd should include the current working directory"
			);
		}
		// Other fields remain default
		assert!(!policy.read_anywhere);
		assert!(!policy.bypass_all_checks);

		Ok(())
	}

	#[test]
	fn test_security_policy_append_writable_dir() -> Result<()> {
		// -- Setup & Fixtures
		let dir = SPath::new("/some/test/dir");

		// -- Exec
		let policy = SecurityPolicy::default().append_writable_dir(dir.clone());

		// -- Check
		assert_eq!(policy.writable_dirs.len(), 1);
		assert_eq!(policy.writable_dirs[0], dir);

		Ok(())
	}

	#[test]
	fn test_security_policy_with_read_anywhere() -> Result<()> {
		// -- Exec
		let policy = SecurityPolicy::default().with_read_anywhere();

		// -- Check
		assert!(policy.read_anywhere);

		Ok(())
	}

	#[test]
	fn test_security_policy_with_bypass_all_checks() -> Result<()> {
		// -- Exec
		let policy = SecurityPolicy::default().with_bypass_all_checks();

		// -- Check
		assert!(policy.bypass_all_checks);

		Ok(())
	}
}

// endregion: --- Tests

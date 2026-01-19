// region:    --- Modules

mod fs_guard;

mod applier;
mod apply_changes_status;
mod error;
mod extract;
mod file_changes;
mod file_directives;
mod patch_completer;

#[cfg(feature = "prompt")]
mod prompt;

pub use applier::*;
pub use apply_changes_status::*;
pub use error::*;
pub use extract::*;
pub use file_changes::*;
pub use file_directives::*;

#[cfg(feature = "prompt")]
pub use prompt::prompt;

#[cfg(any(test, feature = "test-support"))]
pub mod for_test {
	pub use crate::applier::apply_patch;
	pub use crate::patch_completer::complete;
}

// endregion: --- Modules

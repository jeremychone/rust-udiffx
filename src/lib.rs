// region:    --- Modules

mod fs_guard;

mod applier;
mod apply_changes_status;
mod error;
mod extract;
mod file_changes;
mod file_directives;
mod files_context;
mod patch_completer;

pub use applier::apply_file_changes;
pub use apply_changes_status::*;
pub use error::*;
pub use extract::*;
pub use file_changes::*;
pub use file_directives::*;
pub use files_context::load_files_context;
pub use patch_completer::MatchTier;
pub use patch_completer::split_raw_hunks;
pub use patch_completer::has_actionable_hunks;

// -- feature prompt
#[cfg(feature = "prompt")]
mod prompt;
#[cfg(feature = "prompt")]
pub use prompt::prompt_file_changes;

#[cfg(any(test, feature = "test-support"))]
pub mod for_test {
	pub use crate::applier::apply_patch;
	pub use crate::patch_completer::complete;
	pub use crate::patch_completer::has_actionable_hunks;
	pub use crate::patch_completer::split_raw_hunks;
}

// endregion: --- Modules

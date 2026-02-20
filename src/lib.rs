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
pub mod hashline;
pub use hashline::{apply_hashline_edits, HashlineEdit, LineTag};

pub use applier::apply_file_changes;
pub use apply_changes_status::*;
pub use error::*;
pub use extract::*;
pub use file_changes::*;
pub use file_directives::*;
pub use files_context::load_files_context;
pub use patch_completer::MatchTier;

// -- feature prompt
#[cfg(feature = "prompt")]
mod prompt;
#[cfg(feature = "prompt")]
pub use prompt::prompt_file_changes;

#[cfg(any(test, feature = "test-support"))]
pub mod for_test {
	pub use crate::applier::apply_patch;
	pub use crate::patch_completer::complete;
}

// endregion: --- Modules

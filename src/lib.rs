// region:    --- Modules

mod fs_guard;

mod applier;
mod apply_changes_status;
mod error;
mod extract;
mod file_changes;
mod file_directives;

pub use applier::*;
pub use apply_changes_status::*;
pub use error::*;
pub use extract::*;
pub use file_changes::*;
pub use file_directives::*;

// endregion: --- Modules

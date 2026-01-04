use derive_more::{Display, From};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Display, From)]
#[display("{self:?}")]
pub enum Error {
	#[display("{_0}")]
	#[from(String, &String, &str)]
	Custom(String),

	// -- Parse / Extract
	#[display("Missing attribute '{attr}' for tag '{tag}'")]
	ParseMissingAttribute { tag: String, attr: String },
	#[display("Unknown directive tag '{tag}'")]
	ParseUnknownDirectiveTag { tag: String },

	// -- Apply / Operations
	#[display("Path not found for {op}: {path}")]
	ApplyPathNotFound { op: String, path: String },

	// -- Security / Guard
	#[display("Security violation, target '{target}' is outside base dir '{base_dir}'")]
	SecurityViolation { target: String, base_dir: String },

	// -- diffy
	#[display("diffy parse patch error: {cause}")]
	DiffyParsePatch { cause: String },

	#[display("diffy apply patch error: {cause}")]
	DiffyApplyPatch { cause: String },

	#[display("patch completion error: {cause}")]
	PatchCompletion { cause: String },

	// -- Externals (captured as cause strings, but with udiffx semantics)
	#[display("Read file failed: {_0}")]
	IoReadFile(PathAndCause),

	#[display("Create file failed: {_0}")]
	IoCreateFile(PathAndCause),

	#[display("Write file failed: {_0}")]
	IoWriteFile(PathAndCause),

	#[display("Rename path failed: {from_path} -> {to_path}, cause: {cause}")]
	IoRenamePath {
		from_path: String,
		to_path: String,
		cause: String,
	},

	#[display("Delete file failed: {_0}")]
	IoDeleteFile(PathAndCause),

	#[display("Delete dir failed: {_0}")]
	IoDeleteDirAll(PathAndCause),

	#[display("simple_fs error: {cause}")]
	SimpleFs { cause: String },
}

#[derive(Debug, Clone, Display)]
#[display("{path}, cause: {cause}")]
pub struct PathAndCause {
	pub path: String,
	pub cause: String,
}

// region:    --- Custom

impl Error {
	pub fn parse_missing_attribute(tag: impl Into<String>, attr: impl Into<String>) -> Self {
		Self::ParseMissingAttribute {
			tag: tag.into(),
			attr: attr.into(),
		}
	}

	pub fn parse_unknown_directive_tag(tag: impl Into<String>) -> Self {
		Self::ParseUnknownDirectiveTag { tag: tag.into() }
	}

	pub fn apply_path_not_found(op: impl Into<String>, path: impl Into<String>) -> Self {
		Self::ApplyPathNotFound {
			op: op.into(),
			path: path.into(),
		}
	}

	pub fn security_violation(target: impl Into<String>, base_dir: impl Into<String>) -> Self {
		Self::SecurityViolation {
			target: target.into(),
			base_dir: base_dir.into(),
		}
	}

	pub fn io_read_file(path: impl Into<String>, err: impl std::error::Error) -> Self {
		Self::IoReadFile(PathAndCause {
			path: path.into(),
			cause: err.to_string(),
		})
	}

	pub fn io_create_file(path: impl Into<String>, err: impl std::error::Error) -> Self {
		Self::IoCreateFile(PathAndCause {
			path: path.into(),
			cause: err.to_string(),
		})
	}

	pub fn io_write_file(path: impl Into<String>, err: impl std::error::Error) -> Self {
		Self::IoWriteFile(PathAndCause {
			path: path.into(),
			cause: err.to_string(),
		})
	}

	pub fn io_rename_path(
		from_path: impl Into<String>,
		to_path: impl Into<String>,
		err: impl std::error::Error,
	) -> Self {
		Self::IoRenamePath {
			from_path: from_path.into(),
			to_path: to_path.into(),
			cause: err.to_string(),
		}
	}

	pub fn io_delete_file(path: impl Into<String>, err: impl std::error::Error) -> Self {
		Self::IoDeleteFile(PathAndCause {
			path: path.into(),
			cause: err.to_string(),
		})
	}

	pub fn io_delete_dir_all(path: impl Into<String>, err: impl std::error::Error) -> Self {
		Self::IoDeleteDirAll(PathAndCause {
			path: path.into(),
			cause: err.to_string(),
		})
	}

	pub fn simple_fs(err: impl std::error::Error) -> Self {
		Self::SimpleFs { cause: err.to_string() }
	}

	pub fn diffy_parse_patch(err: impl std::error::Error) -> Self {
		Self::DiffyParsePatch { cause: err.to_string() }
	}

	pub fn diffy_apply_patch(err: impl std::error::Error) -> Self {
		Self::DiffyApplyPatch { cause: err.to_string() }
	}

	pub fn patch_completion(cause: impl Into<String>) -> Self {
		Self::PatchCompletion { cause: cause.into() }
	}
}

// endregion: --- Custom

// region:    --- Error Boilerplate

impl std::error::Error for Error {}

// endregion: --- Error Boilerplate

// region:    --- Froms

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		Self::Custom(err.to_string())
	}
}

impl From<simple_fs::Error> for Error {
	fn from(err: simple_fs::Error) -> Self {
		Self::simple_fs(err)
	}
}

impl From<diffy::ParsePatchError> for Error {
	fn from(err: diffy::ParsePatchError) -> Self {
		Self::diffy_parse_patch(err)
	}
}

impl From<diffy::ApplyError> for Error {
	fn from(err: diffy::ApplyError) -> Self {
		Self::diffy_apply_patch(err)
	}
}

// endregion: --- Froms

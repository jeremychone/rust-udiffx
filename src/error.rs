use derive_more::{Display, From};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Display, From)]
pub enum Error {
	#[from(String, &String, &str)]
	Custom(String),

	// -- Externals
	#[from]
	Io(std::io::Error),

	#[from]
	SimpleFs(simple_fs::Error),

	#[from]
	DiffyParse(diffy::ParsePatchError),

	#[from]
	DiffyApply(diffy::ApplyError),
}

// region:    --- Error Boilerplate

impl std::error::Error for Error {}

// endregion: --- Error Boilerplate

use crate::FileDirective;

#[derive(Debug, Clone)]
pub struct ApplyChangesInfo {
	pub infos: Vec<DirectiveInfo>,
}

#[derive(Debug, Clone)]
pub struct DirectiveInfo {
	pub kind: DirectiveKind,
	pub success: bool,
	pub error_msg: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DirectiveKind {
	New {
		file_path: String,
	},
	Patch {
		file_path: String,
	},
	Rename {
		from_path: String,
		file_path: String,
	},
	Delete {
		file_path: String,
	},

	Fail {
		kind_str: String,
		file_path: Option<String>,
	},
}

impl DirectiveInfo {
	pub fn file_path(&self) -> &str {
		match &self.kind {
			DirectiveKind::New { file_path } => file_path,
			DirectiveKind::Patch { file_path } => file_path,
			DirectiveKind::Rename { file_path, .. } => file_path,
			DirectiveKind::Delete { file_path } => file_path,
			DirectiveKind::Fail { file_path, .. } => file_path.as_deref().unwrap_or("unknown"),
		}
	}

	pub fn success(&self) -> bool {
		self.success
	}

	pub fn error_msg(&self) -> Option<&str> {
		self.error_msg.as_deref()
	}

	pub fn kind(&self) -> &'static str {
		match &self.kind {
			DirectiveKind::New { .. } => "New",
			DirectiveKind::Patch { .. } => "Patch",
			DirectiveKind::Rename { .. } => "Rename",
			DirectiveKind::Delete { .. } => "Delete",
			DirectiveKind::Fail { .. } => "Fail",
		}
	}
}

// region:    --- Froms

impl From<&FileDirective> for DirectiveInfo {
	fn from(directive: &FileDirective) -> Self {
		let mut error_msg = None;

		let kind = match directive {
			FileDirective::New { file_path, .. } => DirectiveKind::New {
				file_path: file_path.clone(),
			},
			FileDirective::Patch { file_path, .. } => DirectiveKind::Patch {
				file_path: file_path.clone(),
			},
			FileDirective::Rename { from_path, to_path } => DirectiveKind::Rename {
				from_path: from_path.clone(),
				file_path: to_path.clone(),
			},
			FileDirective::Delete { file_path } => DirectiveKind::Delete {
				file_path: file_path.clone(),
			},
			FileDirective::Fail {
				kind,
				file_path,
				error_msg: msg,
			} => {
				error_msg = Some(msg.clone());
				DirectiveKind::Fail {
					kind_str: kind.clone(),
					file_path: file_path.clone(),
				}
			}
		};

		Self {
			kind,
			success: false,
			error_msg,
		}
	}
}

// endregion: --- Froms

#[derive(Debug, Clone)]
pub enum FileDirective {
	New {
		file_path: String,
		content: Content,
	},
	Patch {
		file_path: String,
		content: Content,
	},
	Rename {
		from_path: String,
		to_path: String,
	},
	Delete {
		file_path: String,
	},

	Fail {
		kind: String,
		file_path: Option<String>,
		error_msg: String,
	},
}

#[derive(Debug, Clone)]
pub struct Content {
	pub content: String,
	pub code_fence: Option<CodeFence>,
}

#[derive(Debug, Clone)]
pub struct CodeFence {
	pub start: String,
	pub end: String,
}

impl Content {
	pub fn from_raw(raw: String) -> Self {
		let trimmed_start = raw.trim_start();
		if trimmed_start.starts_with("```") {
			if let Some(f_idx) = trimmed_start.find('\n') {
				let start_fence = trimmed_start[..f_idx].trim().to_string();
				let remaining = &trimmed_start[f_idx + 1..];
				let trimmed_end = remaining.trim_end();
				if trimmed_end.ends_with("```") {
					if let Some(l_idx) = trimmed_end.rfind('\n') {
						let last_line = &trimmed_end[l_idx + 1..];
						if last_line.trim().starts_with("```") {
							let end_fence = last_line.trim().to_string();
							let content = trimmed_end[..l_idx].to_string();
							return Self {
								content,
								code_fence: Some(CodeFence {
									start: start_fence,
									end: end_fence,
								}),
							};
						}
					}
				}
			}
		}
		Self {
			content: raw,
			code_fence: None,
		}
	}
}

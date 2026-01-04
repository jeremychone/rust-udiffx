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
		let mut raw = raw;
		if let Some(stripped) = raw.strip_prefix('\n') {
			raw = stripped.to_string();
		}

		let trimmed_start = raw.trim_start();
		if trimmed_start.starts_with("```")
			&& let Some(f_idx) = trimmed_start.find('\n')
		{
			let start_fence = trimmed_start[..f_idx].to_string();
			let remaining = &trimmed_start[f_idx + 1..];
			let trimmed_end = remaining.trim_end();

			if trimmed_end.ends_with("```")
				&& let Some(l_idx) = trimmed_end.rfind('\n')
			{
				let last_line = &trimmed_end[l_idx + 1..];
				if last_line.trim_start().starts_with("```") {
					let end_fence = last_line.to_string();
					let mut content = remaining[..l_idx + 1].to_string();

					// Note: We also strip the first newline if it exists inside the code fence,
					//       to match the behavior of non-fenced content where one level of newlines is removed.
					if let Some(stripped) = content.strip_prefix('\n') {
						content = stripped.to_string();
					}

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

		Self {
			content: raw,
			code_fence: None,
		}
	}
}

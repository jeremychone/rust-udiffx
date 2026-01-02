use crate::{ApplyChangesInfo, DirectiveInfo, FileChanges, FileDirective, Result, fs_guard};
use diffy::{Patch, apply};
use simple_fs::{SPath, ensure_file_dir, read_to_string};
use std::fs;

/// Executes the file changes defined in `AipFileChanges` relative to `base_dir`.
pub fn apply_file_changes(base_dir: &SPath, file_changes: FileChanges) -> Result<ApplyChangesInfo> {
	let mut infos = Vec::new();

	for directive in file_changes {
		let mut info = DirectiveInfo::from(&directive);

		let res: Result<()> = (|| {
			match directive {
				FileDirective::New { file_path, content } => {
					let full_path = base_dir.join(&file_path);
					fs_guard::check_for_write(&full_path, base_dir)?;

					ensure_file_dir(&full_path)?;
					fs::write(&full_path, &content.content)?;
				}

				FileDirective::Patch {
					file_path,
					content: patch_content,
				} => {
					let full_path = base_dir.join(&file_path);
					fs_guard::check_for_read(&full_path, base_dir)?;
					fs_guard::check_for_write(&full_path, base_dir)?;

					let original_content = read_to_string(&full_path)?;
					let patch_obj = Patch::from_str(&patch_content.content)?;
					let new_content = apply(&original_content, &patch_obj)?;

					fs::write(&full_path, new_content)?;
				}

				FileDirective::Rename { from_path, to_path } => {
					let full_from = base_dir.join(&from_path);
					let full_to = base_dir.join(&to_path);

					fs_guard::check_for_read(&full_from, base_dir)?;
					fs_guard::check_for_write(&full_to, base_dir)?;

					if full_from.exists() {
						ensure_file_dir(&full_to)?;
						fs::rename(&full_from, &full_to)?;
					} else {
						return Err(format!("Rename Source path '{from_path}' not found").into());
					}
				}

				FileDirective::Delete { file_path } => {
					let full_path = base_dir.join(&file_path);
					fs_guard::check_for_write(&full_path, base_dir)?;

					if full_path.exists() {
						if full_path.is_dir() {
							fs::remove_dir_all(&full_path)?;
						} else {
							fs::remove_file(&full_path)?;
						}
					} else {
						return Err(format!("Delete path '{file_path}' not found").into());
					}
				}

				FileDirective::Fail { error_msg, .. } => {
					return Err(error_msg.into());
				}
			}
			Ok(())
		})();

		match res {
			Ok(_) => info.success = true,
			Err(err) => info.error_msg = Some(err.to_string()),
		}

		infos.push(info);
	}

	Ok(ApplyChangesInfo { infos })
}

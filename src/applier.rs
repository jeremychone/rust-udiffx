use crate::{
    fs_guard, patch_completer, ApplyChangesStatus, DirectiveStatus, Error, FileChanges,
    FileDirective, MatchTier, Result,
};
use diffy::{apply, Patch};
use simple_fs::{ensure_file_dir, read_to_string, safer_trash_dir, safer_trash_file, SPath};
use std::fs;

/// Executes the file changes defined in `AipFileChanges` relative to `base_dir`.
pub fn apply_file_changes(
    base_dir: impl Into<SPath>,
    file_changes: FileChanges,
) -> Result<ApplyChangesStatus> {
    let base_dir = base_dir.into();
    // -- Safety check: base_dir must be within CWD
    let cwd = std::env::current_dir().map_err(|err| Error::io_read_file(".", err))?;
    let cwd_spath = SPath::from_std_path(cwd)?.into_collapsed();

    let base_dir = if base_dir.is_absolute() {
        base_dir.clone().into_collapsed()
    } else {
        cwd_spath.join(base_dir).into_collapsed()
    };

    if !base_dir.as_str().starts_with(cwd_spath.as_str()) {
        return Err(Error::security_violation(
            base_dir.to_string(),
            cwd_spath.to_string(),
        ));
    }

    let mut items = Vec::new();

    for directive in file_changes {
        let mut info = DirectiveStatus::from(&directive);

        let res: Result<()> = (|| {
            match directive {
                FileDirective::New { file_path, content } => {
                    let full_path = base_dir.join(&file_path);
                    fs_guard::check_for_write(&full_path, &base_dir)?;

                    ensure_file_dir(&full_path).map_err(Error::simple_fs)?;

                    if full_path.exists() {
                        let existing_content =
                            read_to_string(&full_path).map_err(Error::simple_fs)?;
                        if existing_content == content.content {
                            return Err(Error::apply_no_changes(file_path));
                        }
                        fs::write(&full_path, &content.content)
                            .map_err(|err| Error::io_write_file(full_path.to_string(), err))?;
                    } else {
                        fs::write(&full_path, &content.content)
                            .map_err(|err| Error::io_create_file(full_path.to_string(), err))?;
                    }
                }

                FileDirective::Patch {
                    file_path,
                    content: patch_content,
                } => {
                    let full_path = base_dir.join(&file_path);
                    fs_guard::check_for_read(&full_path, &base_dir)?;
                    fs_guard::check_for_write(&full_path, &base_dir)?;

                    let original_content = if full_path.exists() {
                        read_to_string(&full_path).map_err(Error::simple_fs)?
                    } else {
                        String::new()
                    };

                    let (new_content, tier) =
                        apply_patch(&file_path, &original_content, &patch_content.content)?;
                    info.match_tier = tier;

                    if new_content == original_content && full_path.exists() {
                        return Err(Error::apply_no_changes(file_path));
                    }

                    if !full_path.exists() {
                        ensure_file_dir(&full_path).map_err(Error::simple_fs)?;
                    }

                    fs::write(&full_path, new_content)
                        .map_err(|err| Error::io_write_file(full_path.to_string(), err))?;
                }

                FileDirective::HashlinePatch { file_path, edits } => {
                    let full_path = base_dir.join(&file_path);
                    fs_guard::check_for_read(&full_path, &base_dir)?;
                    fs_guard::check_for_write(&full_path, &base_dir)?;

                    let original_content = if full_path.exists() {
                        read_to_string(&full_path).map_err(Error::simple_fs)?
                    } else {
                        String::new()
                    };

                    let res = crate::hashline::apply_hashline_edits(&original_content, edits)
                        .map_err(|e| Error::Custom(e.to_string()))?;

                    if res.content == original_content && full_path.exists() {
                        return Err(Error::apply_no_changes(file_path));
                    }

                    if !full_path.exists() {
                        ensure_file_dir(&full_path).map_err(Error::simple_fs)?;
                    }

                    fs::write(&full_path, res.content)
                        .map_err(|err| Error::io_write_file(full_path.to_string(), err))?;
                }

                FileDirective::Rename { from_path, to_path } => {
                    let full_from = base_dir.join(&from_path);
                    let full_to = base_dir.join(&to_path);

                    fs_guard::check_for_read(&full_from, &base_dir)?;
                    fs_guard::check_for_write(&full_to, &base_dir)?;

                    if full_from.exists() {
                        ensure_file_dir(&full_to).map_err(Error::simple_fs)?;
                        fs::rename(&full_from, &full_to).map_err(|err| {
                            Error::io_rename_path(full_from.to_string(), full_to.to_string(), err)
                        })?;
                    } else {
                        return Err(Error::apply_path_not_found("rename source", from_path));
                    }
                }

                FileDirective::Delete { file_path } => {
                    let full_path = base_dir.join(&file_path);

                    if full_path.exists() {
                        if full_path.is_dir() {
                            safer_trash_dir(&full_path, ()).map_err(|err| {
                                Error::io_delete_dir_all(full_path.to_string(), err)
                            })?;
                        } else {
                            safer_trash_file(&full_path, ())
                                .map_err(|err| Error::io_delete_file(full_path.to_string(), err))?;
                        }
                    } else {
                        return Err(Error::apply_path_not_found("delete", file_path));
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

        items.push(info);
    }

    Ok(ApplyChangesStatus { items })
}

/// Applies a patch content to an original string, handling potential patch completion.
pub fn apply_patch(
    file_path: &str,
    original: &str,
    patch_raw: &str,
) -> Result<(String, Option<MatchTier>)> {
    // Ensure original has a trailing newline (POSIX compliance)
    let mut original_fixed = original.to_string();
    if !original_fixed.is_empty() && !original_fixed.ends_with('\n') {
        original_fixed.push('\n');
    }

    let (completed_patch, tier) = patch_completer::complete(&original_fixed, patch_raw)?;
    let patch_obj = Patch::from_str(&completed_patch)
        .map_err(|err| Error::diffy_parse_patch(file_path, err, &completed_patch))?;
    let new_content = apply(&original_fixed, &patch_obj)
        .map_err(|err| Error::diffy_apply_patch(file_path, err, &completed_patch))?;
    Ok((new_content, tier))
}

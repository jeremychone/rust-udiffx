use crate::{Content, Error, FileChanges, FileDirective, Result};
use markex::tag;

/// Extracts the first `FILE_CHANGES` block from the input string.
pub fn extract_file_changes(
    input: &str,
    extrude_other_content: bool,
) -> Result<(FileChanges, Option<String>)> {
    let parts = tag::extract(input, &["FILE_CHANGES"], extrude_other_content);

    let (tag_elems, extruded) = if extrude_other_content {
        let (elems, s) = parts.into_with_extrude_content();
        (elems, Some(s))
    } else {
        (parts.into_tag_elems(), None)
    };

    let Some(changes_tag) = tag_elems.into_iter().next() else {
        return Ok((FileChanges::new(Vec::new()), extruded));
    };

    let inner_content = changes_tag.content;

    // -- Pre-process to expand potential self-closing tags (since markex might skip them)
    let inner_content = expand_self_closing_tags(inner_content);

    let child_parts = tag::extract(
        &inner_content,
        &[
            "FILE_NEW",
            "FILE_PATCH",
            "FILE_RENAME",
            "FILE_DELETE",
            "FILE_HASHLINE_PATCH",
        ],
        false,
    );

    let mut directives = Vec::new();

    for elem in child_parts.into_tag_elems() {
        let tag_name = elem.tag.clone();
        let mut attrs = elem.attrs.unwrap_or_default();

        // Try to find a path for better reporting if it fails.
        let file_path_attr = attrs
            .get("file_path")
            .or_else(|| attrs.get("to_path"))
            .or_else(|| attrs.get("from_path"))
            .cloned();

        let directive_res = (|| -> Result<FileDirective> {
            match tag_name.as_str() {
                "FILE_NEW" => {
                    let file_path = attrs
                        .remove("file_path")
                        .ok_or_else(|| Error::parse_missing_attribute("FILE_NEW", "file_path"))?;

                    Ok(FileDirective::New {
                        file_path,
                        content: Content::from_raw(elem.content),
                    })
                }
                "FILE_PATCH" => {
                    let file_path = attrs
                        .remove("file_path")
                        .ok_or_else(|| Error::parse_missing_attribute("FILE_PATCH", "file_path"))?;

                    Ok(FileDirective::Patch {
                        file_path,
                        content: Content::from_raw(elem.content),
                    })
                }
                "FILE_HASHLINE_PATCH" => {
                    let file_path = attrs.remove("file_path").ok_or_else(|| {
                        Error::parse_missing_attribute("FILE_HASHLINE_PATCH", "file_path")
                    })?;

                    let lines: Vec<&str> = elem.content.lines().collect();
                    let mut edits = Vec::new();
                    for line in lines {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        let edit =
                            crate::hashline::parse_edit(line).map_err(|e| Error::Custom(e))?;
                        edits.push(edit);
                    }

                    Ok(FileDirective::HashlinePatch { file_path, edits })
                }

                "FILE_RENAME" => {
                    let from_path = attrs.remove("from_path").ok_or_else(|| {
                        Error::parse_missing_attribute("FILE_RENAME", "from_path")
                    })?;
                    let to_path = attrs
                        .remove("to_path")
                        .ok_or_else(|| Error::parse_missing_attribute("FILE_RENAME", "to_path"))?;

                    Ok(FileDirective::Rename { from_path, to_path })
                }
                "FILE_DELETE" => {
                    let file_path = attrs.remove("file_path").ok_or_else(|| {
                        Error::parse_missing_attribute("FILE_DELETE", "file_path")
                    })?;

                    Ok(FileDirective::Delete { file_path })
                }
                _ => Err(Error::parse_unknown_directive_tag(tag_name.to_string())),
            }
        })();

        let directive = match directive_res {
            Ok(d) => d,
            Err(err) => FileDirective::Fail {
                kind: tag_name,
                file_path: file_path_attr,
                error_msg: err.to_string(),
            },
        };

        directives.push(directive);
    }

    Ok((FileChanges::new(directives), extruded))
}

// region:    --- Support

/// Expands self-closing tags like <TAG /> to <TAG></TAG> so markex can find them.
fn expand_self_closing_tags(mut content: String) -> String {
    let tags = [
        "FILE_NEW",
        "FILE_PATCH",
        "FILE_RENAME",
        "FILE_DELETE",
        "FILE_HASHLINE_PATCH",
    ];
    for tag in tags {
        let mut search_pos = 0;
        let tag_pattern = format!("<{tag}");
        while let Some(start_idx) = content[search_pos..].find(&tag_pattern) {
            let start_idx = search_pos + start_idx;
            if let Some(end_idx) = content[start_idx..].find('>') {
                let end_idx = start_idx + end_idx;
                // Check if the tag is self-closing (ends with />)
                let trimmed_part = content[..end_idx].trim_end();
                if trimmed_part.ends_with('/') {
                    let slash_idx = trimmed_part.len() - 1;
                    let expansion = format!("></{tag}>");
                    content.replace_range(slash_idx..end_idx + 1, &expansion);
                    search_pos = slash_idx + expansion.len();
                } else {
                    search_pos = end_idx + 1;
                }
            } else {
                break;
            }
        }
    }
    content
}

// endregion: --- Support

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_hashline_patch() -> Result<()> {
        let input = r#"
<FILE_CHANGES>
<FILE_HASHLINE_PATCH file_path="src/main.rs">
1#ZZ:new content
>+2#ZZ appended
<+3#ZZ prepended
4#ZZ-5#ZZ:replaced
</FILE_HASHLINE_PATCH>
</FILE_CHANGES>
"#;

        let (changes, _) = extract_file_changes(input, false)?;
        let mut iter = changes.into_iter();
        let directive = iter.next().unwrap();

        match &directive {
            FileDirective::HashlinePatch { file_path, edits } => {
                assert_eq!(file_path, "src/main.rs");
                assert_eq!(edits.len(), 4);
            }
            _ => panic!("Expected HashlinePatch, got {:?}", directive),
        }

        Ok(())
    }
}

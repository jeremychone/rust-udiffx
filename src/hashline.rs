use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use twox_hash::XxHash32;

use once_cell::sync::Lazy;

const NIBBLE_STR: &str = "ZPMQVRWSNKTXJBYH";

static DICT: Lazy<Vec<String>> = Lazy::new(|| {
    let mut dict = Vec::with_capacity(256);
    let nibbles: Vec<char> = NIBBLE_STR.chars().collect();
    for i in 0..256 {
        let h = (i >> 4) & 0x0f;
        let l = i & 0x0f;
        dict.push(format!("{}{}", nibbles[h], nibbles[l]));
    }
    dict
});

static RE_WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static RE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*[>+-]*\s*(\d+)\s*#\s*([ZPMQVRWSNKTXJBYH]{2})").unwrap());
static RE_CONTINUATION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:&&|\|\||\?\?|\?|:|=|,|\+|-|\*|/|\.|\()\s*$").unwrap());
static RE_MERGE_OPS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[|&?]").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineTag {
    pub line: usize,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashlineEdit {
    Set {
        tag: LineTag,
        content: Vec<String>,
    },
    Replace {
        first: LineTag,
        last: LineTag,
        content: Vec<String>,
    },
    Append {
        after: Option<LineTag>,
        content: Vec<String>,
    },
    Prepend {
        before: Option<LineTag>,
        content: Vec<String>,
    },
    Insert {
        after: LineTag,
        before: LineTag,
        content: Vec<String>,
    },
}

pub fn compute_line_hash(_idx: usize, line: &str) -> String {
    let line = if line.ends_with('\r') {
        &line[..line.len() - 1]
    } else {
        line
    };

    let normalized = RE_WHITESPACE.replace_all(line, "");

    let mut hasher = XxHash32::with_seed(0);
    hasher.write(normalized.as_bytes());
    let hash = hasher.finish() as u32;

    DICT[(hash & 0xff) as usize].clone()
}

pub fn format_line_tag(line: usize, content: &str) -> String {
    format!("{}#{}", line, compute_line_hash(line, content))
}

pub fn format_hash_lines(content: &str, start_line: usize) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut formatted = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let num = start_line + i;
        formatted.push(format!("{}:{line}", format_line_tag(num, line)));
    }
    formatted.join("\n")
}

pub fn parse_tag(ref_str: &str) -> Result<LineTag, String> {
    let caps = RE_TAG.captures(ref_str).ok_or_else(|| {
        format!(
            "Invalid line reference \"{}\". Expected format \"LINE#ID\" (e.g. \"5#aa\").",
            ref_str
        )
    })?;

    let line = caps[1]
        .parse::<usize>()
        .map_err(|_| "Invalid line number".to_string())?;
    if line < 1 {
        return Err(format!(
            "Line number must be >= 1, got {} in \"{}\".",
            line, ref_str
        ));
    }

    Ok(LineTag {
        line,
        hash: caps[2].to_string(),
    })
}

pub fn parse_edit(line: &str) -> Result<HashlineEdit, String> {
    let line = line.trim();

    // Format: LINE#ID-LINE#ID:CONTENT or LINE#ID:CONTENT
    if let Some(colon_idx) = line.find(':') {
        let ref_part = line[..colon_idx].trim();
        let content_part = &line[colon_idx + 1..];
        let content = vec![content_part.to_string()];

        if let Some(dash_idx) = ref_part.find('-') {
            let first_ref = &ref_part[..dash_idx];
            let last_ref = &ref_part[dash_idx + 1..];
            return Ok(HashlineEdit::Replace {
                first: parse_tag(first_ref)?,
                last: parse_tag(last_ref)?,
                content,
            });
        } else {
            return Ok(HashlineEdit::Set {
                tag: parse_tag(ref_part)?,
                content,
            });
        }
    }

    // Format: >+LINE#ID CONTENT (Append after)
    if let Some(stripped) = line.strip_prefix(">+") {
        let stripped = stripped.trim();
        if let Some(space_idx) = stripped.find(' ') {
            let ref_part = &stripped[..space_idx];
            let content_part = &stripped[space_idx + 1..];
            return Ok(HashlineEdit::Append {
                after: Some(parse_tag(ref_part)?),
                content: vec![content_part.to_string()],
            });
        }
    }

    // Format: <+LINE#ID CONTENT (Prepend before)
    if let Some(stripped) = line.strip_prefix("<+") {
        let stripped = stripped.trim();
        if let Some(space_idx) = stripped.find(' ') {
            let ref_part = &stripped[..space_idx];
            let content_part = &stripped[space_idx + 1..];
            return Ok(HashlineEdit::Prepend {
                before: Some(parse_tag(ref_part)?),
                content: vec![content_part.to_string()],
            });
        }
    }

    Err(format!("Could not parse hashline edit: {}", line))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashMismatch {
    pub line: usize,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug)]
pub struct HashlineMismatchError {
    pub mismatches: Vec<HashMismatch>,
    pub file_lines: Vec<String>,
}

impl std::fmt::Display for HashlineMismatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut mismatch_set = HashMap::new();
        for m in &self.mismatches {
            mismatch_set.insert(m.line, m);
        }

        const MISMATCH_CONTEXT: usize = 2;
        let mut display_lines = HashSet::new();
        for m in &self.mismatches {
            let lo = if m.line > MISMATCH_CONTEXT {
                m.line - MISMATCH_CONTEXT
            } else {
                1
            };
            let hi = if m.line + MISMATCH_CONTEXT > self.file_lines.len() {
                self.file_lines.len()
            } else {
                m.line + MISMATCH_CONTEXT
            };
            for i in lo..=hi {
                display_lines.insert(i);
            }
        }

        let mut sorted: Vec<usize> = display_lines.into_iter().collect();
        sorted.sort();

        writeln!(f, "{} line{} changed since last read. Use the updated LINE#ID references shown below (>>> marks changed lines).", 
            self.mismatches.len(), 
            if self.mismatches.len() > 1 { "s have" } else { " has" })?;
        writeln!(f)?;

        let mut prev_line: i32 = -1;
        for line_num in sorted {
            if prev_line != -1 && line_num as i32 > prev_line + 1 {
                writeln!(f, "    ...")?;
            }
            prev_line = line_num as i32;

            let content = &self.file_lines[line_num - 1];
            let hash = compute_line_hash(line_num, content);
            let prefix = format!("{}#{}", line_num, hash);

            if mismatch_set.contains_key(&line_num) {
                writeln!(f, ">>> {}:{}", prefix, content)?;
            } else {
                writeln!(f, "    {}:{}", prefix, content)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for HashlineMismatchError {}

pub fn validate_line_ref(
    tag: &LineTag,
    file_lines: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if tag.line < 1 || tag.line > file_lines.len() {
        return Err(format!(
            "Line {} does not exist (file has {} lines)",
            tag.line,
            file_lines.len()
        )
        .into());
    }
    let actual_hash = compute_line_hash(tag.line, &file_lines[tag.line - 1]);
    if actual_hash != tag.hash {
        return Err(Box::new(HashlineMismatchError {
            mismatches: vec![HashMismatch {
                line: tag.line,
                expected: tag.hash.clone(),
                actual: actual_hash,
            }],
            file_lines: file_lines.to_vec(),
        }));
    }
    Ok(())
}

pub struct HashlineStreamOptions {
    pub start_line: Option<usize>,
    pub max_chunk_lines: Option<usize>,
    pub max_chunk_bytes: Option<usize>,
}

impl Default for HashlineStreamOptions {
    fn default() -> Self {
        Self {
            start_line: Some(1),
            max_chunk_lines: Some(200),
            max_chunk_bytes: Some(64 * 1024),
        }
    }
}

pub fn stream_hash_lines_from_lines<I>(
    lines: I,
    options: HashlineStreamOptions,
) -> impl Iterator<Item = String>
where
    I: IntoIterator<Item = String>,
{
    let mut line_num = options.start_line.unwrap_or(1);
    let max_chunk_lines = options.max_chunk_lines.unwrap_or(200);
    let max_chunk_bytes = options.max_chunk_bytes.unwrap_or(64 * 1024);

    let mut out_lines = Vec::new();
    let mut out_bytes = 0;
    let mut saw_any_line = false;

    let mut lines_iter = lines.into_iter();
    let mut result_chunks = Vec::new();

    loop {
        match lines_iter.next() {
            Some(line) => {
                saw_any_line = true;
                let formatted = format!("{}:{line}", format_line_tag(line_num, &line));
                line_num += 1;

                let sep_bytes = if out_lines.is_empty() { 0 } else { 1 };
                let line_bytes = formatted.len();

                if !out_lines.is_empty()
                    && (out_lines.len() >= max_chunk_lines
                        || out_bytes + sep_bytes + line_bytes > max_chunk_bytes)
                {
                    result_chunks.push(out_lines.join("\n"));
                    out_lines.clear();
                    out_bytes = 0;
                }

                out_bytes += (if out_lines.is_empty() { 0 } else { 1 }) + line_bytes;
                out_lines.push(formatted);

                if out_lines.len() >= max_chunk_lines || out_bytes >= max_chunk_bytes {
                    result_chunks.push(out_lines.join("\n"));
                    out_lines.clear();
                    out_bytes = 0;
                }
            }
            None => {
                if !saw_any_line {
                    let formatted = format!("{}:", format_line_tag(line_num, ""));
                    result_chunks.push(formatted);
                } else if !out_lines.is_empty() {
                    result_chunks.push(out_lines.join("\n"));
                }
                break;
            }
        }
    }
    result_chunks.into_iter()
}

pub fn stream_hash_lines_from_utf8(
    source: &[u8],
    options: HashlineStreamOptions,
) -> impl Iterator<Item = String> {
    let content = String::from_utf8_lossy(source);
    let lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    stream_hash_lines_from_lines(lines, options)
}

fn strip_all_whitespace(s: &str) -> String {
    RE_WHITESPACE.replace_all(s, "").to_string()
}

fn strip_trailing_continuation_tokens(s: &str) -> String {
    RE_CONTINUATION.replace_all(s, "").to_string()
}

fn strip_merge_operator_chars(s: &str) -> String {
    RE_MERGE_OPS.replace_all(s, "").to_string()
}

fn leading_whitespace(s: &str) -> &str {
    s.find(|c: char| !c.is_whitespace())
        .map(|idx| &s[..idx])
        .unwrap_or(s)
}

fn restore_leading_indent(template_line: &str, line: &str) -> String {
    if line.is_empty() {
        return line.to_string();
    }
    let template_indent = leading_whitespace(template_line);
    if template_indent.is_empty() {
        return line.to_string();
    }
    let indent = leading_whitespace(line);
    if !indent.is_empty() {
        return line.to_string();
    }
    format!("{}{}", template_indent, line)
}

fn restore_indent_for_paired_replacement(
    old_lines: &[String],
    new_lines: &[String],
) -> Vec<String> {
    if old_lines.len() != new_lines.len() {
        return new_lines.to_vec();
    }
    new_lines
        .iter()
        .enumerate()
        .map(|(i, line)| restore_leading_indent(&old_lines[i], line))
        .collect()
}

fn restore_old_wrapped_lines(old_lines: &[String], new_lines: &[String]) -> Vec<String> {
    if old_lines.is_empty() || new_lines.len() < 2 {
        return new_lines.to_vec();
    }

    let mut canon_to_old: HashMap<String, (String, usize)> = HashMap::new();
    for line in old_lines {
        let canon = strip_all_whitespace(line);
        let entry = canon_to_old.entry(canon).or_insert((line.clone(), 0));
        entry.1 += 1;
    }

    let mut candidates = Vec::new();
    for start in 0..new_lines.len() {
        for len in 2..=10 {
            if start + len > new_lines.len() {
                break;
            }
            let combined = new_lines[start..start + len].join("");
            let canon_span = strip_all_whitespace(&combined);
            if let Some((old_line, count)) = canon_to_old.get(&canon_span) {
                if *count == 1 && canon_span.len() >= 6 {
                    candidates.push((start, len, old_line.clone(), canon_span));
                }
            }
        }
    }

    if candidates.is_empty() {
        return new_lines.to_vec();
    }

    let mut canon_counts = HashMap::new();
    for (_, _, _, canon) in &candidates {
        *canon_counts.entry(canon.clone()).or_insert(0) += 1;
    }

    let mut unique_candidates: Vec<_> = candidates
        .into_iter()
        .filter(|(_, _, _, canon)| *canon_counts.get(canon).unwrap() == 1)
        .collect();

    if unique_candidates.is_empty() {
        return new_lines.to_vec();
    }

    unique_candidates.sort_by(|a, b| b.0.cmp(&a.0));

    let mut out = new_lines.to_vec();
    for (start, len, replacement, _) in unique_candidates {
        out.splice(start..start + len, std::iter::once(replacement));
    }
    out
}

fn strip_insert_anchor_echo_after(anchor_line: &str, dst_lines: &[String]) -> Vec<String> {
    if dst_lines.len() <= 1 {
        return dst_lines.to_vec();
    }
    if equals_ignoring_whitespace(&dst_lines[0], anchor_line) {
        return dst_lines[1..].to_vec();
    }
    dst_lines.to_vec()
}

fn strip_insert_anchor_echo_before(anchor_line: &str, dst_lines: &[String]) -> Vec<String> {
    if dst_lines.len() <= 1 {
        return dst_lines.to_vec();
    }
    if equals_ignoring_whitespace(&dst_lines[dst_lines.len() - 1], anchor_line) {
        return dst_lines[..dst_lines.len() - 1].to_vec();
    }
    dst_lines.to_vec()
}

fn strip_insert_boundary_echo(
    after_line: &str,
    before_line: &str,
    dst_lines: &[String],
) -> Vec<String> {
    let mut out = dst_lines.to_vec();
    if out.len() > 1 && equals_ignoring_whitespace(&out[0], after_line) {
        out.remove(0);
    }
    if out.len() > 1 && equals_ignoring_whitespace(&out[out.len() - 1], before_line) {
        out.pop();
    }
    out
}

fn strip_range_boundary_echo(
    file_lines: &[String],
    start_line: usize,
    end_line: usize,
    dst_lines: &[String],
) -> Vec<String> {
    let count = end_line - start_line + 1;
    if dst_lines.len() <= 1 || dst_lines.len() <= count {
        return dst_lines.to_vec();
    }

    let mut out = dst_lines.to_vec();
    let before_idx = start_line as i32 - 2;
    if before_idx >= 0 && equals_ignoring_whitespace(&out[0], &file_lines[before_idx as usize]) {
        out.remove(0);
    }

    let after_idx = end_line;
    if after_idx < file_lines.len()
        && !out.is_empty()
        && equals_ignoring_whitespace(&out[out.len() - 1], &file_lines[after_idx])
    {
        out.pop();
    }

    out
}

fn equals_ignoring_whitespace(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    strip_all_whitespace(a) == strip_all_whitespace(b)
}

pub struct ApplyHashlineResult {
    pub content: String,
    pub first_changed_line: Option<usize>,
    pub noop_edits: Vec<NoopEdit>,
}

#[derive(Debug)]
pub struct NoopEdit {
    pub edit_index: usize,
    pub loc: String,
    pub current_content: String,
}

pub fn apply_hashline_edits(
    content: &str,
    mut edits: Vec<HashlineEdit>,
) -> Result<ApplyHashlineResult, Box<dyn std::error::Error>> {
    if edits.is_empty() {
        return Ok(ApplyHashlineResult {
            content: content.to_string(),
            first_changed_line: None,
            noop_edits: Vec::new(),
        });
    }

    let mut file_lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let original_file_lines = file_lines.clone();
    let mut first_changed_line: Option<usize> = None;
    let mut noop_edits = Vec::new();

    let autocorrect = true;

    let mut explicitly_touched_lines = HashSet::new();
    for edit in &edits {
        match edit {
            HashlineEdit::Set { tag, .. } => {
                explicitly_touched_lines.insert(tag.line);
            }
            HashlineEdit::Replace { first, last, .. } => {
                for ln in first.line..=last.line {
                    explicitly_touched_lines.insert(ln);
                }
            }
            HashlineEdit::Append { after, .. } => {
                if let Some(tag) = after {
                    explicitly_touched_lines.insert(tag.line);
                }
            }
            HashlineEdit::Prepend { before, .. } => {
                if let Some(tag) = before {
                    explicitly_touched_lines.insert(tag.line);
                }
            }
            HashlineEdit::Insert { after, before, .. } => {
                explicitly_touched_lines.insert(after.line);
                explicitly_touched_lines.insert(before.line);
            }
        }
    }

    let mut mismatches = Vec::new();
    let mut validate_ref = |tag: &LineTag| {
        if tag.line < 1 || tag.line > file_lines.len() {
            return Err(format!(
                "Line {} does not exist (file has {} lines)",
                tag.line,
                file_lines.len()
            ));
        }
        let actual_hash = compute_line_hash(tag.line, &file_lines[tag.line - 1]);
        if actual_hash == tag.hash {
            Ok(true)
        } else {
            mismatches.push(HashMismatch {
                line: tag.line,
                expected: tag.hash.clone(),
                actual: actual_hash,
            });
            Ok(false)
        }
    };

    for edit in &edits {
        match edit {
            HashlineEdit::Set { tag, .. } => {
                validate_ref(tag)?;
            }
            HashlineEdit::Append { after, content, .. } => {
                if content.is_empty() {
                    return Err("Insert-after edit requires non-empty dst".into());
                }
                if let Some(tag) = after {
                    validate_ref(tag)?;
                }
            }
            HashlineEdit::Prepend {
                before, content, ..
            } => {
                if content.is_empty() {
                    return Err("Insert-before edit requires non-empty dst".into());
                }
                if let Some(tag) = before {
                    validate_ref(tag)?;
                }
            }
            HashlineEdit::Insert {
                after,
                before,
                content,
                ..
            } => {
                if content.is_empty() {
                    return Err("Insert-between edit requires non-empty dst".into());
                }
                if before.line <= after.line {
                    return Err(format!(
                        "insert requires after ({}) < before ({})",
                        after.line, before.line
                    )
                    .into());
                }
                validate_ref(after)?;
                validate_ref(before)?;
            }
            HashlineEdit::Replace { first, last, .. } => {
                if first.line > last.line {
                    return Err(format!(
                        "Range start line {} must be <= end line {}",
                        first.line, last.line
                    )
                    .into());
                }
                validate_ref(first)?;
                validate_ref(last)?;
            }
        }
    }

    if !mismatches.is_empty() {
        return Err(Box::new(HashlineMismatchError {
            mismatches,
            file_lines: original_file_lines,
        }));
    }

    // Deduplicate identical edits
    let mut seen_edit_keys = HashSet::new();
    let mut to_remove = Vec::new();
    for (i, edit) in edits.iter().enumerate() {
        let line_key = match edit {
            HashlineEdit::Set { tag, .. } => format!("s:{}", tag.line),
            HashlineEdit::Replace { first, last, .. } => format!("r:{}:{}", first.line, last.line),
            HashlineEdit::Append { after, .. } => after
                .as_ref()
                .map(|tag| format!("i:{}", tag.line))
                .unwrap_or_else(|| "ieof".to_string()),
            HashlineEdit::Prepend { before, .. } => before
                .as_ref()
                .map(|tag| format!("ib:{}", tag.line))
                .unwrap_or_else(|| "ibef".to_string()),
            HashlineEdit::Insert { after, before, .. } => {
                format!("ix:{}:{}", after.line, before.line)
            }
        };
        let content = match edit {
            HashlineEdit::Set { content, .. }
            | HashlineEdit::Replace { content, .. }
            | HashlineEdit::Append { content, .. }
            | HashlineEdit::Prepend { content, .. }
            | HashlineEdit::Insert { content, .. } => content.join("\n"),
        };
        let dst_key = format!("{}:{}", line_key, content);
        if seen_edit_keys.contains(&dst_key) {
            to_remove.push(i);
        } else {
            seen_edit_keys.insert(dst_key);
        }
    }
    for &i in to_remove.iter().rev() {
        edits.remove(i);
    }

    #[derive(Eq, PartialEq)]
    struct AnnotatedEdit {
        edit: HashlineEdit,
        idx: usize,
        sort_line: usize,
        precedence: u8,
    }

    impl Ord for AnnotatedEdit {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other
                .sort_line
                .cmp(&self.sort_line)
                .then(self.precedence.cmp(&other.precedence))
                .then(self.idx.cmp(&other.idx))
        }
    }

    impl PartialOrd for AnnotatedEdit {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut annotated: Vec<_> = edits
        .into_iter()
        .enumerate()
        .map(|(idx, edit)| {
            let (sort_line, precedence) = match &edit {
                HashlineEdit::Set { tag, .. } => (tag.line, 0),
                HashlineEdit::Replace { last, .. } => (last.line, 0),
                HashlineEdit::Append { after, .. } => (
                    after
                        .as_ref()
                        .map(|tag| tag.line)
                        .unwrap_or(original_file_lines.len() + 1),
                    1,
                ),
                HashlineEdit::Prepend { before, .. } => {
                    (before.as_ref().map(|tag| tag.line).unwrap_or(0), 2)
                }
                HashlineEdit::Insert { before, .. } => (before.line, 3),
            };
            AnnotatedEdit {
                edit,
                idx,
                sort_line,
                precedence,
            }
        })
        .collect();

    annotated.sort();

    let track_first_changed = |line: usize, first_changed_line: &mut Option<usize>| {
        if first_changed_line.is_none() || Some(line) < *first_changed_line {
            *first_changed_line = Some(line);
        }
    };

    for annot in annotated {
        let edit = annot.edit;
        let idx = annot.idx;
        match edit {
            HashlineEdit::Set { tag, content } => {
                if autocorrect {
                    if let Some(merged) = maybe_expand_single_line_merge(
                        tag.line,
                        &content,
                        &file_lines,
                        &explicitly_touched_lines,
                    ) {
                        let orig_lines = &original_file_lines
                            [merged.start_line - 1..merged.start_line - 1 + merged.delete_count];
                        let mut next_lines = merged.new_lines;
                        next_lines = restore_indent_for_paired_replacement(
                            &[orig_lines[0].clone()],
                            &next_lines,
                        );

                        if orig_lines.iter().eq(next_lines.iter()) {
                            noop_edits.push(NoopEdit {
                                edit_index: idx,
                                loc: format!("{}#{}", tag.line, tag.hash),
                                current_content: orig_lines.join("\n"),
                            });
                            continue;
                        }
                        file_lines.splice(
                            merged.start_line - 1..merged.start_line - 1 + merged.delete_count,
                            next_lines,
                        );
                        track_first_changed(merged.start_line, &mut first_changed_line);
                        continue;
                    }
                }

                let orig_lines = &original_file_lines[tag.line - 1..tag.line];
                let mut stripped = if autocorrect {
                    strip_range_boundary_echo(&original_file_lines, tag.line, tag.line, &content)
                } else {
                    content
                };
                stripped = if autocorrect {
                    restore_old_wrapped_lines(orig_lines, &stripped)
                } else {
                    stripped
                };
                let new_lines = if autocorrect {
                    restore_indent_for_paired_replacement(orig_lines, &stripped)
                } else {
                    stripped
                };
                if orig_lines.iter().eq(new_lines.iter()) {
                    noop_edits.push(NoopEdit {
                        edit_index: idx,
                        loc: format!("{}#{}", tag.line, tag.hash),
                        current_content: orig_lines.join("\n"),
                    });
                    continue;
                }
                file_lines.splice(tag.line - 1..tag.line, new_lines);
                track_first_changed(tag.line, &mut first_changed_line);
            }
            HashlineEdit::Replace {
                first,
                last,
                content,
            } => {
                let count = last.line - first.line + 1;
                let orig_lines = &original_file_lines[first.line - 1..first.line - 1 + count];
                let mut stripped = if autocorrect {
                    strip_range_boundary_echo(&original_file_lines, first.line, last.line, &content)
                } else {
                    content
                };
                stripped = if autocorrect {
                    restore_old_wrapped_lines(orig_lines, &stripped)
                } else {
                    stripped
                };
                let new_lines = if autocorrect {
                    restore_indent_for_paired_replacement(orig_lines, &stripped)
                } else {
                    stripped
                };
                if autocorrect && orig_lines.iter().eq(new_lines.iter()) {
                    noop_edits.push(NoopEdit {
                        edit_index: idx,
                        loc: format!("{}#{}", first.line, first.hash),
                        current_content: orig_lines.join("\n"),
                    });
                    continue;
                }
                file_lines.splice(first.line - 1..first.line - 1 + count, new_lines);
                track_first_changed(first.line, &mut first_changed_line);
            }
            HashlineEdit::Append { after, content } => {
                let inserted = if let Some(tag) = &after {
                    if autocorrect {
                        strip_insert_anchor_echo_after(&original_file_lines[tag.line - 1], &content)
                    } else {
                        content
                    }
                } else {
                    content
                };
                if inserted.is_empty() {
                    noop_edits.push(NoopEdit {
                        edit_index: idx,
                        loc: after
                            .as_ref()
                            .map(|tag| format!("{}#{}", tag.line, tag.hash))
                            .unwrap_or_else(|| "EOF".to_string()),
                        current_content: after
                            .as_ref()
                            .map(|tag| original_file_lines[tag.line - 1].clone())
                            .unwrap_or_default(),
                    });
                    continue;
                }
                if let Some(tag) = after {
                    file_lines.splice(tag.line..tag.line, inserted);
                    track_first_changed(tag.line + 1, &mut first_changed_line);
                } else {
                    if file_lines.len() == 1 && file_lines[0].is_empty() {
                        file_lines.splice(0..1, inserted);
                        track_first_changed(1, &mut first_changed_line);
                    } else {
                        let len = inserted.len();
                        file_lines.extend(inserted);
                        track_first_changed(file_lines.len() - len + 1, &mut first_changed_line);
                    }
                }
            }
            HashlineEdit::Prepend { before, content } => {
                let inserted = if let Some(tag) = &before {
                    if autocorrect {
                        strip_insert_anchor_echo_before(
                            &original_file_lines[tag.line - 1],
                            &content,
                        )
                    } else {
                        content
                    }
                } else {
                    content
                };
                if inserted.is_empty() {
                    noop_edits.push(NoopEdit {
                        edit_index: idx,
                        loc: before
                            .as_ref()
                            .map(|tag| format!("{}#{}", tag.line, tag.hash))
                            .unwrap_or_else(|| "BOF".to_string()),
                        current_content: before
                            .as_ref()
                            .map(|tag| original_file_lines[tag.line - 1].clone())
                            .unwrap_or_default(),
                    });
                    continue;
                }
                if let Some(tag) = before {
                    file_lines.splice(tag.line - 1..tag.line - 1, inserted);
                    track_first_changed(tag.line, &mut first_changed_line);
                } else {
                    if file_lines.len() == 1 && file_lines[0].is_empty() {
                        file_lines.splice(0..1, inserted);
                    } else {
                        file_lines.splice(0..0, inserted);
                    }
                    track_first_changed(1, &mut first_changed_line);
                }
            }
            HashlineEdit::Insert {
                after,
                before,
                content,
            } => {
                let after_line = &original_file_lines[after.line - 1];
                let before_line = &original_file_lines[before.line - 1];
                let inserted = if autocorrect {
                    strip_insert_boundary_echo(after_line, before_line, &content)
                } else {
                    content
                };
                if inserted.is_empty() {
                    noop_edits.push(NoopEdit {
                        edit_index: idx,
                        loc: format!(
                            "{}#{}..{}#{}",
                            after.line, after.hash, before.line, before.hash
                        ),
                        current_content: format!("{}\n{}", after_line, before_line),
                    });
                    continue;
                }
                file_lines.splice(before.line - 1..before.line - 1, inserted);
                track_first_changed(before.line, &mut first_changed_line);
            }
        }
    }

    Ok(ApplyHashlineResult {
        content: file_lines.join("\n"),
        first_changed_line,
        noop_edits,
    })
}

struct MergedExpansion {
    start_line: usize,
    delete_count: usize,
    new_lines: Vec<String>,
}

fn maybe_expand_single_line_merge(
    line: usize,
    content: &[String],
    file_lines: &[String],
    explicitly_touched_lines: &HashSet<usize>,
) -> Option<MergedExpansion> {
    if content.len() != 1 {
        return None;
    }
    if line < 1 || line > file_lines.len() {
        return None;
    }

    let new_line = &content[0];
    let new_canon = strip_all_whitespace(new_line);
    let new_canon_for_merge_ops = strip_merge_operator_chars(&new_canon);
    if new_canon.is_empty() {
        return None;
    }

    let orig = &file_lines[line - 1];
    let orig_canon = strip_all_whitespace(orig);
    let orig_canon_for_match = strip_trailing_continuation_tokens(&orig_canon);
    let orig_canon_for_merge_ops = strip_merge_operator_chars(&orig_canon);
    let orig_looks_like_continuation = orig_canon_for_match.len() < orig_canon.len();
    if orig_canon.is_empty() {
        return None;
    }

    let next_idx = line;
    let prev_idx = line as i32 - 2;

    // Case A: dst absorbed the next continuation line.
    if orig_looks_like_continuation
        && next_idx < file_lines.len()
        && !explicitly_touched_lines.contains(&(line + 1))
    {
        let next = &file_lines[next_idx];
        let next_canon = strip_all_whitespace(next);
        let a = new_canon.find(&orig_canon_for_match);
        let b = new_canon.find(&next_canon);
        if let (Some(ai), Some(bi)) = (a, b) {
            if ai < bi && new_canon.len() <= orig_canon.len() + next_canon.len() + 32 {
                return Some(MergedExpansion {
                    start_line: line,
                    delete_count: 2,
                    new_lines: content.to_vec(),
                });
            }
        }
    }

    // Case B: dst absorbed the previous declaration/continuation line.
    if prev_idx >= 0 && !explicitly_touched_lines.contains(&(line - 1)) {
        let prev = &file_lines[prev_idx as usize];
        let prev_canon = strip_all_whitespace(prev);
        let prev_canon_for_match = strip_trailing_continuation_tokens(&prev_canon);
        let prev_looks_like_continuation = prev_canon_for_match.len() < prev_canon.len();
        if !prev_looks_like_continuation {
            return None;
        }
        let a = new_canon_for_merge_ops.find(&strip_merge_operator_chars(&prev_canon_for_match));
        let b = new_canon_for_merge_ops.find(&orig_canon_for_merge_ops);
        if let (Some(ai), Some(bi)) = (a, b) {
            if ai < bi && new_canon.len() <= prev_canon.len() + orig_canon.len() + 32 {
                return Some(MergedExpansion {
                    start_line: line - 1,
                    delete_count: 2,
                    new_lines: content.to_vec(),
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tag(line: usize, content: &str) -> LineTag {
        parse_tag(&format_line_tag(line, content)).unwrap()
    }

    #[test]
    fn test_compute_line_hash() {
        let hash = compute_line_hash(1, "hello");
        assert!(RE_TAG.is_match(&format!("1#{}", hash)));

        let a = compute_line_hash(1, "hello");
        let b = compute_line_hash(1, "hello");
        assert_eq!(a, b);

        let c = compute_line_hash(1, "world");
        assert_ne!(a, c);

        let empty = compute_line_hash(1, "");
        assert_eq!(empty.len(), 2);
    }

    #[test]
    fn test_format_hash_lines() {
        let result = format_hash_lines("hello", 1);
        let hash = compute_line_hash(1, "hello");
        assert_eq!(result, format!("1#{}:hello", hash));

        let result = format_hash_lines("foo\nbar\nbaz", 1);
        let lines: Vec<&str> = result.split('\n').collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("1#"));
        assert!(lines[1].starts_with("2#"));
        assert!(lines[2].starts_with("3#"));

        let result = format_hash_lines("foo\nbar", 10);
        let lines: Vec<&str> = result.split('\n').collect();
        assert!(lines[0].starts_with("10#"));
        assert!(lines[1].starts_with("11#"));
    }

    #[test]
    fn test_parse_tag() {
        let ref_tag = parse_tag("5#QQ").unwrap();
        assert_eq!(ref_tag.line, 5);
        assert_eq!(ref_tag.hash, "QQ");

        assert!(parse_tag("1#Q").is_err());

        let ref_tag = parse_tag("100#QQQQ").unwrap();
        assert_eq!(ref_tag.line, 100);
        assert_eq!(ref_tag.hash, "QQ");

        assert!(parse_tag("5QQ").is_err());
        assert!(parse_tag("abc#Q").is_err());
        assert!(parse_tag("0#QQ").is_err());
        assert!(parse_tag("").is_err());
    }

    #[test]
    fn test_apply_hashline_edits_replace() {
        let content = "aaa\nbbb\nccc";
        let edits = vec![HashlineEdit::Set {
            tag: make_tag(2, "bbb"),
            content: vec!["BBB".to_string()],
        }];

        let result = apply_hashline_edits(content, edits).unwrap();
        assert_eq!(result.content, "aaa\nBBB\nccc");
        assert_eq!(result.first_changed_line, Some(2));
    }

    #[test]
    fn test_apply_hashline_edits_insert_after() {
        let content = "aaa\nbbb\nccc";
        let edits = vec![HashlineEdit::Append {
            after: Some(make_tag(1, "aaa")),
            content: vec!["NEW".to_string()],
        }];

        let result = apply_hashline_edits(content, edits).unwrap();
        assert_eq!(result.content, "aaa\nNEW\nbbb\nccc");
        assert_eq!(result.first_changed_line, Some(2));
    }

    #[test]
    fn test_apply_hashline_edits_delete() {
        let content = "aaa\nbbb\nccc";
        let edits = vec![HashlineEdit::Set {
            tag: make_tag(2, "bbb"),
            content: vec![],
        }];

        let result = apply_hashline_edits(content, edits).unwrap();
        assert_eq!(result.content, "aaa\nccc");
        assert_eq!(result.first_changed_line, Some(2));
    }

    #[test]
    fn test_apply_hashline_edits_multiple() {
        let content = "aaa\nbbb\nccc\nddd\neee";
        let edits = vec![
            HashlineEdit::Set {
                tag: make_tag(2, "bbb"),
                content: vec!["BBB".to_string()],
            },
            HashlineEdit::Set {
                tag: make_tag(4, "ddd"),
                content: vec!["DDD".to_string()],
            },
        ];

        let result = apply_hashline_edits(content, edits).unwrap();
        assert_eq!(result.content, "aaa\nBBB\nccc\nDDD\neee");
        assert_eq!(result.first_changed_line, Some(2));
    }

    #[test]
    fn test_autocorrect_strip_anchor_echo() {
        let content = "aaa\nbbb\nccc";
        let edits = vec![HashlineEdit::Append {
            after: Some(make_tag(2, "bbb")),
            content: vec!["bbb".to_string(), "NEW".to_string()],
        }];

        let result = apply_hashline_edits(content, edits).unwrap();
        assert_eq!(result.content, "aaa\nbbb\nNEW\nccc");
    }

    #[test]
    fn test_autocorrect_merge_next_line() {
        let content = "line1 &&\nline2\ntail";
        let edits = vec![HashlineEdit::Set {
            tag: make_tag(1, "line1 &&"),
            content: vec!["line1 || line2".to_string()],
        }];

        let result = apply_hashline_edits(content, edits).unwrap();
        assert_eq!(result.content, "line1 || line2\ntail");
    }
}

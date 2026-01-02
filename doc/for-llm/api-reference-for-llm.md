# udiffx, for-llm api reference

- crate: `udiffx`
- version: `0.1.0-alpha.1`
- purpose: parse and apply an AI-optimized "file changes" envelope (XML-like tags + unified diff patches)

## Envelope format (the only thing to parse)

- exactly one root container is expected when you intend to apply changes:

<FILE_CHANGES>
... directives ...
</FILE_CHANGES>

Inside `<FILE_CHANGES>`, mix any number of directives:

- `<FILE_NEW file_path="..."> ... </FILE_NEW>`
- `<FILE_PATCH file_path="..."> ... </FILE_PATCH>` (Unified Diff content)
- `<FILE_RENAME from_path="..." to_path="..." />`
- `<FILE_DELETE file_path="..." />`

Notes:
- Tags are XML-like, not fully XML-compliant, content does not need XML escaping.
- Self-closing tags like `<FILE_DELETE ... />` and `<FILE_RENAME ... />` are supported.

## Public API (Rust)

Re-exports (from `udiffx` root):
- `extract_file_changes`
- `apply_file_changes`
- `FileChanges`, `FileDirective`
- `ApplyChangesStatus`, `DirectiveStatus`
- `Error`, `Result<T>`

### Result / Error

- `pub type Result<T> = core::result::Result<T, Error>;`
- `Error` is `Debug + Display`, designed to provide actionable messages, including I/O failures and parsing failures.

### Extract

Signature:

- `pub fn extract_file_changes(input: &str, extract_content: bool) -> Result<(FileChanges, Option<String>)>`

Behavior:
- Finds and parses the first `<FILE_CHANGES> ... </FILE_CHANGES>` block in `input`.
- Returns:
  - `FileChanges` (possibly empty)
  - `extruded: Option<String>`
    - `extract_content = false` => `extruded = None`
    - `extract_content = true` => `extruded = Some(input_without_first_file_changes_block)`

Directive parsing:
- Recognized child tags: `FILE_NEW`, `FILE_PATCH`, `FILE_RENAME`, `FILE_DELETE`
- Missing required attributes or unknown directive tags produce `FileDirective::Fail { ... }` entries (instead of failing extraction entirely).

Example:

````rust
use udiffx::{extract_file_changes, Result};

fn main() -> Result<()> {
    let input = r#"
Some text...

<FILE_CHANGES>
<FILE_NEW file_path="src/hello.rs">
pub fn hello() { println!("Hello"); }
</FILE_NEW>

<FILE_DELETE file_path="old.txt" />
</FILE_CHANGES>
"#;

    let (changes, extruded) = extract_file_changes(input, true)?;

    assert!(!changes.is_empty());
    assert!(extruded.is_some());

    for d in &changes {
        println!("{d:?}");
    }

    Ok(())
}
````

### FileChanges

Type:
- `pub struct FileChanges { directives: Vec<FileDirective>, ... }`

Key methods:
- `pub fn new(directives: Vec<FileDirective>) -> Self`
- `pub fn is_empty(&self) -> bool`
- `pub fn iter(&self) -> std::slice::Iter<'_, FileDirective>`

Iteration:
- `impl IntoIterator for FileChanges` yields owned `FileDirective`
- `impl IntoIterator for &FileChanges` yields `&FileDirective`

### FileDirective

Type:

- `pub enum FileDirective { New { file_path, content }, Patch { file_path, content }, Rename { from_path, to_path }, Delete { file_path }, Fail { kind, file_path, error_msg } }`

Semantics:
- `New`: write full content to `file_path` (create or overwrite)
- `Patch`: apply unified diff patch to existing file at `file_path`
- `Rename`: rename/move from `from_path` to `to_path`
- `Delete`: delete file or directory at `file_path` (recursive for dirs)
- `Fail`: represents a parsing failure for a directive, it is still part of the `FileChanges`

### Content (for New/Patch)

Type:
- `pub struct Content { pub content: String, pub code_fence: Option<CodeFence> }`
- `pub struct CodeFence { pub start: String, pub end: String }`

Behavior:
- `Content::from_raw(raw)` strips an outer markdown fence if present:
  - leading line starts with ```...
  - trailing line starts with ```
  - stores fences in `code_fence`
  - stores inner payload in `content`

### Apply

Signature:

- `pub fn apply_file_changes(base_dir: &simple_fs::SPath, file_changes: FileChanges) -> Result<ApplyChangesStatus>`

Core rules:
- All directive paths are interpreted as relative to `base_dir`.
- Path security guard is enforced:
  - operations must stay within `base_dir` (collapsed path check)
- Patch application:
  - uses `diffy` to parse/apply unified diff patches

Directive behavior:
- `FILE_NEW`
  - ensures parent directory exists
  - writes content to file (create or overwrite)
- `FILE_PATCH`
  - reads the file
  - parses patch via `diffy::Patch::from_str`
  - applies patch via `diffy::apply`
  - writes updated file content
- `FILE_RENAME`
  - ensures destination parent directory exists
  - renames from -> to
- `FILE_DELETE`
  - deletes file or deletes dir recursively
- `Fail`
  - always treated as failure for that directive when applying

Return value:
- Apply returns `Ok(ApplyChangesStatus)` even if some directives failed.
- Per-directive failures are captured in the returned status (not by returning `Err`), except catastrophic errors that prevent completing the loop.

Example:

````rust
use simple_fs::SPath;
use udiffx::{apply_file_changes, extract_file_changes, Result};

fn main() -> Result<()> {
    let base_dir = SPath::new("./my-project");

    let input = r#"
<FILE_CHANGES>
<FILE_PATCH file_path="src/main.rs">
@@ -1,3 +1,3 @@
-fn main() { println!("Hello"); }
+fn main() { println!("Hello, world"); }
</FILE_PATCH>
</FILE_CHANGES>
"#;

    let (changes, _) = extract_file_changes(input, false)?;
    let status = apply_file_changes(&base_dir, changes)?;

    for info in status.infos {
        if info.success() {
            println!("OK   {} {}", info.kind(), info.file_path());
        } else {
            println!(
                "FAIL {} {}: {}",
                info.kind(),
                info.file_path(),
                info.error_msg().unwrap_or("unknown error"),
            );
        }
    }

    Ok(())
}
````

### ApplyChangesStatus / DirectiveStatus

Types:
- `pub struct ApplyChangesStatus { pub infos: Vec<DirectiveStatus> }`
- `pub struct DirectiveStatus { pub kind: DirectiveKind, pub success: bool, pub error_msg: Option<String> }`

Helpers:
- `DirectiveStatus::file_path(&self) -> &str`
- `DirectiveStatus::success(&self) -> bool`
- `DirectiveStatus::error_msg(&self) -> Option<&str>`
- `DirectiveStatus::kind(&self) -> &'static str` in `{ "New" | "Patch" | "Rename" | "Delete" | "Fail" }`

## Recommended LLM output patterns (strict)

- Emit exactly one `<FILE_CHANGES>` block when output is meant to be applied.
- Prefer `FILE_PATCH` for small edits to large files.
- Use self-closing tags when possible for rename and delete:
  - `<FILE_RENAME from_path="a" to_path="b" />`
  - `<FILE_DELETE file_path="path" />`
- For `FILE_PATCH` content, use valid unified diff hunks starting with `@@ ... @@`.
- `file_path` values should be relative paths (no traversal), to ensure base-dir guard passes.

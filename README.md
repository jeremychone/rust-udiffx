# udiffx

Parse and apply an AI-optimized "file changes" envelope that carries multiple file operations in a single block, using unified diff patches for updates.

This crate is designed for LLM output that needs to be machine-parsable and efficient for large files with small edits.

## Concept, `FILE_CHANGES`

A response contains one root container:

- `<FILE_CHANGES> ... </FILE_CHANGES>`

Inside it, you can mix multiple directives:

- `<FILE_NEW file_path="..."> ... </FILE_NEW>`
- `<FILE_PATCH file_path="..."> ... </FILE_PATCH>` (Unified Diff content)
- `<FILE_RENAME from_path="..." to_path="..." />`
- `<FILE_DELETE file_path="..." />`

Notes:

- Tags are XML-like but not intended to be strictly XML compliant.
- The parser is tag-based, it extracts only the above tags, content does not need XML escaping.
- Self-closing tags like `<FILE_DELETE ... />` are supported.

## API overview

The crate exposes two main operations:

- Extract: parse the first `<FILE_CHANGES>` block from a string.
- Apply: execute the extracted directives against a base directory.

Key public types:

- `FileChanges`, iterable list of directives.
- `FileDirective`, one directive (new, patch, rename, delete, fail).
- `ApplyChangesStatus`, per-directive success and error reporting.
- `Error` / `Result<T>`, crate error type and alias.

## Extracting changes from text

Use `extract_file_changes` to parse a model response or any input string.

```rust
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

    let (changes, _extruded) = extract_file_changes(input, false)?;

    if changes.is_empty() {
        println!("No changes found");
        return Ok(());
    }

    for d in &changes {
        println!("{d:?}");
    }

    Ok(())
}
```

`extract_content` parameter:

- `extract_content = false` parses tags, returns `extruded = None`.
- `extract_content = true` also returns the input with the extracted `<FILE_CHANGES>` block removed as `Some(String)`.

## Applying changes to disk

Use `apply_file_changes` to execute directives relative to a base directory.

- All file paths are treated as relative to `base_dir`.
- The crate performs basic path safety checks to ensure operations stay within `base_dir`.
- Patch application uses `diffy` (Unified Diff parsing and application).

```rust
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

    for d in status.items {
        if d.success() {
            println!("OK   {} {}", d.kind(), d.file_path());
        } else {
            println!(
                "FAIL {} {}: {}",
                d.kind(),
                d.file_path(),
                d.error_msg().unwrap_or("unknown error")
            );
        }
    }

    Ok(())
}
```

## Directive behavior

- `FILE_NEW`: creates or overwrites a file, parent directories are created.
- `FILE_PATCH`: reads the target file, applies a unified diff, writes the result back.
- `FILE_RENAME`: renames/moves `from_path` to `to_path`.
- `FILE_DELETE`: removes a file or directory recursively.

If extraction fails for a directive (unknown tag, missing attribute, etc.), the directive is represented as:

- `FileDirective::Fail { kind, file_path, error_msg }`

When applying, `Fail` directives always yield an error for that directive and are reported via `ApplyChangesInfo`.

## Format tips for LLM output

- Always emit exactly one `<FILE_CHANGES>` block when you intend to apply changes.
- Prefer `FILE_PATCH` for small edits to large files.
- Use self-closing tags for rename and delete when convenient:
  - `<FILE_RENAME from_path="a" to_path="b" />`
  - `<FILE_DELETE file_path="path" />`

## System Prompt (optional)

The crate includes the recommended system instructions for LLMs to ensure they output the correct format. This is available via the `prompt` feature.

```toml
[dependencies]
udiffx = { version = "0.1", features = ["prompt"] }
```

```rust
use udiffx::prompt;

let instructions = prompt();
// Pass this to your LLM system message.
```


## License

MIT OR Apache-2.0

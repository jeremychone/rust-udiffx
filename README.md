# udiffx

Parse and apply an AI-optimized “file changes” envelope that carries multiple file operations in a single block, using unified diff patches for updates.

This crate is designed for LLM output that needs to be machine-parsable and efficient for large files with small edits.

[Doc for LLM](docs/for-llm/api-reference-for-llm.md)

## Quick Example

When the LLM returns some text eventually `FILE_CHANGES` tag like :

```txt

I fix the issue you reported. 

<FILE_CHANGES>

<FILE_NEW file_path="src/hello.rs">
pub fn hello() {
    println!("Hello from udiffx");
}
</FILE_NEW>

<FILE_APPEND file_path="changelog.md">
## Next

- Add a hello module.
</FILE_APPEND>

<FILE_PATCH file_path="src/main.rs">
@@
+mod hello;

 fn main() {
-    println!("Hello");
+    hello::hello();
 }
</FILE_PATCH>

<FILE_COPY from_path="docs/template.md" to_path="docs/getting-started.md" />

<FILE_RENAME from_path="src/old-name.rs" to_path="src/legacy-name.rs" />

<FILE_DELETE file_path="tmp/generated.txt" />

</FILE_CHANGES>

```

Your Rust code picks it up and applies everything in one shot:

```rust
use simple_fs::SPath;
use udiffx::{apply_file_changes, extract_file_changes, Result};

fn main() -> Result<()> {
    let ai_response = "..."; // the string above
    let base_dir = SPath::new("./my-project");
    let (changes, _) = extract_file_changes(ai_response, false)?;
    let status = apply_file_changes(&base_dir, changes)?;

    for item in status.items {
        if item.success() {
            println!("OK   {} {}", item.kind(), item.file_path());
        } else {
            eprintln!(
                "FAIL {} {}: {}",
                item.kind(),
                item.file_path(),
                item.error_msg().unwrap_or("unknown")
            );
        }
    }
    Ok(())
}
```

## Directive Behavior

- `<FILE_NEW file_path="..."> ... </FILE_NEW>` – creates or overwrites a file.
- `<FILE_APPEND file_path="..."> ... </FILE_APPEND>` – appends content to the end of a file (creates if missing).
- `<FILE_PATCH file_path="..."> ... </FILE_PATCH>` – modifies a file with one or more unified-diff hunks.
- `<FILE_COPY from_path="..." to_path="..." />` – copies a file.
- `<FILE_RENAME from_path="..." to_path="..." />` – renames or moves a file.
- `<FILE_DELETE file_path="..." />` – deletes a file or directory recursively.

All paths are relative to the base directory.

## Additional Features

- `load_files_context(base_dir, globs)` gathers file contents into `<FILE_CONTENT path="...">` blocks for LLM input.
- `prompt()` (feature `prompt`) returns recommended LLM system instructions for the envelope format.
- `apply_file_changes` performs path safety checks and applies patches incrementally; per-hunk errors are reported without stopping the whole operation.

## License

MIT OR Apache-2.0

---

[This Repo](https://github.com/jeremychone/rust-udiffx)
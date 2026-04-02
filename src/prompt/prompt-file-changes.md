## AI File Change Format

When modifying a codebase, emit all change directives inside a single `<FILE_CHANGES>` container using the directives format below. Do not place any other content inside `<FILE_CHANGES>`. Like

<FILE_CHANGES>
_file_change_directives_
</FILE_CHANGES>

You may include explanation before or after the `<FILE_CHANGES>` block. If no changes are required, output nothing.

IMPORTANT: There can be only one FILE_CHANGES tag per response. So make sure you think of everything before you give the file directives inside that tag.

IMPORTANT: This `FILE_CHANGES` tag can only have the file directives tag, and cannot contain any other tag.

### File Directives

| Directive     | Purpose                                                          |
| ------------- | ---------------------------------------------------------------- |
| `FILE_NEW`    | Create a new file                                                |
| `FILE_APPEND` | Append content to the end of a file (use this to append to file) |
| `FILE_PATCH`  | Modify an existing file via unified diff                         |
| `FILE_COPY`   | Copy a file                                                      |
| `FILE_RENAME` | Rename or move a file                                            |
| `FILE_DELETE` | Delete a file                                                    |

### General Rules

- The `file_path` attribute is the sole source of truth for the target file.
- Preserve exact formatting, indentation, and whitespace.
- Do not invent files or paths.
- The code fence language (e.g., `rust`, `ts`, `python`) is for syntax highlighting only.
- Make sure and triple check that the file patch hunk body surround lines or remove lines match exactly.
- **Never remove or alter existing comments** (except if explicitly asked by the user). Preserve them verbatim.

### FILE_NEW

Creates a new file. The content inside the code fence is the full file content.

<FILE_NEW file_path="path/to/file.ext">
_full_file_contents_
</FILE_NEW>

### FILE_APPEND

Appends content to the end of a file. If the file does not exist, it is created.

- If your intent is append-only, use `FILE_APPEND` instead of `FILE_PATCH`.
- Use `FILE_PATCH` only when modifying, removing, or replacing existing content in-place.

<FILE_APPEND file_path="path/to/file.ext">
_content_to_append_
</FILE_APPEND>

### FILE_PATCH

Modifies an existing file using a simplified, numberless unified diff format.

**Important: Use the `~` shorthand for large consecutive removals to keep patches concise.**

**Important: Do not include "no-op" hunks that consist only of context lines without any additions or removals.**

<FILE_PATCH file_path="path/to/file.ext">
_patch_format_
</FILE_PATCH>

#### Hunk header

- Use a single `@@` on its own line, with no line numbers.
- Never use `@@ -35,26 +83,32 @@`; always just `@@`.
- Do **not** include `---` / `+++` file header lines.
- A single `FILE_PATCH` may contain multiple hunks, each starting with `@@`.

#### Hunk body line format

Every line in a hunk body **must** start with one of exactly three prefix characters:

| Prefix | Meaning                   | Description                                                      |
| ------ | ------------------------- | ---------------------------------------------------------------- |
| ` `    | Context (space character) | Unchanged surrounding line; must match the original file exactly |
| `-`    | Removal                   | Line to remove; must match the original file exactly             |
| `+`    | Addition                  | Line to add                                                      |

**Critical rules for hunk body lines:**

- Every line must begin with one of these three prefix characters. There are no exceptions.
- Context lines (` ` prefix) and removal lines (`-` prefix) must be **exact character-for-character copies** of the corresponding lines in the original file. This includes all leading/trailing whitespace, indentation, and any content markers (e.g., Markdown bullet points like `-`, `*`, or `+`, and numbered list markers like `1.`). Any deviation, even a single space or tab, will cause the patch to fail.
- **Never omit removal lines (`-`)** for lines that exist in the original file but are being replaced or removed. If a line is being changed, it must be represented as a `-` line followed by a `+` line. Do not skip lines within the scope of a hunk.
- **Use the `~` (tilde) marker** for large consecutive removals to avoid unnecessary verbosity (see below).
- **Avoid no-op hunks:** Every hunk must contain at least one addition (`+`) or removal (`-`). Do not include hunks that only contain context lines.
- Minimize the number of context lines to reduce the chance of mismatch. Include only enough context to uniquely identify the location.
- Addition lines (`+` prefix) contain the new content to insert.

#### Range-Remove (`~`) shorthand

**Important: Favor this technique whenever removing more than 4-5 consecutive lines to keep the patch concise and resilient.**

When removing a large consecutive block of lines, use the `~` (tilde) marker instead of listing every single removal line. Place it on its own line between two groups of `-` lines:

- At least **2** removal lines must appear **above** the `~`.
- At least **2** removal lines must appear **below** the `~`.
- The engine will remove all original lines between the top and bottom anchor groups (inclusive of the anchors).
- `~` must only appear between `-` lines. It cannot appear between context (` `) or addition (`+`) lines.
- Multiple `~` markers are allowed within a single hunk, each independently bracketed by `-` lines.
- Addition (`+`) lines may follow the bottom anchor group as usual.

Example:

```
@@
 context before
-first line to remove
-second line to remove
~
-second-to-last line to remove
-last line to remove
+replacement line
 context after
```

#### FILE_PATCH format

<FILE_PATCH file_path="path/to/existing_file.ext">
@@
(context line - exact copy of original, prefixed with a space)
-(removal line - exact copy of original, prefixed with -)
+(addition line - new content, prefixed with +)
(context line - if needed)
</FILE_PATCH>

### FILE_COPY

Copies a file from `from_path` to `to_path`.

- The source must exist and be a file.
- The destination parent directories are created if needed.
- If the destination already exists, it is overwritten.

<FILE_COPY from_path="old/path.ext" to_path="new/path.ext" />

### FILE_RENAME

<FILE_RENAME from_path="old/path.ext" to_path="new/path.ext" />

### FILE_DELETE

<FILE_DELETE file_path="path/to/file.ext" />

### Complete Example

Always with `FILE_CHANGES` tag surrounding the file directives

#### Example

<FILE_CHANGES>

<FILE_NEW file_path="src/hello.rs">
pub fn hello() {
println!("Hello from hello.rs");
}

</FILE_NEW>

<FILE_PATCH file_path="src/main.rs">
@@
+mod hello;

 fn main() {
- println!("Old Message");
+ hello::hello();
 }
  </FILE_PATCH>

<FILE_COPY from_path="docs/OLD_README.md" to_path="docs/README.backup.md" />

<FILE_RENAME from_path="docs/OLD_README.md" to_path="README.md" />

<FILE_DELETE file_path="temp_notes.txt" />

</FILE_CHANGES>

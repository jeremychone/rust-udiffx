## AI File Change Format

When modifying a codebase, emit all change directives inside a single `<FILE_CHANGES>` container using the directives format below. Do not place any other content inside `<FILE_CHANGES>`. Like

<FILE_CHANGES>
_FILE_DIRECTIVES_
</FILE_CHANGES>

You may include explanation before or after the `<FILE_CHANGES>` block. If no changes are required, output nothing.

IMPORTANT. There can be only one FILE_CHANGES tag per response. So make sure you think of everything before you give the directives inside that tag.

### Directives

| Directive            | Purpose                                  |
| -------------------- | ---------------------------------------- |
| `FILE_NEW`           | Create a new file                        |
| `FILE_HASHLINE_PATCH`| Modify a file via hashline references    |
| `FILE_RENAME`        | Rename or move a file                    |
| `FILE_DELETE`        | Delete a file                            |

### General Rules

- The `file_path` attribute is the sole source of truth for the target file.
- Preserve exact formatting, indentation, and whitespace.
- Do not invent files or paths.
- The code fence language (e.g., `rust`, `ts`, `python`) is for syntax highlighting only.

### FILE_NEW

Creates a new file. The content inside the code fence is the full file content.

<FILE_NEW file_path="path/to/file.ext">
_full_file_contents_
</FILE_NEW>

### FILE_HASHLINE_PATCH

Modifies an existing file using a line-addressable format based on content hashes.

Each line in the file is identified by its 1-indexed line number and a short hexadecimal hash (e.g., `5#aa`). This combined `LINE#ID` reference ensures the edit targets the correct content and provides a staleness check.

#### Edit Operations

| Format                  | Operation | Description                                                                 |
| ----------------------- | --------- | --------------------------------------------------------------------------- |
| `LINE#ID:CONTENT`       | Set       | Replaces the content of the specified line.                                 |
| `LINE#ID-LINE#ID:CONTENT`| Replace   | Replaces a range of lines (inclusive) with the new content.                 |
| `>+LINE#ID CONTENT`     | Append    | Inserts new content *after* the specified line.                             |
| `<+LINE#ID CONTENT`     | Prepend   | Inserts new content *before* the specified line.                            |

**Critical rules for hashline edits:**

- Use the exact `LINE#ID` provided in the `<FILE_CONTENT>` block.
- Each edit must be on its own line within the `<FILE_HASHLINE_PATCH>` block.
- For range replacements (`LINE#ID-LINE#ID`), the start line must be less than or equal to the end line.
- You can include multiple edits for the same file in a single block.

#### FILE_HASHLINE_PATCH format

<FILE_HASHLINE_PATCH file_path="path/to/existing_file.ext">
5#aa:new content for line 5
>+10#bb inserted after line 10
15#cc-17#dd:replaces lines 15 through 17 with this text
</FILE_HASHLINE_PATCH>

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

<FILE_HASHLINE_PATCH file_path="src/main.rs">
>+1#ZZ:mod hello;
>+2#ZZ:
5#ZZ:    hello::hello();
</FILE_HASHLINE_PATCH>

<FILE_RENAME from_path="docs/OLD_README.md" to_path="README.md" />

<FILE_DELETE file_path="temp_notes.txt" />

</FILE_CHANGES>

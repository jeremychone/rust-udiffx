# AI-Optimized File Change Format and Utility

This document defines the `FILE_CHANGES` format, a high-density, machine-parsable protocol designed for AI models to communicate file system modifications efficiently.

## Overview

The `FILE_CHANGES` format optimizes communication by grouping multiple operations (create, update, rename, delete) into a single container. It leverages the Unified Diff format for updates, significantly reducing token consumption for large files with minor changes.

While the format uses XML-like tags, it is not intended to be strictly XML compliant. The content within tags or the surrounding text may not follow XML escaping rules. These tags are designed to be processed by a tag extractor that identifies and retrieves content based on specific tag names.

## Structure

All modifications must be encapsulated within a single `<FILE_CHANGES>` root tag.

### Root Container

- Tag: `<FILE_CHANGES>`
- Purpose: Encloses a sequence of file directives to be processed as a single unit.

## Directives

### 1. New File (`FILE_NEW`)

Used to create a new file. If the file already exists, it will be overwritten.

- Attributes: `file_path="..."`
- Content: The complete source code of the new file.

Format:
```xml
<FILE_NEW file_path="path/to/file.ext">
CONTENT
</FILE_NEW>
```

### 2. Patch File (`FILE_PATCH`)

Used to modify existing files using the Unified Diff format.

- Attributes: `file_path="..."`
- Content: A standard Unified Diff (UDiff) or a Simplified Patch representing the changes.

#### Simplified Hunk Headers
To reduce token usage, the format supports simplified hunk headers. Instead of full line numbers (e.g., `@@ -10,4 +10,4 @@`), the AI can emit just `@@`. The applier will automatically search the original file for the context lines and compute the correct line numbers.

Format:
```xml
<FILE_PATCH file_path="path/to/file.ext">
@@
-removed line
+added line
 context line
</FILE_PATCH>
```

### 3. Rename File (`FILE_RENAME`)

Used to move or rename files and directories.

- Attributes: `from_path="..."`, `to_path="..."`

Format:
```xml
<FILE_RENAME from_path="src/old.rs" to_path="src/new.rs" />
```

### 4. Delete File (`FILE_DELETE`)

Used to permanently remove a file or directory.

- Attributes: `file_path="..."`

Format:
```xml
<FILE_DELETE file_path="path/to/obsolete.txt" />
```

## Content Encoding and Fencing

To ensure better readability and compatibility with markdown-aware tools, content within `FILE_NEW` and `FILE_PATCH` tags can optionally be wrapped in triple backtick code fences.

- The applier will automatically detect and strip these fences.
- One level of leading/trailing newline is typically ignored for both fenced and non-fenced content.

Example with code fence:
```xml
<FILE_NEW file_path="src/lib.rs">
```rust
pub fn init() {
    // ...
}
```
</FILE_NEW>
```

## Unified Example

Below is an example of a single response containing multiple operations, using both standard and simplified syntax.

```xml
<FILE_CHANGES>

<FILE_RENAME from_path="src/legacy_mod.rs" to_path="src/core_mod.rs" />

<FILE_DELETE file_path="temp_config.json" />

<FILE_NEW file_path="src/helpers.rs">
```rust
pub fn get_version() -> &'static str {
    "0.1.0"
}
```
</FILE_NEW>

<FILE_PATCH file_path="src/main.rs">
@@
 fn main() {
-    println!("Starting...");
+    println!("App v{}", helpers::get_version());
     core_mod::init();
 }
</FILE_PATCH>

</FILE_CHANGES>
```

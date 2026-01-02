# AI-Optimized File Change FIrnat abd Utility

This document defines the `FILE_CHANGES` format, a high-density, machine-parsable protocol designed for AI models to communicate file system modifications efficiently.

## Overview

The `FILE_CHANGES` format optimizes communication by grouping multiple operations (create, update, rename, delete) into a single container. It leverages the Unified Diff format for updates, significantly reducing token consumption for large files with minor changes.

While the format uses XML-like tags, it is not intended to be strictly XML compliant in its entirety. The content within tags or the surrounding text may not follow XML escaping rules. These tags are designed to be processed by a tag extractor that identifies and retrieves content based on specific tag names. Specifically, these tags while XML themselves are not intended to have their content or surrounding XML compliant, they will be extract from a tag extractor that will extract those tags for given tag names.

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
- Content: A standard Unified Diff (UDiff) representing the changes.

Format:
```xml
<FILE_PATCH file_path="path/to/file.ext">
@@ -line,count +line,count @@
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

## Unified Example

Below is an example of a single response containing multiple operations.

```xml
<FILE_CHANGES>

<FILE_RENAME from_path="src/legacy_mod.rs" to_path="src/core_mod.rs" />

<FILE_DELETE file_path="temp_config.json" />

<FILE_NEW file_path="src/helpers.rs">
pub fn get_version() -> &'static str {
    "0.1.0"
}
</FILE_NEW>

<FILE_PATCH file_path="src/main.rs">
@@ -10,4 +10,4 @@
 fn main() {
-    println!("Starting...");
+    println!("App v{}", helpers::get_version());
     core_mod::init();
 }
</FILE_PATCH>

</FILE_CHANGES>
```

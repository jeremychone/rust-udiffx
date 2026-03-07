# Dev Chat

Add a new `## Request: _user_ask_title_concise_` with the answer below (concise title). Use markdown sub-headings for sub sections. Keep this top instruction in this file.

## Request: Patch Resilience Improvement Analysis

After reviewing the spec (`spec-patch-strategy.md`) and the implementation (`patch_completer.rs`), here are potential improvements to make the patch completer more resilient:

### 1. Leading Prefix/Indentation Change Tolerance

Currently, if the LLM changes the indentation style (e.g., tabs vs spaces), Strict fails and Resilient handles it via trimming. However, if the _amount_ of indentation changes uniformly across a block (e.g., a re-indent), there is no detection of a consistent offset. A "shifted indentation" heuristic could detect when all context/removal lines differ by a constant indent delta and still match at Resilient tier.

### 2. Line Reordering Within a Hunk

If the LLM accidentally swaps two adjacent context lines, the entire hunk fails to match. A small-window permutation check (e.g., allowing a single adjacent swap) could recover from this without false positives.

### 3. Partial/Truncated Removal Lines

The suffix matching logic applies to context lines, but removal lines (`-`) could also benefit from suffix matching. If an LLM truncates the beginning of a long removal line, it currently fails. Extending suffix matching to removal lines at Resilient/Fuzzy tiers would help.

### 4. Comment-Only Line Tolerance

Source files often have comments that the LLM omits or rewrites in context. A heuristic that recognizes comment patterns (e.g., `//`, `#`, `/* */`, `<!-- -->`) and treats comment-only context lines more leniently (e.g., matching just the non-comment prefix or ignoring them if they don't match) could improve resilience for code files.

### 5. Consecutive Hunk Recovery

When a multi-hunk patch has one failing hunk, the entire file fails. A "best-effort" mode that applies successful hunks and reports failures per-hunk (rather than aborting) would allow partial progress. The current `search_from` tracking already supports this conceptually.

### 6. Duplicate/Near-Duplicate Block Disambiguation

When the same code block appears multiple times (e.g., repeated patterns), the scoring uses exact whitespace count + proximity. Adding a "surrounding context" check, looking at lines immediately before/after the candidate match that are NOT part of the hunk, could disambiguate better.

### 7. Trailing Semicolon / Comma Tolerance

In code files, LLMs sometimes add or omit trailing commas or semicolons. The Fuzzy tier strips trailing punctuation, but only for the entire line. A more targeted approach for specific languages (trailing `,` or `;`) at the Resilient tier could catch common code formatting differences without going fully fuzzy.

### 8. Numeric Literal Tolerance

LLMs sometimes reformat numeric literals (e.g., `1000` vs `1_000` in Rust, or `0x0A` vs `0x0a`). This is an edge case but could be addressed at the Fuzzy tier with a numeric normalization pass.

### 9. Multi-Line String Handling

When context spans a multi-line string literal, the LLM may collapse or reformat it. There is currently no special handling for string boundaries.

### 10. Empty File Bootstrapping

When patching an empty file (or a file that doesn't exist yet), the code falls back to treating the original as an empty string. A `FILE_PATCH` against an empty file with only `+` lines works, but if the LLM includes phantom context lines for a non-existent file, it fails. Detecting "original is empty + hunk has context lines" and auto-converting context to additions could help.

### Priority Ranking

| #   | Improvement                    | Impact  | Complexity | Final Priority (10 highest) | Suggested Priority | Tier(s)             |
| --- | ------------------------------ | ------- | ---------- | --------------------------- | ------------------ | ------------------- |
| 5   | Consecutive Hunk Recovery      | High    | Medium     | 0                           | High               | All (orchestration) |
| 3   | Partial Removal Line Suffix    | Medium  | Low        | 10                          | High               | Resilient, Fuzzy    |
| 1   | Shifted Indentation            | Medium  | Medium     | 9                           | Medium             | Resilient           |
| 6   | Duplicate Block Disambiguation | Medium  | Medium     | 3                           | Medium             | All (scoring)       |
| 10  | Empty File Bootstrapping       | Low-Med | Low        | 8                           | Medium             | All (pre-matching)  |
| 4   | Comment-Only Tolerance         | Medium  | High       | 4                           | Low                | Resilient           |
| 7   | Trailing Semicolon/Comma       | Low     | Low        | 9                           | Low                | Resilient           |
| 2   | Line Reordering                | Low     | High       | 0                           | Low                | Resilient           |
| 8   | Numeric Literal Tolerance      | Low     | Low        | 7                           | Low                | Fuzzy               |
| 9   | Multi-Line String Handling     | Low     | High       | 6                           | Low                | Resilient, Fuzzy    |

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

| #   | Improvement                    | Impact  | Complexity | Final Priority (10 highest) | Suggested Priority | Tier(s)             | Status       |
| --- | ------------------------------ | ------- | ---------- | --------------------------- | ------------------ | ------------------- | ------------ |
| 5   | Consecutive Hunk Recovery      | High    | Medium     | 0                           | High               | All (orchestration) | Not started  |
| 3   | Partial Removal Line Suffix    | Medium  | Low        | 10                          | High               | Resilient, Fuzzy    | **Done**     |
| 1   | Shifted Indentation            | Medium  | Medium     | 9                           | Medium             | Resilient           | **Done**     |
| 6   | Duplicate Block Disambiguation | Medium  | Medium     | 3                           | Medium             | All (scoring)       | **Done**     |
| 10  | Empty File Bootstrapping       | Low-Med | Low        | 8                           | Medium             | All (pre-matching)  | **Done**     |
| 4   | Comment-Only Tolerance         | Medium  | High       | 4                           | Low                | Resilient           | **Done**     |
| 7   | Trailing Semicolon/Comma       | Low     | Low        | 9                           | Low                | Resilient           | **Done**     |
| 2   | Line Reordering                | Low     | High       | 0                           | Low                | Resilient           | Not started  |
| 8   | Numeric Literal Tolerance      | Low     | Low        | 7                           | Low                | Fuzzy               | **Done**     |
| 9   | Multi-Line String Handling     | Low     | High       | 6                           | Low                | Resilient, Fuzzy    | **Done**     |

## Implementation Summary (2026-03-07)

All items except #2 (Line Reordering) and #5 (Consecutive Hunk Recovery) have been implemented. Here is a summary of what was done:

### #3 - Partial Removal Line Suffix Matching

- Verified that the existing `line_matches` function (which covers both context and removal lines) already had suffix matching at the Resilient and Fuzzy tiers.
- Added explicit test coverage: `test_patch_completer_complete_removal_suffix_match` and `test_patch_completer_complete_removal_short_no_suffix_match`.
- Added integration test in `tests/data/test-patches/test-14-removal-suffix/`.

### #7 - Trailing Semicolon/Comma Tolerance

- Added a targeted check in `line_matches` at the Resilient tier: strips trailing `,` or `;` from both lines and re-compares (trimmed and normalized whitespace).
- Only activates when at least one line actually has a trailing `,` or `;` that differs, preventing false positives.

### #1 - Shifted Indentation Heuristic

- Added `leading_ws_len` and `has_uniform_indent_delta` helpers.
- Extended `CandidateMatch` with `uniform_indent: bool`.
- Updated `score_candidate` to use `uniform_bonus` (weighted between exact_ws_count and proximity).
- This allows disambiguation when duplicate code blocks exist at different indentation levels.

### #10 - Empty File Bootstrapping

- In `compute_hunk_bounds`, added early detection for empty/blank-only originals.
- When detected, all context/removal lines in the hunk are auto-converted to additions.
- Pure addition-only hunks continue to use existing append logic.

### #8 - Numeric Literal Tolerance

- Added `strip_numeric_underscores` helper that removes `_` between hex digits.
- Added as a Fuzzy-tier fallback in `line_matches`.

### #9 - Multi-Line String Handling

- Added `strip_all_ws` helper and a last-resort comparison in the Fuzzy tier.
- Strips all whitespace and compares remaining characters (minimum 4 non-whitespace characters to avoid false positives on short lines).

### #6 - Duplicate Block Disambiguation via Adjacent Hunk Context

- Refactored `complete()` to a two-pass approach: first pass collects all raw hunk bodies, second pass processes each hunk with knowledge of adjacent hunks.
- Added `AdjacentHints` struct, `build_adjacent_hints`, `hint_line_matches`, and `compute_adjacent_hint_matches`.
- Adjacent hint matches contribute a large scoring bonus (10,000 per match), strongly preferring candidates with surrounding context confirmation.

### #4 - Comment-Only Line Tolerance

- Added `strip_comment_marker` helper recognizing `//`, `#` (excluding `#!`/`##`), and `<!-- -->`.
- In `line_matches` at the Resilient tier, if both lines are comment-only, strip the marker and compare with `normalize_ws`.
- Also added `is_comment_marker_prefix` to the suffix matcher to reject false positives where the non-matching prefix is a comment marker.

### Remaining Items

- **#5 - Consecutive Hunk Recovery**: Not implemented. Would require a "best-effort" mode that applies successful hunks and reports failures per-hunk rather than aborting. Considered lower priority since the adjacent hunk disambiguation (#6) already reduces multi-hunk failures.
- **#2 - Line Reordering**: Not implemented. High complexity with risk of false positives; considered low priority.

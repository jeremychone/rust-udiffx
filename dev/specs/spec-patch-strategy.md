# Patch Application and Matching Strategy

This document specifies the matching and application logic for the `FILE_PATCH` directive. The goal of this strategy is to provide high reliability when applying LLM-generated patches, which often contain minor whitespace inaccuracies, casing errors, or incomplete context.

## Simplified Unified Diff Format

The `FILE_PATCH` directive uses a numberless version of the Unified Diff format.

- **Hunk Header**: Uses a single `@@` on its own line. It does not require line numbers (e.g., `@@ -1,5 +1,6 @@`).
- **Context/Removal/Addition**: Follows the standard ` `, `-`, and `+` prefix conventions.
- **Completion**: The applier "completes" these simplified hunks by locating the context in the target file and generating a standard Unified Diff that tools like `diffy` can process.

## Tiered Matching Logic

To balance accuracy with resilience, the patch completion engine employs a three-tier matching strategy. It attempts to find a match using the strictest criteria first, falling back to more lenient tiers only if no candidates are found.

- **Stage 1: Strict** - Performs a character-for-character exact match of the entire line.
  - No trimming, normalization, or casing adjustments are performed.
  - This tier provides the highest confidence and is processed first to ensure well-formed patches are applied exactly as intended.

- **Stage 2: Resilient** - Performs case-sensitive matching with the following normalizations:
  - **Trimming**: Leading and trailing whitespace is ignored.
  - **Whitespace Normalization**: Multiple consecutive spaces or tabs are collapsed into a single space.
  - **Markdown Heading Normalization**: Heading markers (e.g., `### `) and their leading/trailing whitespace are normalized to allow for variation in heading levels or spacing.
  - **Suffix Matching**: If a context or removal fragment is at least 10 characters long, it matches if it appears at the end of an original line. This accommodates long lines that the LLM may have truncated. A comment-marker-prefix guard prevents non-comment lines from false-positive matching comment lines via suffix (e.g., `"do something"` will not match `"// do something"` via suffix).
  - **Trailing Semicolon/Comma Tolerance**: Strips a single trailing `;` or `,` from both the original and patch lines and re-compares. This handles common LLM formatting differences in code (e.g., omitting or adding trailing punctuation) without requiring the Fuzzy tier. Only activates when at least one line has a trailing `,` or `;` that differs.
  - **Comment-Only Line Tolerance**: When both the original and patch lines are comment-only (starting with `//`, `#` excluding `#!`/`##`, or `<!-- -->`), the engine strips the comment marker and compares the remaining content with normalized whitespace. This handles minor wording or spacing differences in comments. If only one line is a comment, this check is skipped entirely to prevent false positives.

- **Stage 3: Fuzzy** - Performs all normalizations from the Resilient tier and adds:
  - **Case-Insensitivity**: All comparisons are performed on lowercased versions of the lines.
  - **Inline Formatting Resilience**: Strips inline backticks (`` ` ``) from both the original and patch lines to resolve discrepancies in Markdown code formatting.
  - **Trailing Punctuation Resilience**: Ignores trailing ASCII punctuation (like periods or commas) that LLMs often add or omit inconsistently at the end of sentences.
  - **Numeric Literal Tolerance**: Strips underscore separators (`_`) from numeric literals. This handles reformatting like `1_000` vs `1000` or `0xFF_FF` vs `0xFFFF`. Only underscores immediately preceded and followed by hex digits are stripped.
  - **Whitespace-Stripped Last Resort**: As a final fallback, strips all whitespace from both lines and compares the remaining characters (minimum 4 non-whitespace characters required). This handles cases where the LLM reformats internal whitespace in string literals or similar content without introducing false positives on very short lines.

### Execution Flow

The engine iterates through these tiers in order. If a tier yields one or more candidate matches, the search stops, and the best candidate from that tier is selected. If no candidates are found after all tiers are exhausted, the patch application fails.

## Match Resolution and Scoring

When multiple candidates are found within the same tier, a scoring system determines the best fit.

### Proximity

The search for hunk context begins at the end of the previously applied hunk (tracking cumulative line-count deltas). Proximity to this expected location is a key factor in scoring to prevent matching similar code blocks far apart in a file.

Note: For Resilient and Fuzzy tiers, a proximity limit is enforced to prevent excessive drift and maintain performance. This limit is set to 1,000 lines from the expected position (in either direction), except for the first hunk where a window of up to 5,000 lines is allowed to facilitate anchoring in large files.

Note: The search scans the entire file (both before and after the expected position) at all tiers, relying on proximity scoring to prefer the nearest match. This ensures that out-of-order hunks (where the LLM emits hunks in reverse file order) can still be matched when pre-sort positioning fails. For Strict tier, there is no proximity limit; for Resilient and Fuzzy tiers, the proximity window above applies in both directions.

### Exact Whitespace Count

Within the Resilient and Fuzzy tiers, candidates that have more "exact" matches (where the line matched without needing normalization) are scored higher. This ensures that even in lenient tiers, the most visually similar block is preferred.

### Uniform Indentation Delta

When context/removal lines match at the Resilient tier via trimmed comparison, the engine computes the leading whitespace delta (difference in leading space count between original and patch lines) for each matched non-blank line pair. If the delta is the same for every pair, the candidate receives a scoring boost (`uniform_indent = true`). This helps disambiguate when duplicate code blocks exist at different indentation levels; the block with a consistent re-indent is preferred over one with irregular indentation differences.

### Adjacent Hunk Context Disambiguation

When a patch contains multiple hunks, adjacent hunk context is used to disambiguate identical code blocks. For each candidate match:

- **Previous hint**: The last context/removal line of the previous hunk is compared (using Resilient-tier matching) against the original line immediately before the candidate's start position. A match indicates the candidate is positioned correctly relative to the prior hunk's changes.
- **Next hint**: The first context/removal line of the next hunk is compared against the original line immediately after the candidate's matched region. A match indicates the candidate is followed by content that the next hunk expects.

Each matching hint adds a substantial scoring bonus (weighted higher than uniform indent and proximity), so candidates with surrounding context confirmation are strongly preferred over those without.

### Scoring Priority Order

The scoring factors are evaluated in the following priority order (highest to lowest):

1. **Exact whitespace count** (primary, higher is better)
2. **Adjacent hint matches** (0-2, each worth 10,000 points)
3. **Uniform indent bonus** (worth 1,000 points if uniform)
4. **Proximity** (negative distance from expected position, closer is better)

## Structural Resilience

The strategy includes specific handlers for common LLM formatting artifacts.

### Wrapper Meta Line Sanitization

LLM outputs sometimes wrap simplified hunks in outer helper lines such as:

- `*** Begin Patch`
- `*** Update File: ...`
- `*** End Patch`

These wrapper meta lines are not part of the simplified unified diff format and must not interfere with hunk parsing.

- During actionable-hunk detection and hunk splitting, the engine first tries the raw content as-is.
- If no actionable hunks are found, it retries after removing recognized wrapper meta lines.
- Non-wrapper `*** ...` lines remain normal content and can still be matched literally when they are part of the original file.

This preserves strict behavior for legitimate file content while still recovering from common wrapper artifacts in LLM output.

### Hunk Cleanup (Artifact Removal)

To ensure high reliability, the engine performs a cleanup step on each hunk before matching:

- **Trailing Whitespace-Only Lines**: Any lines at the end of a hunk that consist solely of whitespace (including a single space `" "` which can be mistaken for a blank context line) are stripped. These are typically artifacts of the XML/tag extraction process and do not represent intentional patch content. This cleanup allows hunks containing only additions followed by cosmetic whitespace to be correctly identified as append-only operations.
- **Surround-Only Hunk Filtering**: A hunk is considered actionable only if it contains at least one `+` or `-` line. Hunks containing only context lines are ignored during splitting and completion. If all hunks are surround-only, completion returns an empty patch and no match tier.

### Suffix Matching

If a context or removal line in a patch is a suffix of an original line (minimum 10 characters), it is considered a match. This allows the LLM to provide only the trailing part of a long line as context. A guard against comment marker prefixes prevents false positives where the non-matching prefix is a comment marker (e.g., `//` or `#`).

### Blank Line Alignment

LLMs often insert "cosmetic" blank lines in patches for readability, or conversely, omit blank lines that are present in the source.

- If a patch contains a blank context line that aligns with a blank line in the source, it matches normally.
- If a blank context line does not align with a blank line in the source, the engine converts that hunk line into an addition (`+`) line. This preserves the LLM's intended spacing without causing alignment drift for subsequent context lines.
- In Resilient and Fuzzy tiers, if the original file contains consecutive blank lines that are not present in the patch context before a non-blank line, the engine automatically skips these extra source blank lines and includes them in the resulting hunk to maintain correct alignment.
- When reconstructing the final completed hunk, skipped original blank lines are emitted back as exact context lines before the next matched line, preserving file alignment for downstream application.

### EOF Overhang

If the context lines in a hunk extend beyond the end of the file, they are treated as "overhang" and dropped, provided they are context lines (` `) and not removal lines (`-`). This allows patches to include trailing context that the LLM incorrectly assumed existed at the end of a file.

Validation rules prevent overhang abuse: at least 2 significant (non-blank) in-file matches are required, and in-file matches must outnumber overhang lines.

### Append-Only Hunks and Blank-Line Overlap

A hunk with no context/removal lines (only `+` lines) is treated as an append operation at the end of the file.

To avoid accidental duplication of trailing blank lines during append:

- The engine counts trailing blank lines already present in the original file.
- If the append hunk starts with blank addition lines, it converts as many of those leading blank additions as possible into context lines that anchor to existing trailing blanks.
- If the append hunk ends with blank addition lines, it can also convert some trailing blank additions into context lines using any remaining trailing blanks not already consumed by the leading overlap.
- Blank additions that cannot be overlapped remain true additions.

This behavior preserves intended spacing while preventing repeated blank-line growth at EOF when LLM output includes cosmetic empty lines around appended content.

### Empty File Bootstrapping

When the original content is empty (or contains only blank lines) and a `FILE_PATCH` hunk contains context or removal lines, the engine auto-converts all context and removal lines into addition lines. This allows a `FILE_PATCH` against a non-existent or empty file to succeed rather than failing to find context. Pure addition-only hunks against empty files continue to use the existing append logic.

### Adjacent Hunk Context

When multiple hunks are present in a patch, the engine collects all hunk bodies in a first pass, then processes each hunk with knowledge of the adjacent hunks' context lines. This allows the scoring system to disambiguate identical code blocks by verifying that the surrounding file content matches what neighboring hunks expect. See the "Adjacent Hunk Context Disambiguation" section under Match Resolution and Scoring for details.

### Empty Completed Patch Semantics

If no actionable hunks are found, and there are no preserved non-hunk prefix lines, patch completion returns:

- an empty completed patch string
- no match tier

This represents a no-op completion result from the completer itself.

## Tilde Range-Remove (`~`)

### Overview

The `~` (tilde) marker provides a shorthand for large consecutive block removals. Instead of listing every removal line, the LLM specifies the top few and bottom few removal lines as anchors, separated by `~`. The engine expands the range by removing all original lines between the top and bottom anchors.

### Syntax Rules

- `~` appears on its own line within a hunk body.
- At least 2 removal (`-`) lines must appear immediately above the `~`.
- At least 2 removal (`-`) lines must appear immediately below the `~`.
- `~` cannot appear between context (` `) or addition (`+`) lines; such usage is a validation error.
- Multiple `~` markers are allowed within a single hunk, each independently bracketed by removal lines.
- Addition (`+`) lines may follow the bottom anchor group normally.

### Expansion Logic

When the engine encounters `~` during hunk processing:

1. The top anchor removal lines are matched against the original file using the standard tiered matching (Strict > Resilient > Fuzzy).
2. The bottom anchor removal lines are located by searching forward from the last matched top-anchor position.
3. All original lines between the last top anchor and first bottom anchor (exclusive of anchors) are emitted as removal (`-`) lines in the completed patch.
4. The explicit bottom anchor lines are then emitted as normal removal lines.
5. `old_count` includes all consumed original lines: explicit top anchors, expanded intermediate lines, and explicit bottom anchors.

### Anchor Matching

The top and bottom anchor `-` lines go through the same tiered matching as normal removal lines. This means anchor lines benefit from Resilient whitespace normalization and Fuzzy case-insensitive matching when needed.

### Ambiguous Anchors

If the bottom anchor sequence appears multiple times within the range, the engine matches the first occurrence searching forward from the last top anchor. This is consistent with the greedy forward search used for normal hunk context matching.

### Interaction with Blank Lines

If the original file has blank lines between the top and bottom anchors, they are consumed as part of the expanded range and emitted as removal lines.

### Validation Errors

If `~` is not properly bracketed by the minimum number of removal lines, the engine produces a clear `PatchCompletion` error for that hunk. Other hunks in the same patch are unaffected (partial hunk application still applies).

## Performance Considerations

- **Early Exit**: The tiered approach ensures that well-formed, strict patches are processed quickly without ever triggering the more expensive normalization or lowercasing logic of the Resilient and Fuzzy tiers.
- **Search Window**: While the search is greedy, it prioritizes the area immediately following the last successful hunk.
- **Two-Pass Architecture**: The first pass to collect raw hunks is lightweight (no matching), and the second pass benefits from adjacent hunk knowledge for better disambiguation without additional file scans.

### Hunk Pre-Sort by File Position

LLMs sometimes emit hunks in reverse or arbitrary order relative to the file (e.g., bottom-of-file changes first, then top-of-file changes). Since the completion engine tracks `search_from` across hunks, out-of-order hunks can cause cascading failures if the search only moves forward.

To handle this, the engine performs a pre-sort pass before processing hunks:

- For each hunk, the first non-blank context or removal line is extracted and searched for in the original file using **Strict (exact) matching only**.
- If exactly one exact match is found, that line index is recorded as the hunk's estimated file position. If zero or multiple matches are found, the position is `None` (ambiguous), and the hunk retains its original relative order.
- If any hunk's estimated position is less than the previous hunk's (i.e., out of order), a stable sort by position is performed. Hunks without a position estimate receive `usize::MAX` as their sort key, pushing them to the end while preserving their relative order among themselves.
- If hunks are already in ascending order, the sort is a no-op (zero overhead for the common case).

This approach is safe by construction: only hunks with unambiguous Strict anchors are reordered. When duplicate code blocks exist (multiple exact matches), the presort declines to reorder, and the existing proximity scoring and adjacent hint disambiguation resolve the correct position during normal processing.

### Full-File Search with Proximity Scoring

The search for hunk context scans the entire original file (from index 0 through the end), not just forward from `search_from`. This ensures that out-of-order hunks can still be matched when presort cannot determine their positions (e.g., all position estimates are `None` due to ambiguous matches).

Safety is maintained through proximity scoring: `score_candidate` uses `abs_diff(candidate_index, search_from)` as a negative score component, so matches near the expected position are strongly preferred. For Resilient and Fuzzy tiers, the `MAX_PROXIMITY_FOR_LENIENT` window (1,000 lines, or 5,000 for the first hunk) filters out candidates that are too far in either direction. Strict tier has no proximity limit since exact matches carry high confidence.

## Partial Hunk Application

When a `FILE_PATCH` directive contains multiple hunks, the engine applies them incrementally rather than as an all-or-nothing operation. This ensures that a single malformed or unmatchable hunk does not prevent other valid hunks in the same patch from being applied.

### Incremental Apply Flow

- The raw patch content is split into individual hunks using the same parsing logic as the completion engine (CRLF normalization, wrapper meta line sanitization, trailing whitespace stripping, and the actionable check).
- Each hunk is processed independently in order, using the current in-memory file content as the base for completion and application.
- On success: the working content is updated with the hunk's changes, and the match tier is tracked.
- On failure: the working content remains unchanged, and the failure is recorded with the hunk body and cause.

Before per-hunk application:

- The applier normalizes CRLF input to LF for both original content and patch content.
- If the original content is non-empty and lacks a trailing newline, one is added before patch completion and diff application.

### Single-Hunk Optimization

When a patch contains only one hunk, the engine delegates to the standard all-or-nothing apply path for simplicity. The incremental logic is only activated for multi-hunk patches.
- This preserves the existing public `apply_patch` behavior while allowing multi-hunk `FILE_PATCH` directives to partially succeed.

### Directive-Level Success Semantics

- A patch directive is considered successful if at least one hunk was applied and the file write succeeded.
- If all hunks fail, the directive is marked as failed with a summary error message.
- The highest match tier encountered across all successfully applied hunks is reported at the directive level.
- If at least one hunk applies but some fail, the directive is still successful and the partial failures are exposed through `error_hunks`.
- After patch application, if the resulting file content is unchanged from the original and the target file already exists, the directive is treated as `No changes applied`.
- For multi-hunk patch directives, structured per-hunk failures are preserved even when all hunks fail. In that case, the directive remains failed, `error_msg` contains the summary, and `error_hunks` contains every failed hunk. This avoids status/reporting mismatches where a caller can see an all-hunks-failed summary but no per-hunk details.

### Per-Hunk Error Reporting

Failed hunks are captured in the `error_hunks` field of the directive status. Each entry contains:

- `hunk_body`: the raw hunk text that failed, useful for debugging.
- `cause`: a description of why the hunk failed (e.g., completion mismatch, diffy parse error, diffy apply error).

The directive-level `error_msg` field remains available for summary errors. The `error_hunks` list provides structured, per-hunk detail that callers can inspect or display.

At the status-model level:

- `DirectiveStatus.error_hunks` is populated only for patch directives.
- `DirectiveStatus.match_tier` stores the highest tier used by any successfully applied hunk.
- `HunkError` contains the raw single-hunk body and the per-hunk failure cause.
- In the all-hunks-fail case for a multi-hunk patch, `DirectiveStatus.error_hunks` still contains all failed hunks. Callers should treat `error_hunks` as the source of truth for per-hunk reporting, regardless of whether the directive had partial success or total failure.

### Cross-Directive Continuation

Partial hunk failure within one `FILE_PATCH` does not affect other directives. The applier processes each directive in sequence; errors are captured per directive, and subsequent directives always execute regardless of prior failures.

### File Creation and Writeback Behavior

- A `FILE_PATCH` may target a file that does not yet exist. In that case, the applier treats the original content as empty and still attempts completion and application.
- If patch application succeeds for a non-existent target, parent directories are created before writing the new file.
- The current implementation writes output using LF line endings, even when the original input contained CRLF.

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

- **Stage 2: Resilient** - Performs case-sensitive matching with the following normalization:
  - **Trimming**: Leading and trailing whitespace is ignored.
  - **Whitespace Normalization**: Multiple consecutive spaces or tabs are collapsed into a single space.
  - **Markdown Heading Normalization**: Heading markers (e.g., `### `) and their leading/trailing whitespace are normalized to allow for variation in heading levels or spacing.
  - **Suffix Matching**: If a context fragment is at least 10 characters long, it matches if it appears at the end of an original line. This accommodates long lines that the LLM may have truncated.

- **Stage 3: Fuzzy** - Performs all normalizations from the Resilient tier and adds:
  - **Case-Insensitivity**: All comparisons are performed on lowercased versions of the lines.
  - **Inline Formatting Resilience**: Strips inline backticks (`` ` ``) from both the original and patch lines to resolve discrepancies in Markdown code formatting.
  - **Trailing Punctuation Resilience**: Ignores trailing ASCII punctuation (like periods or commas) that LLMs often add or omit inconsistently at the end of sentences.
  - **Whitespace-Stripped Last Resort**: As a final fallback, strips all whitespace from both lines and compares the remaining characters (minimum 4 non-whitespace characters required). This handles cases where the LLM reformats internal whitespace in string literals or similar content without introducing false positives on very short lines.

### Execution Flow

The engine iterates through these tiers in order. If a tier yields one or more candidate matches, the search stops, and the best candidate from that tier is selected. If no candidates are found after all tiers are exhausted, the patch application fails.

## Match Resolution and Scoring

When multiple candidates are found within the same tier, a scoring system determines the best fit.

### Proximity

The search for hunk context begins at the end of the previously applied hunk (tracking cumulative line-count deltas). Proximity to this expected location is a key factor in scoring to prevent matching similar code blocks far apart in a file.

Note: For Resilient and Fuzzy tiers, a proximity limit is enforced to prevent excessive drift and maintain performance. This limit is set to 1,000 lines from the expected position, except for the first hunk where a window of up to 5,000 lines is allowed to facilitate anchoring in large files.

### Exact Whitespace Count

Within the Resilient and Fuzzy tiers, candidates that have more "exact" matches (where the line matched without needing normalization) are scored higher. This ensures that even in lenient tiers, the most visually similar block is preferred.

### Adjacent Hunk Context Disambiguation

When a patch contains multiple hunks, adjacent hunk context is used to disambiguate identical code blocks. For each candidate match:

- **Previous hint**: The last context/removal line of the previous hunk is compared (using Resilient-tier matching) against the original line immediately before the candidate's start position. A match indicates the candidate is positioned correctly relative to the prior hunk's changes.
- **Next hint**: The first context/removal line of the next hunk is compared against the original line immediately after the candidate's matched region. A match indicates the candidate is followed by content that the next hunk expects.

Each matching hint adds a substantial scoring bonus (weighted higher than uniform indent and proximity), so candidates with surrounding context confirmation are strongly preferred over those without.

## Structural Resilience

The strategy includes specific handlers for common LLM formatting artifacts.

### Hunk Cleanup (Artifact Removal)

To ensure high reliability, the engine performs a cleanup step on each hunk before matching:

- **Trailing Whitespace-Only Lines**: Any lines at the end of a hunk that consist solely of whitespace (including a single space `" "` which can be mistaken for a blank context line) are stripped. These are typically artifacts of the XML/tag extraction process and do not represent intentional patch content. This cleanup allows hunks containing only additions followed by cosmetic whitespace to be correctly identified as append-only operations.

### Suffix Matching

If a context line in a patch is a suffix of an original line (minimum 10 characters), it is considered a match. This allows the LLM to provide only the trailing part of a long line as context.

### Blank Line Alignment

LLMs often insert "cosmetic" blank lines in patches for readability, or conversely, omit blank lines that are present in the source.

- If a patch contains a blank context line that aligns with a blank line in the source, it matches normally.
- If a blank context line does not align with a blank line in the source, the engine converts that hunk line into an addition (`+`) line. This preserves the LLM's intended spacing without causing alignment drift for subsequent context lines.
- In Resilient and Fuzzy tiers, if the original file contains consecutive blank lines that are not present in the patch context before a non-blank line, the engine automatically skips these extra source blank lines and includes them in the resulting hunk to maintain correct alignment.

### EOF Overhang

If the context lines in a hunk extend beyond the end of the file, they are treated as "overhang" and dropped, provided they are context lines (` `) and not removal lines (`-`). This allows patches to include trailing context that the LLM incorrectly assumed existed at the end of a file.

### Append-Only Hunks and Blank-Line Overlap

A hunk with no context/removal lines (only `+` lines) is treated as an append operation at the end of the file.

To avoid accidental duplication of trailing blank lines during append:

- The engine counts trailing blank lines already present in the original file.
- If the append hunk starts with blank addition lines, it converts as many of those leading blank additions as possible into context lines that anchor to existing trailing blanks.
- If the append hunk ends with blank addition lines, it can also convert some trailing blank additions into context lines using any remaining trailing blanks not already consumed by the leading overlap.
- Blank additions that cannot be overlapped remain true additions.

This behavior preserves intended spacing while preventing repeated blank-line growth at EOF when LLM output includes cosmetic empty lines around appended content.

### Adjacent Hunk Context

When multiple hunks are present in a patch, the engine collects all hunk bodies in a first pass, then processes each hunk with knowledge of the adjacent hunks' context lines. This allows the scoring system to disambiguate identical code blocks by verifying that the surrounding file content matches what neighboring hunks expect. See the "Adjacent Hunk Context Disambiguation" section under Match Resolution and Scoring for details.

## Performance Considerations

- **Early Exit**: The tiered approach ensures that well-formed, strict patches are processed quickly without ever triggering the more expensive normalization or lowercasing logic of the Resilient and Fuzzy tiers.
- **Search Window**: While the search is greedy, it prioritizes the area immediately following the last successful hunk.

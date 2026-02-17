# Patch Application and Matching Strategy

This document specifies the matching and application logic for the `FILE_PATCH` directive. The goal of this strategy is to provide high reliability when applying LLM-generated patches, which often contain minor whitespace inaccuracies, casing errors, or incomplete context.

## Simplified Unified Diff Format

The `FILE_PATCH` directive uses a numberless version of the Unified Diff format.

- **Hunk Header**: Uses a single `@@` on its own line. It does not require line numbers (e.g., `@@ -1,5 +1,6 @@`).
- **Context/Removal/Addition**: Follows the standard ` `, `-`, and `+` prefix conventions.
- **Completion**: The applier "completes" these simplified hunks by locating the context in the target file and generating a standard Unified Diff that tools like `diffy` can process.

## Tiered Matching Logic

To balance accuracy with resilience, the patch completion engine employs a three-tier matching strategy. It attempts to find a match using the strictest criteria first, falling back to more lenient tiers only if no candidates are found.

| Tier | Name | Description |
| --- | --- | --- |
| 1 | **Strict** | Character-for-character exact match. No trimming or normalization is performed. This provides the highest confidence. |
| 2 | **Resilient** | Case-sensitive match using trimmed lines, normalized whitespace (multiple spaces collapsed to one), and suffix matching for truncated lines. |
| 3 | **Fuzzy** | Same as Resilient tier but performed case-insensitively. This handles LLM casing discrepancies in headers or comments. |

### Execution Flow

The engine iterates through these tiers in order. If a tier yields one or more candidate matches, the search stops, and the best candidate from that tier is selected. If no candidates are found after all tiers are exhausted, the patch application fails.

## Match Resolution and Scoring

When multiple candidates are found within the same tier, a scoring system determines the best fit.

### Proximity

The search for hunk context begins at the end of the previously applied hunk (tracking cumulative line-count deltas). Proximity to this expected location is a key factor in scoring to prevent matching similar code blocks far apart in a file.

### Exact Whitespace Count

Within the Resilient and Fuzzy tiers, candidates that have more "exact" matches (where the line matched without needing normalization) are scored higher. This ensures that even in lenient tiers, the most visually similar block is preferred.

## Structural Resilience

The strategy includes specific handlers for common LLM formatting artifacts.

### Suffix Matching

If a context line in a patch is a suffix of an original line (minimum 10 characters), it is considered a match. This allows the LLM to provide only the trailing part of a long line as context.

### Blank Line Alignment

LLMs often insert "cosmetic" blank lines in patches for readability. 

- If a patch contains a blank context line that aligns with a blank line in the source, it matches normally.
- If a blank context line does not align with a blank line in the source, the engine converts that hunk line into an addition (`+`) line. This preserves the LLM's intended spacing without causing alignment drift for subsequent context lines.

### EOF Overhang

If the context lines in a hunk extend beyond the end of the file, they are treated as "overhang" and dropped, provided they are context lines (` `) and not removal lines (`-`). This allows patches to include trailing context that the LLM incorrectly assumed existed at the end of a file.

## Performance Considerations

- **Early Exit**: The tiered approach ensures that well-formed, strict patches are processed quickly without ever triggering the more expensive normalization or lowercasing logic of the Resilient and Fuzzy tiers.
- **Search Window**: While the search is greedy, it prioritizes the area immediately following the last successful hunk.

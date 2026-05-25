# SecurityPolicy

## Intent

Provide a configurable, safe‑by‑default security policy that controls path‑traversal checks for file‑change operations (read and write) within the `udiffx` library.

The policy is passed to `apply_file_changes` as an optional parameter. When omitted (`None`), the current strict behavior applies: all file operations must stay inside a single `base_dir`.

## Code Design

```rust
pub struct SecurityPolicy {
    /// Directories where writes are allowed.
    /// If empty, writes are restricted to the operation's `base_dir`.
    pub writable_dirs: Vec<SPath>,

    /// When `true`, allow reading from any path, even outside the
    /// writable directories.
    pub read_anywhere: bool,

    /// When `true`, **all** path checks are disabled.
    pub bypass_all_checks: bool,
}
```

### Fluid API

No separate builder is used; the struct provides a self‑consuming fluent API.

- `SecurityPolicy::default()` – strict policy: writes restricted to `base_dir`, reads also restricted.
- `SecurityPolicy::trusted_cwd()` – trust the entire current working directory; `writable_dirs` is populated with the CWD path.
- `SecurityPolicy::from_writable_dirs(dirs)` – construct a policy with only `writable_dirs` filled.
- `append_writable_dir(dir: impl Into<SPath>) -> Self` – push an additional writable directory.
- `append_writable_dirs(dirs: impl IntoIterator<Item = impl Into<SPath>>) -> Self` – append multiple writable directories.
- `with_writable_dirs(dirs: impl IntoIterator<Item = impl Into<SPath>>) -> Self` – replace the writable directories.
- `with_read_anywhere() -> Self` – set `read_anywhere = true`.
- `with_bypass_all_checks() -> Self` – set `bypass_all_checks = true`.

### Integration

`apply_file_changes` signature gains an optional parameter:

```rust
pub fn apply_file_changes(
    base_dir: impl Into<SPath>,
    file_changes: FileChanges,
    security_policy: impl Into<SecurityPolicy>,
) -> Result<ApplyChangesStatus> { ... }
```

The `security_policy` parameter accepts `Option<SecurityPolicy>`, `SecurityPolicy`, or anything convertible into `SecurityPolicy`.  
When `None` (or `SecurityPolicy::default()`) the strict default applies.

Internal `fs_guard` functions will be extended to receive the policy and branch accordingly:

- **Writes**: allowed if `target` is under `base_dir` **or** under any directory in `writable_dirs`.
- **Reads**: if `read_anywhere` is `true`, no check is performed; otherwise, same as writes.
- **Bypass**: when `bypass_all_checks` is `true`, all checks are skipped.

## Design Considerations

- **Safe by default** – the current behavior is preserved; `None` yields the strictest policy.
- **Fluid, no builder** – the struct itself serves as a builder, reducing boilerplate and keeping the API simple.
- **Naming** – `writable_dirs` is concise, and `append_writable_dir` uses the singular to match “append one directory”. The verb “dir” is used rather than “dirs” for grammatical consistency.
- **Extensibility** – future permissions (e.g., specific file patterns) can be added as additional fields without changing the fluent pattern.
- **Integration footprint** – only a single optional parameter is added to `apply_file_changes`; the internal guard functions are updated to interpret the policy.

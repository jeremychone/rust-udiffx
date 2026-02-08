//! Integration tests for applying extracted FILE_CHANGES fixtures.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

use simple_fs::SPath;
use udiffx::{apply_file_changes, extract_file_changes};

mod test_support;

#[test]
fn test_changes_no_changes() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("tests_changes_no_changes")?;
	let input = include_str!("data/changes-no-changes.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	assert!(
		status.items.is_empty() || status.items.iter().all(|i| i.success()),
		"Expected no failures, got: {status:#?}"
	);

	Ok(())
}

#[test]
fn test_changes_001() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("test_changes_001")?;
	simple_fs::ensure_dir(&base_dir)?;

	let original_file_path = base_dir.join("original.md");

	let initial_content = r###"# Dev Chat

Update this dev chat history with each of you answer with the `## Request: ...` title/section for each ask (concise title)

## Request: Code Improvement Assessment

The current implementation provides a solid foundation for a button explosion effect. To reach production-grade quality, the following improvements are suggested:

- **Canvas Scaling**: Implement `devicePixelRatio` handling to ensure sharp rendering on high-resolution displays.
- **Animation Control**: Guard the `animate` loop to prevent multiple concurrent `requestAnimationFrame` instances if the logic is expanded.
- **Resource Management**: Implement a particle pool to reduce GC pressure from constant object instantiation.
- **Interaction UX**: Transition from a hardcoded `setTimeout` to a state-based reset that triggers when all particles have faded out.
- **Accessibility**: Add ARIA attributes to handle the temporary disappearance of the primary interaction element.
"###;

	std::fs::write(&original_file_path, initial_content)?;

	let input = r###"
<FILE_CHANGES>
<FILE_PATCH file_path="original.md">
```md
@@
 - **Accessibility**: Add ARIA attributes to handle the temporary disappearance of the primary interaction element.

+## Request: Architecture Refactoring
+
+Splitting the JavaScript files is a highly recommended step for maintainability. As the project grows, separating concerns improves readability and simplifies testing:
+
+- **Particle Module**: Move the `Particle` class to a dedicated file to encapsulate particle-specific logic.
+- **Animation Engine**: Extract the core canvas setup and the `requestAnimationFrame` loop.
+- **Application Entry**: Use `js/main.js` strictly for DOM initialization and orchestrating interactions.
+
+This modular structure follows modern development standards and leverages ES module capabilities.
+
```
</FILE_PATCH>
</FILE_CHANGES>
"###;
	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	assert_eq!(status.items.len(), 1, "Should have 1 directive status");
	assert!(
		status.items[0].success,
		"Directive should have succeeded. Error: {:?}",
		status.items[0].error_msg
	);

	let final_content = std::fs::read_to_string(original_file_path)?;
	assert!(final_content.contains("## Request: Architecture Refactoring"));
	assert!(final_content.contains(
		"- **Particle Module**: Move the `Particle` class to a dedicated file to encapsulate particle-specific logic."
	));

	Ok(())
}

#[test]
fn test_changes_with_newline_surround() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("test_changes_with_newline_surround")?;
	let keys_conf_path = base_dir.join("keys.conf");
	let initial_content = r###"# bind m send-keys "tmux list-panes -a -F '#{?session_attached,ATTACHED,DETACHED} #S:#I.#P \"#{window_name}\" #{pane_current_path} #{pane_current_command}'" Enter

## Disabled for now, since tmux-plugins
# bind-key -T copy-mode-vi o send-keys -X copy-pipe-and-cancel "pbpaste | xargs open"
# bind-key -T copy-mode o    send-keys -X copy-pipe-and-cancel "pbpaste | xargs open"


# Clear right panels
bind K send-keys -t 2 "clear" Enter "\\" Enter C-l \; send-keys -t 3 "clear" Enter "\\" Enter C-l
"###;
	std::fs::write(&keys_conf_path, initial_content)?;

	let input = r#"
<FILE_CHANGES>

<FILE_PATCH file_path="keys.conf">
```conf
@@
 ## Disabled for now, since tmux-plugins
 # bind-key -T copy-mode-vi o send-keys -X copy-pipe-and-cancel "pbpaste | xargs open"
 # bind-key -T copy-mode o    send-keys -X copy-pipe-and-cancel "pbpaste | xargs open"
+bind-key -T copy-mode-vi o send-keys -X copy-pipe-and-cancel "xargs open"
+bind-key -T copy-mode    o send-keys -X copy-pipe-and-cancel "xargs open"


 # Clear right panels
```
</FILE_PATCH>

</FILE_CHANGES>
"#;

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	assert_eq!(status.items.len(), 1, "Should have 1 directive status");
	assert!(
		status.items[0].success,
		"Directive should have succeeded. Error: {:?}",
		status.items[0].error_msg
	);

	let final_content = std::fs::read_to_string(keys_conf_path)?;
	assert!(final_content.contains("bind-key -T copy-mode-vi o send-keys -X copy-pipe-and-cancel \"xargs open\""));
	assert!(final_content.contains("bind-key -T copy-mode    o send-keys -X copy-pipe-and-cancel \"xargs open\""));

	Ok(())
}

#[test]
fn test_changes_simple() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("test_changes_simple")?;
	let input = include_str!("data/changes-simple.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	let len = status.items.len();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.items.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");

	Ok(())
}

#[test]
fn test_changes_no_head_nums() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("test_changes_no_head_nums")?;
	let input = include_str!("data/changes-no-head-nums.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir, changes)?;

	// -- Check
	let len = status.items.len();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.items.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");
	// check main.rs
	let main_content = simple_fs::read_to_string(base_dir.join("src/main.rs"))?;
	assert!(
		main_content.contains("hello::hello()"),
		"main.rs should contain 'hello::hello()'"
	);

	Ok(())
}

#[test]
fn test_changes_with_code_fence() -> Result<()> {
	// -- Setup & Fixtures
	let base_dir = test_support::new_out_dir_path("tests_changes_with_code_fence")?;
	let base_dir_spath = SPath::new(&base_dir);
	let input = include_str!("data/changes-with-code-fence.md");

	// -- Exec
	let (changes, _extruded) = extract_file_changes(input, false)?;
	let status = apply_file_changes(&base_dir_spath, changes)?;

	// -- Check
	let len = status.items.len();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.items.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");

	Ok(())
}

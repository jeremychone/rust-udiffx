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
	let len = status.items.iter().count();
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
	let len = status.items.iter().count();
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
	let len = status.items.iter().count();
	assert_eq!(5, len, "Wrong directive length");
	let success_count = status.items.iter().filter(|i| i.success()).count();
	assert_eq!(3, success_count, "Wrong success count");

	Ok(())
}

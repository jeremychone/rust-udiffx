// region:    --- Tests

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

use crate::applier::apply_patch_incremental;

#[test]
fn test_applier_apply_patch_incremental_noop_hunks_do_not_fail() -> Result<()> {
	// -- Setup & Fixtures
	let original = r#"async fn run() {
	match agent_res {
		Ok(_agent) => {
			let (redo_ctx, redo_requested) = exec_run(run_args, runtime).await?;
			self.set_current_redo_ctx(redo_ctx).await;
		}
	}
}
"#;

	let patch_raw = r#"@@
 	match agent_res {
 		Ok(_agent) => {
-			let (redo_ctx, redo_requested) = exec_run(run_args, runtime).await?;
+			let (redo_ctx, redo_requested) = exec_run(run_args, runtime).await?;
 			self.set_current_redo_ctx(redo_ctx).await;
 		}
"#;

	// -- Exec
	let (content, _mt, _error_hunks, _num) = apply_patch_incremental(original, patch_raw)?;

	// -- Check
	assert_eq!(content, original);

	Ok(())
}

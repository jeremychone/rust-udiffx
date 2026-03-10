Some text before

<TEST_FILE_CHANGES>

<TEST_FILE_NEW file_path="src/hello.rs">
pub fn hello() {
    println!("Hello");
}
</TEST_FILE_NEW>

<TEST_FILE_PATCH file_path="src/lib.rs">
@@
 pub use applier::apply_file_changes;
 pub use apply_changes_status::*;
 pub use error::*;
</TEST_FILE_PATCH>

<TEST_FILE_PATCH file_path="src/main.rs">
@@
-fn main() {
+fn main() -> Result<()> {
     println!("Hello");
+    Ok(())
 }
</TEST_FILE_PATCH>

</TEST_FILE_CHANGES>

Some text after

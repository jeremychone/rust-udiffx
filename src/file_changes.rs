use crate::FileDirective;

#[derive(Debug, Clone)]
pub struct FileChanges {
	directives: Vec<FileDirective>,
}

impl FileChanges {
	pub fn new(directives: Vec<FileDirective>) -> Self {
		Self { directives }
	}

	pub fn is_empty(&self) -> bool {
		self.directives.is_empty()
	}
}

// region:    --- Iterators

impl FileChanges {
	pub fn iter(&self) -> std::slice::Iter<'_, FileDirective> {
		self.directives.iter()
	}
}

impl IntoIterator for FileChanges {
	type Item = FileDirective;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		self.directives.into_iter()
	}
}

impl<'a> IntoIterator for &'a FileChanges {
	type Item = &'a FileDirective;
	type IntoIter = std::slice::Iter<'a, FileDirective>;

	fn into_iter(self) -> Self::IntoIter {
		self.directives.iter()
	}
}

// endregion: --- Iterators

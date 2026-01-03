//! Some support utilities for the tests
//! Note: Must be imported in each test file

#![allow(unused)] // For test support

// region:    --- Modules

mod helpers;

pub use helpers::*;

type TestResult<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

// endregion: --- Modules

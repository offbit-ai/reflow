//! Math operation actors matching Zeal's tools-utilities/math templates.
//!
//! Each actor takes numeric inputs on ports A/B (or a list), applies the
//! operation, and outputs the result. All support configurable precision.

mod ops;

pub use ops::*;

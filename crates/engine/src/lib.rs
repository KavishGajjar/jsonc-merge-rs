//! Reviewable three-way JSONC merges with explicit record identities.
//!
//! ```
//! use jsonc_merge::{Document, Policy, merge};
//! let base = Document::parse(r#"{"a": 1, "b": 1}"#)?;
//! let ours = Document::parse(r#"{"a": 2, "b": 1}"#)?;
//! let theirs = Document::parse(r#"{"a": 1, "b": 2}"#)?;
//! let plan = merge(&base, &ours, &theirs, &Policy::default())?;
//! assert!(plan.is_clean());
//! assert!(plan.render()?.contains("2"));
//! # Ok::<(), jsonc_merge::Diagnostic>(())
//! ```

mod diagnostic;
mod model;
mod order;
mod plan;
mod policy;
mod reconcile;
mod render;
mod source;

pub use diagnostic::{Diagnostic, DiagnosticCode, Limits, Span};
pub use plan::{
    Conflict, ConflictKind, MergePlan, Origin, PathSegment, Provenance, Report, Side, SourceEdit,
};
pub use policy::{ArrayOrder, ArrayPolicy, Policy};
pub use reconcile::{merge, merge_with_limits};
pub use source::Document;

// Compile the public README example as part of the ordinary doctest suite.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
mod readme {}

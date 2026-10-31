use serde::Serialize;
use std::fmt;

/// Half-open UTF-8 byte offsets in an input snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Stable categories; diagnostic prose is intended for people.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    Syntax,
    DuplicateKey,
    Policy,
    Identity,
    Budget,
    Conflict,
    StalePlan,
    Render,
}

/// An input, policy, resource or API error, distinct from a merge conflict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub message: String,
    pub span: Option<Span>,
    /// Populated for input validation performed by a three-way merge.
    pub input: Option<crate::Side>,
}

impl Diagnostic {
    pub(crate) fn new(
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            input: None,
        }
    }
    pub(crate) fn budget(message: impl Into<String>) -> Self {
        Self::new(DiagnosticCode::Budget, message, None)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(side) = self.input {
            write!(f, "{side:?} ")?;
        }
        write!(f, "{:?}: {}", self.code, self.message)?;
        if let Some(span) = self.span {
            write!(f, " (bytes {}..{})", span.start, span.end)?;
        }
        Ok(())
    }
}
impl std::error::Error for Diagnostic {}

/// Explicit resource bounds. Exceeding one returns `Budget`, never a conflict.
/// Depths above 128 are unsupported, even when a caller requests a larger bound.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Comment tokens per input/output, independent of value-node count.
    pub max_comments: usize,
    pub max_number_bytes: usize,
    pub max_order_pairs: usize,
    /// Combined path-storage estimate across the three merge inputs.
    /// Each component costs 32 bytes plus its decoded key/identity bytes.
    pub max_path_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 8 * 1024 * 1024,
            max_depth: 128,
            max_nodes: 200_000,
            max_comments: 200_000,
            max_number_bytes: 4096,
            max_order_pairs: 1_000_000,
            max_path_bytes: 32 * 1024 * 1024,
        }
    }
}

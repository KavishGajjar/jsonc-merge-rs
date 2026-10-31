use crate::model::{Comment, Node};
use crate::{Diagnostic, DiagnosticCode, Document, Limits, Policy, Span};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// An immutable input snapshot within a merge plan.
pub enum Side {
    Base,
    Ours,
    Theirs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// Half-open byte range in the identified input snapshot.
pub struct Origin {
    pub side: Side,
    pub span: Span,
}

/// Semantic paths can identify records independently of their array offsets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSegment {
    Key(String),
    Record(String),
    Index(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Semantic reason why the engine cannot select a clean result.
pub enum ConflictKind {
    Value,
    AtomicArray,
    DeleteEdit,
    Comment,
    Order,
}

#[derive(Clone, Debug, Serialize)]
/// An unresolved semantic location and its available source alternatives.
pub struct Conflict {
    pub kind: ConflictKind,
    pub path: Vec<PathSegment>,
    /// Absent alternatives represent deletion; spans point to immutable input snapshots.
    pub base: Option<Origin>,
    pub ours: Option<Origin>,
    pub theirs: Option<Origin>,
    pub order_witness: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// Replacement against the original ours snapshot, not the intermediate output.
pub struct SourceEdit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, Serialize)]
/// Contributing source fragments at a semantic location; ranges can overlap.
pub struct Provenance {
    pub path: Vec<PathSegment>,
    pub origins: Vec<Origin>,
}

#[derive(Clone, Debug, Serialize)]
/// Versioned inspection data. This is not an executable saved patch format.
pub struct Report {
    pub schema_version: u32,
    pub clean: bool,
    pub conflicts: Vec<Conflict>,
    pub provenance: Vec<Provenance>,
}

/// Immutable review result. Conflicted plans cannot render a configuration.
#[derive(Clone, Debug)]
pub struct MergePlan {
    pub(crate) inputs: [Document; 3],
    pub(crate) root: Option<Arc<Node>>,
    pub(crate) header: Vec<Comment>,
    pub(crate) footer: Vec<Comment>,
    pub(crate) conflicts: Vec<Conflict>,
    pub(crate) provenance: Vec<Provenance>,
    pub(crate) policy: Policy,
    pub(crate) limits: Limits,
}

impl MergePlan {
    /// Whether there are no semantic conflicts. Rendering may still exceed budgets.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
    /// Deterministically ordered unresolved conflicts.
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }
    /// Inspection data with source spans relative to these inputs.
    pub fn report(&self) -> Report {
        Report {
            schema_version: 1,
            clean: self.is_clean(),
            conflicts: self.conflicts.clone(),
            provenance: self.provenance.clone(),
        }
    }
    /// Render a clean plan, reparse it, and verify its values/comment ownership.
    /// Conflicted plans return `Conflict`; inputs remain untouched on every path.
    pub fn render(&self) -> Result<String, Diagnostic> {
        crate::render::render(self)
    }

    /// A single changed-region edit, reduced by common UTF-8 prefix/suffix.
    /// This is ordered/non-overlapping, but does not promise a minimal edit program.
    pub fn source_edits(&self) -> Result<Vec<SourceEdit>, Diagnostic> {
        let output = self.render()?;
        let ours = self.inputs[1].source();
        if output == ours {
            return Ok(vec![]);
        }
        let mut prefix = ours
            .bytes()
            .zip(output.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !ours.is_char_boundary(prefix) || !output.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let mut suffix = ours.as_bytes()[prefix..]
            .iter()
            .rev()
            .zip(output.as_bytes()[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        while !ours.is_char_boundary(ours.len() - suffix)
            || !output.is_char_boundary(output.len() - suffix)
        {
            suffix -= 1;
        }
        Ok(vec![SourceEdit {
            span: Span {
                start: prefix,
                end: ours.len() - suffix,
            },
            replacement: output[prefix..output.len() - suffix].into(),
        }])
    }

    /// Reject stale external buffers before producing any output.
    pub fn apply_to(&self, original: &str) -> Result<String, Diagnostic> {
        if original != self.inputs[1].source() {
            return Err(Diagnostic::new(
                DiagnosticCode::StalePlan,
                "buffer differs from the ours snapshot",
                None,
            ));
        }
        self.render()
    }

    pub(crate) fn fragment(&self, origin: &Origin) -> &str {
        let index = match origin.side {
            Side::Base => 0,
            Side::Ours => 1,
            Side::Theirs => 2,
        };
        &self.inputs[index].source()[origin.span.start..origin.span.end]
    }
}

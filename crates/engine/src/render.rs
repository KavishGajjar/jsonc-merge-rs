use crate::model::{Comment, Kind, Node, comments_eq, merge_eq};
use crate::{Diagnostic, DiagnosticCode, Document, MergePlan, Origin, Side};

pub(crate) fn render(plan: &MergePlan) -> Result<String, Diagnostic> {
    if !plan.is_clean() {
        return Err(Diagnostic::new(
            DiagnosticCode::Conflict,
            "resolve the reported conflicts before rendering",
            None,
        ));
    }
    let root = plan.root.as_ref().expect("a clean plan has a root");
    let renderer = Renderer {
        plan,
        newline: if plan.inputs[1].source().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        },
    };
    let mut output = renderer.boundary(&plan.header, true);
    output.push_str(&renderer.node(root, 0));
    output.push_str(&renderer.boundary(&plan.footer, false));
    let parsed = Document::parse_with_limits(output.as_str(), plan.limits)?;
    plan.policy.validate(&parsed)?;
    if !merge_eq(root, &parsed.root)
        || !comments_eq(&plan.header, &parsed.header)
        || !comments_eq(&plan.footer, &parsed.footer)
    {
        return Err(Diagnostic::new(
            DiagnosticCode::Render,
            "rendered content or comment ownership differs from the merge plan",
            None,
        ));
    }
    Ok(output)
}

struct Renderer<'a> {
    plan: &'a MergePlan,
    newline: &'static str,
}

impl Renderer<'_> {
    fn node(&self, node: &Node, depth: usize) -> String {
        if let Some(raw) = &node.raw {
            return self.plan.fragment(raw).into();
        }
        let (entries, open, close) = match &node.kind {
            Kind::Object(e) => (e, '{', '}'),
            Kind::Array(e) => (e, '[', ']'),
            _ => unreachable!("changed scalars always retain a selected source"),
        };
        if let Some(layout) = &node.layout
            && let Some(patched) = self.surgical(node, layout, depth)
        {
            return patched;
        }
        let indent = "  ".repeat(depth);
        let child_indent = "  ".repeat(depth + 1);
        let mut output = String::from(open);
        if entries.is_empty() && node.dangling.is_empty() {
            output.push(close);
            return output;
        }
        output.push_str(self.newline);
        for (i, entry) in entries.iter().enumerate() {
            output.push_str(&child_indent);
            if let Some(layout) = &entry.leading_layout {
                output.push_str(&without_separator_commas(self.plan.fragment(layout)));
            } else if !entry.leading.is_empty() {
                for comment in &entry.leading {
                    self.full_comment(&mut output, comment, &child_indent);
                }
                output.push_str(&child_indent);
            }
            if let Some(key) = &entry.key {
                if let Some(prefix) = &entry.prefix {
                    output.push_str(self.plan.fragment(prefix));
                } else {
                    output.push_str(&serde_json::to_string(key).unwrap());
                    output.push(':');
                    for c in &entry.internal {
                        output.push(' ');
                        output.push_str(&c.text);
                        if c.text.starts_with("//") {
                            output.push_str(self.newline);
                            output.push_str(&child_indent);
                        }
                    }
                    output.push(' ');
                }
            }
            let text = self.node(&entry.node, depth + 1);
            output.push_str(&text);
            if i + 1 < entries.len() {
                output.push(',');
            }
            if let Some(layout) = &entry.trailing_layout {
                output.push_str(&without_separator_commas(self.plan.fragment(layout)));
            } else {
                for c in &entry.trailing {
                    output.push(' ');
                    output.push_str(&c.text);
                }
            }
            output.push_str(self.newline);
        }
        for comment in &node.dangling {
            self.full_comment(&mut output, comment, &child_indent);
        }
        output.push_str(&indent);
        output.push(close);
        output
    }

    fn surgical(&self, node: &Node, layout: &Node, depth: usize) -> Option<String> {
        let ((Kind::Object(a), Kind::Object(b)) | (Kind::Array(a), Kind::Array(b))) =
            (&node.kind, &layout.kind)
        else {
            return None;
        };
        let origin = layout.raw.as_ref()?;
        if a.len() != b.len() || !comments_eq(&node.dangling, &layout.dangling) {
            return None;
        }
        if !a.iter().zip(b).all(|(a, b)| {
            a.key == b.key
                && a.origin == b.origin
                && comments_eq(&a.leading, &b.leading)
                && comments_eq(&a.internal, &b.internal)
                && comments_eq(&a.trailing, &b.trailing)
        }) {
            return None;
        }
        let mut output = String::new();
        let mut at = origin.span.start;
        for (merged, original) in a.iter().zip(b) {
            let child = original.node.raw.as_ref()?;
            if child.side != origin.side
                || child.span.start < at
                || child.span.end > origin.span.end
            {
                return None;
            }
            output.push_str(self.range(origin.side, at, child.span.start));
            output.push_str(&self.node(&merged.node, depth + 1));
            at = child.span.end;
        }
        output.push_str(self.range(origin.side, at, origin.span.end));
        Some(output)
    }

    fn range(&self, side: Side, start: usize, end: usize) -> &str {
        self.plan.fragment(&Origin {
            side,
            span: crate::Span { start, end },
        })
    }

    fn full_comment(&self, output: &mut String, comment: &Comment, indent: &str) {
        output.push_str(indent);
        output.push_str(&comment.text);
        output.push_str(self.newline);
    }

    fn boundary(&self, selected: &[Comment], header: bool) -> String {
        for (side, doc) in [Side::Ours, Side::Theirs, Side::Base].into_iter().zip([
            &self.plan.inputs[1],
            &self.plan.inputs[2],
            &self.plan.inputs[0],
        ]) {
            if comments_eq(selected, if header { &doc.header } else { &doc.footer }) {
                let root = doc.root.raw.as_ref().unwrap().span;
                return if header {
                    self.range(side, 0, root.start).into()
                } else {
                    self.range(side, root.end, doc.source().len()).into()
                };
            }
        }
        unreachable!("three-way comment selection always chooses an input sequence")
    }
}

/// Gaps contain only whitespace, comments and separators. Commas inside comments
/// are data and must survive, including when the original comma followed a comment.
fn without_separator_commas(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::new();
    let mut at = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i..i + 2) == Some(b"//") {
            i += 2;
            while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
                i += 1;
            }
        } else if bytes.get(i..i + 2) == Some(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                i += 1;
            }
            i += 2;
        } else if bytes[i] == b',' {
            output.push_str(&text[at..i]);
            i += 1;
            at = i;
        } else {
            i += 1;
        }
    }
    output.push_str(&text[at..]);
    output
}

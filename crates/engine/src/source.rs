use crate::model::{Comment, Decimal, Entry, Kind, Node, span};
use crate::{Diagnostic, DiagnosticCode as Code, Limits, Origin, Side, Span};
use jsonc_parser::{CollectOptions, ParseOptions, ast, common::Ranged, parse_to_ast};
use std::{collections::BTreeSet, sync::Arc};

/// An immutable, owned source snapshot. Cloning is cheap and never aliases mutable CST state.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) source: Arc<str>,
    pub(crate) root: Arc<Node>,
    pub(crate) header: Arc<[Comment]>,
    pub(crate) footer: Arc<[Comment]>,
    metrics: Metrics,
}

#[derive(Clone, Copy, Debug)]
struct Metrics {
    nodes: usize,
    depth: usize,
    number_bytes: usize,
    comments: usize,
}

impl Document {
    /// Parse one strict-profile JSONC value into an owned immutable snapshot.
    /// Comments/trailing commas are accepted; duplicate decoded keys are errors.
    pub fn parse(source: impl AsRef<str>) -> Result<Self, Diagnostic> {
        Self::parse_with_limits(source, Limits::default())
    }

    /// Parse with explicit budgets. Nesting deeper than 128 is always rejected.
    pub fn parse_with_limits(source: impl AsRef<str>, limits: Limits) -> Result<Self, Diagnostic> {
        if source.as_ref().len() > limits.max_input_bytes {
            return Err(Diagnostic::budget("input byte limit exceeded"));
        }
        let source: Arc<str> = source.as_ref().into();
        let (comments, depth) = preflight(&source, limits)?;
        let options = ParseOptions {
            allow_comments: true,
            allow_trailing_commas: true,
            allow_loose_object_property_names: false,
            allow_missing_commas: false,
            allow_single_quoted_strings: false,
            allow_hexadecimal_numbers: false,
            allow_unary_plus_numbers: false,
            allow_bare_decimal_point_numbers: false,
            allow_non_finite_numbers: false,
            allow_extended_string_escapes: false,
        };
        let result = parse_to_ast(&source, &CollectOptions::default(), &options).map_err(|e| {
            Diagnostic::new(
                Code::Syntax,
                e.to_string(),
                Some(span(e.range().start, e.range().end)),
            )
        })?;
        let value = result
            .value
            .ok_or_else(|| Diagnostic::new(Code::Syntax, "expected one JSON value", None))?;
        let mut adapter = Adapter {
            source: &source,
            comments: &comments,
            nodes: 0,
            number_bytes: 0,
            limits,
        };
        let root = adapter.lower(&value)?;
        let root_span = root.raw.as_ref().unwrap().span;
        let header = comments
            .iter()
            .filter(|c| c.end <= root_span.start)
            .map(|s| adapter.comment(*s))
            .collect();
        let footer = comments
            .iter()
            .filter(|c| c.start >= root_span.end)
            .map(|s| adapter.comment(*s))
            .collect();
        let metrics = Metrics {
            nodes: adapter.nodes,
            depth,
            number_bytes: adapter.number_bytes,
            comments: comments.len(),
        };
        Ok(Self {
            source,
            root,
            header,
            footer,
            metrics,
        })
    }

    /// Exact original UTF-8 text, including surrounding whitespace and comments.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Exact JSON value equality; ignores comments, whitespace and object member order.
    /// Array order is significant here (this method does not apply a merge policy).
    pub fn semantically_eq(&self, other: &Self) -> bool {
        crate::model::value_eq(&self.root, &other.root)
    }

    /// One-based line and Unicode-scalar column for a byte offset, or `None` at a non-boundary.
    pub fn line_column(&self, offset: usize) -> Option<(usize, usize)> {
        let before = self.source.get(..offset)?;
        let (mut line, mut column, mut carriage) = (1, 1, false);
        for ch in before.chars() {
            match ch {
                '\r' => {
                    line += 1;
                    column = 1;
                    carriage = true;
                }
                '\n' => {
                    if !carriage {
                        line += 1;
                    }
                    column = 1;
                    carriage = false;
                }
                _ => {
                    column += 1;
                    carriage = false;
                }
            }
        }
        Some((line, column))
    }

    pub(crate) fn imported(&self, side: Side) -> Arc<Node> {
        import(&self.root, side)
    }

    pub(crate) fn check_limits(&self, limits: Limits) -> Result<(), Diagnostic> {
        if self.source.len() > limits.max_input_bytes
            || self.metrics.nodes > limits.max_nodes
            || self.metrics.depth > limits.max_depth.min(128)
            || self.metrics.number_bytes > limits.max_number_bytes
            || self.metrics.comments > limits.max_comments
        {
            return Err(Diagnostic::budget(
                "snapshot exceeds the merge resource limits",
            ));
        }
        Ok(())
    }
}

fn import(node: &Node, side: Side) -> Arc<Node> {
    let mut node = node.clone();
    node.raw.as_mut().unwrap().side = side;
    for c in &mut node.dangling {
        c.origin.side = side;
    }
    if let Kind::Object(entries) | Kind::Array(entries) = &mut node.kind {
        for entry in entries {
            entry.node = import(&entry.node, side);
            for c in entry
                .leading
                .iter_mut()
                .chain(&mut entry.internal)
                .chain(&mut entry.trailing)
            {
                c.origin.side = side;
            }
            if let Some(o) = &mut entry.origin {
                o.side = side;
            }
            if let Some(o) = &mut entry.prefix {
                o.side = side;
            }
            if let Some(o) = &mut entry.leading_layout {
                o.side = side;
            }
            if let Some(o) = &mut entry.trailing_layout {
                o.side = side;
            }
        }
    }
    Arc::new(node)
}

pub(crate) fn import_comments(comments: &[Comment], side: Side) -> Vec<Comment> {
    comments
        .iter()
        .map(|c| {
            let mut c = c.clone();
            c.origin.side = side;
            c
        })
        .collect()
}

struct Adapter<'a> {
    source: &'a str,
    comments: &'a [Span],
    nodes: usize,
    number_bytes: usize,
    limits: Limits,
}

impl Adapter<'_> {
    fn comment(&self, span: Span) -> Comment {
        Comment {
            text: self.source[span.start..span.end].into(),
            origin: Origin {
                side: Side::Ours,
                span,
            },
        }
    }
    fn comments(&self, start: usize, end: usize) -> Vec<Comment> {
        // Binary search avoids rescanning all document comments for every member.
        let first = self.comments.partition_point(|s| s.start < start);
        self.comments[first..]
            .iter()
            .take_while(|s| s.end <= end)
            .map(|s| self.comment(*s))
            .collect()
    }
    fn lower(&mut self, value: &ast::Value<'_>) -> Result<Arc<Node>, Diagnostic> {
        self.nodes += 1;
        if self.nodes > self.limits.max_nodes {
            return Err(Diagnostic::budget("node limit exceeded"));
        }
        let r = value.range();
        let range = span(r.start, r.end);
        let mut dangling = Vec::new();
        let kind = match value {
            ast::Value::NullKeyword(_) => Kind::Null,
            ast::Value::BooleanLit(b) => Kind::Bool(b.value),
            ast::Value::StringLit(_) => Kind::String(self.string(range)?),
            ast::Value::NumberLit(n) => {
                self.number_bytes = self.number_bytes.max(n.value.len());
                if n.value.len() > self.limits.max_number_bytes {
                    return Err(Diagnostic::budget("number token byte limit exceeded"));
                }
                Kind::Number(Decimal::parse(n.value).ok_or_else(|| {
                    Diagnostic::new(Code::Syntax, "invalid JSON number", Some(range))
                })?)
            }
            ast::Value::Object(obj) => {
                let mut entries = Vec::new();
                let mut keys = BTreeSet::new();
                for prop in &obj.properties {
                    let name = prop.name.range();
                    let key_span = span(name.start, name.end);
                    let key = self.string(key_span)?;
                    if !keys.insert(key.clone()) {
                        return Err(Diagnostic::new(
                            Code::DuplicateKey,
                            format!("duplicate decoded key {key:?}"),
                            Some(key_span),
                        ));
                    }
                    let nr = prop.value.range();
                    entries.push(Entry {
                        key: Some(key),
                        node: self.lower(&prop.value)?,
                        leading: vec![],
                        internal: self.comments(name.end, nr.start),
                        trailing: vec![],
                        leading_layout: None,
                        trailing_layout: None,
                        prefix: Some(Origin {
                            side: Side::Ours,
                            span: span(prop.range.start, nr.start),
                        }),
                        origin: Some(Origin {
                            side: Side::Ours,
                            span: span(prop.range.start, nr.end),
                        }),
                    });
                }
                self.attach(range, &mut entries, &mut dangling);
                Kind::Object(entries)
            }
            ast::Value::Array(array) => {
                let mut entries = Vec::new();
                for value in &array.elements {
                    let r = value.range();
                    entries.push(Entry {
                        key: None,
                        node: self.lower(value)?,
                        leading: vec![],
                        internal: vec![],
                        trailing: vec![],
                        leading_layout: None,
                        trailing_layout: None,
                        prefix: None,
                        origin: Some(Origin {
                            side: Side::Ours,
                            span: span(r.start, r.end),
                        }),
                    });
                }
                self.attach(range, &mut entries, &mut dangling);
                Kind::Array(entries)
            }
        };
        Ok(Arc::new(Node {
            kind,
            dangling,
            raw: Some(Origin {
                side: Side::Ours,
                span: range,
            }),
            layout: None,
        }))
    }

    fn string(&self, range: Span) -> Result<String, Diagnostic> {
        // Strict decoding also rejects isolated UTF-16 surrogate escapes and literal controls.
        serde_json::from_str(&self.source[range.start..range.end])
            .map_err(|e| Diagnostic::new(Code::Syntax, e.to_string(), Some(range)))
    }

    fn attach(&self, container: Span, entries: &mut [Entry], dangling: &mut Vec<Comment>) {
        let mut start = container.start + 1;
        for i in 0..=entries.len() {
            let end = entries
                .get(i)
                .map_or(container.end - 1, |e| e.origin.as_ref().unwrap().span.start);
            for comment in self.comments(start, end) {
                let same_line =
                    i > 0 && !self.source[start..comment.origin.span.start].contains(['\n', '\r']);
                if same_line {
                    entries[i - 1].trailing.push(comment);
                } else if let Some(next) = entries.get_mut(i) {
                    next.leading.push(comment);
                } else {
                    dangling.push(comment);
                }
            }
            if let Some(entry) = entries.get(i) {
                start = entry.origin.as_ref().unwrap().span.end;
            }
        }
        for entry in entries {
            let core = entry.origin.as_ref().unwrap().span;
            entry.leading_layout = entry.leading.first().map(|c| Origin {
                side: Side::Ours,
                span: span(c.origin.span.start, core.start),
            });
            entry.trailing_layout = entry.trailing.last().map(|c| Origin {
                side: Side::Ours,
                span: span(core.end, c.origin.span.end),
            });
        }
    }
}

fn preflight(source: &str, limits: Limits) -> Result<(Vec<Span>, usize), Diagnostic> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    let mut max_depth = 0;
    let mut comments = Vec::new();
    // A valid JSON object has at most one property-name token per value node.
    // This conservative lexical bound limits AST allocation before lowering can
    // apply the exact node count, without counting keys as semantic value nodes.
    let mut lexical_nodes = 0usize;
    let mut token = || {
        if lexical_nodes >= limits.max_nodes.saturating_mul(2) {
            return Err(Diagnostic::budget(
                "node token limit exceeded before parsing",
            ));
        }
        lexical_nodes += 1;
        Ok(())
    };
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                token()?;
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i = (i + 2).min(bytes.len()),
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                let start = i;
                i += 2;
                while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
                    i += 1;
                }
                comments.push(span(start, i));
                if comments.len() > limits.max_comments {
                    return Err(Diagnostic::budget("comment token limit exceeded"));
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                if i + 1 >= bytes.len() {
                    return Err(Diagnostic::new(
                        Code::Syntax,
                        "unterminated block comment",
                        Some(span(start, bytes.len())),
                    ));
                }
                i += 2;
                comments.push(span(start, i));
                if comments.len() > limits.max_comments {
                    return Err(Diagnostic::budget("comment token limit exceeded"));
                }
            }
            b'[' | b'{' => {
                token()?;
                depth += 1;
                max_depth = max_depth.max(depth);
                if depth > limits.max_depth.min(128) {
                    return Err(Diagnostic::budget("container nesting limit exceeded"));
                }
                i += 1;
            }
            b']' | b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b if b >= 0x80 || (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')) => {
                let end = i + source[i..].chars().next().unwrap().len_utf8();
                return Err(Diagnostic::new(
                    Code::Syntax,
                    "only JSON whitespace is allowed outside strings/comments",
                    Some(span(i, end)),
                ));
            }
            b'-' | b'0'..=b'9' | b'n' | b't' | b'f' => {
                token()?;
                i += 1;
                while i < bytes.len()
                    && bytes[i].is_ascii_graphic()
                    && !matches!(
                        bytes[i],
                        b'[' | b']' | b'{' | b'}' | b',' | b':' | b'/' | b'"'
                    )
                {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    Ok((comments, max_depth))
}

use crate::{Origin, Span};
use num_bigint::BigInt;
use std::{collections::BTreeMap, sync::Arc};

/// Decimal coefficient × 10^exponent; no floating point or exponent expansion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    negative: bool,
    coefficient: String,
    exponent: BigInt,
}

impl Decimal {
    pub fn parse(raw: &str) -> Option<Self> {
        let bytes = raw.as_bytes();
        let mut i = usize::from(bytes.first() == Some(&b'-'));
        let negative = i == 1;
        match bytes.get(i)? {
            b'0' => i += 1,
            b'1'..=b'9' => {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            _ => return None,
        }
        let mut fraction = 0;
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            let start = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            fraction = i - start;
            if fraction == 0 {
                return None;
            }
        }
        let mantissa_end = i;
        let exponent = if matches!(bytes.get(i), Some(b'e' | b'E')) {
            i += 1;
            let start = i;
            if matches!(bytes.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            let digits = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if i == digits {
                return None;
            }
            raw[start..i].parse::<BigInt>().ok()?
        } else {
            BigInt::from(0)
        };
        if i != bytes.len() {
            return None;
        }
        let digits: String = raw[..mantissa_end]
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let significant = digits.trim_start_matches('0');
        if significant.is_empty() {
            return Some(Self {
                negative: false,
                coefficient: "0".into(),
                exponent: BigInt::from(0),
            });
        }
        let coefficient = significant.trim_end_matches('0').to_owned();
        let trailing = significant.len() - coefficient.len();
        Some(Self {
            negative,
            coefficient,
            exponent: exponent - BigInt::from(fraction) + BigInt::from(trailing),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Comment {
    pub text: String,
    pub origin: Origin,
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub key: Option<String>,
    pub node: Arc<Node>,
    pub leading: Vec<Comment>,
    pub internal: Vec<Comment>,
    pub trailing: Vec<Comment>,
    pub leading_layout: Option<Origin>,
    pub trailing_layout: Option<Origin>,
    /// Original key/colon prefix, or an empty span for an array element.
    pub prefix: Option<Origin>,
    /// Original member/element body, excluding owned outer comments.
    pub origin: Option<Origin>,
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Null,
    Bool(bool),
    String(String),
    Number(Decimal),
    Object(Vec<Entry>),
    Array(Vec<Entry>),
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub kind: Kind,
    pub dangling: Vec<Comment>,
    /// Exact reusable fragment. Cleared when a merge changes its contents.
    pub raw: Option<Origin>,
    /// Original container used for surgical child replacement where possible.
    pub layout: Option<Arc<Node>>,
}

pub(crate) fn comments_eq(a: &[Comment], b: &[Comment]) -> bool {
    a.iter().map(|c| &c.text).eq(b.iter().map(|c| &c.text))
}

pub(crate) fn value_eq(a: &Node, b: &Node) -> bool {
    match (&a.kind, &b.kind) {
        (Kind::Null, Kind::Null) => true,
        (Kind::Bool(a), Kind::Bool(b)) => a == b,
        (Kind::String(a), Kind::String(b)) => a == b,
        (Kind::Number(a), Kind::Number(b)) => a == b,
        (Kind::Object(a), Kind::Object(b)) => {
            let a: BTreeMap<_, _> = a
                .iter()
                .map(|e| (e.key.as_ref().unwrap(), &e.node))
                .collect();
            let b: BTreeMap<_, _> = b
                .iter()
                .map(|e| (e.key.as_ref().unwrap(), &e.node))
                .collect();
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| value_eq(v, w)))
        }
        (Kind::Array(a), Kind::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| value_eq(&a.node, &b.node))
        }
        _ => false,
    }
}

/// Equality including owned comments and presentation order, excluding whitespace.
pub(crate) fn content_eq(a: &Node, b: &Node) -> bool {
    if !comments_eq(&a.dangling, &b.dangling) {
        return false;
    }
    match (&a.kind, &b.kind) {
        (Kind::Object(a), Kind::Object(b)) | (Kind::Array(a), Kind::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| entry_eq(a, b))
        }
        _ => value_eq(a, b),
    }
}
pub(crate) fn entry_eq(a: &Entry, b: &Entry) -> bool {
    a.key == b.key
        && comments_eq(&a.leading, &b.leading)
        && comments_eq(&a.internal, &b.internal)
        && comments_eq(&a.trailing, &b.trailing)
        && content_eq(&a.node, &b.node)
}

/// Merge equality treats object member order as presentation, including inside atomic arrays.
pub(crate) fn merge_eq(a: &Node, b: &Node) -> bool {
    if !comments_eq(&a.dangling, &b.dangling) {
        return false;
    }
    match (&a.kind, &b.kind) {
        (Kind::Object(a), Kind::Object(b)) => {
            let a: BTreeMap<_, _> = a.iter().map(|e| (e.key.as_ref().unwrap(), e)).collect();
            let b: BTreeMap<_, _> = b.iter().map(|e| (e.key.as_ref().unwrap(), e)).collect();
            a.len() == b.len()
                && a.iter()
                    .all(|(key, e)| b.get(key).is_some_and(|f| merge_entry_eq(e, f)))
        }
        (Kind::Array(a), Kind::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(e, f)| merge_entry_eq(e, f))
        }
        _ => value_eq(a, b),
    }
}
pub(crate) fn merge_entry_eq(a: &Entry, b: &Entry) -> bool {
    a.key == b.key
        && comments_eq(&a.leading, &b.leading)
        && comments_eq(&a.internal, &b.internal)
        && comments_eq(&a.trailing, &b.trailing)
        && merge_eq(&a.node, &b.node)
}

pub(crate) fn span(start: usize, end: usize) -> Span {
    Span { start, end }
}

use crate::model::{Kind, Node};
use crate::{Diagnostic, DiagnosticCode as Code, Document};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
/// Whether record order participates in reconciliation.
pub enum ArrayOrder {
    /// Select pairwise changes and reject inconsistent precedence cycles.
    Ordered,
    /// Preserve ours' surviving order; append theirs-only identities sorted.
    Unordered,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Identity declaration for an exact object-property JSON Pointer.
pub struct ArrayPolicy {
    /// Empty for the root, otherwise `/`-separated with JSON Pointer escaping.
    pub path: String,
    /// Single string-valued field identifying records within this array.
    pub key: String,
    /// Ordering semantics; no automatic inference occurs.
    pub order: ArrayOrder,
}

/// Versioned, explicit identity declarations. Paths traverse object properties only.
#[derive(Clone, Debug, Default)]
pub struct Policy {
    pub(crate) arrays: BTreeMap<Vec<String>, ArrayPolicy>,
    trie: Trie,
}

#[derive(Clone, Debug, Default)]
struct Trie {
    rule: Option<ArrayPolicy>,
    children: BTreeMap<String, Trie>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    version: u32,
    arrays: Vec<ArrayPolicy>,
}

impl Policy {
    /// Validate declarations, rejecting duplicates, nested array paths and budgets.
    pub fn new(arrays: Vec<ArrayPolicy>) -> Result<Self, Diagnostic> {
        let mut result = Self::default();
        let mut bytes = 0usize;
        if arrays.len() > 200_000 {
            return Err(Diagnostic::budget("policy declaration limit exceeded"));
        }
        for array in arrays {
            bytes = bytes
                .saturating_add(array.path.len())
                .saturating_add(array.key.len());
            if bytes > 8 * 1024 * 1024 {
                return Err(Diagnostic::budget("policy text byte limit exceeded"));
            }
            let path = pointer(&array.path)?;
            if path.len() > 128 {
                return Err(Diagnostic::budget("policy path depth limit exceeded"));
            }
            if result.arrays.insert(path, array).is_some() {
                return Err(policy_error("duplicate policy path"));
            }
        }
        let paths: Vec<_> = result.arrays.keys().collect();
        for pair in paths.windows(2) {
            if pair[1].starts_with(pair[0]) {
                return Err(policy_error(
                    "a policy path cannot descend through another declared array",
                ));
            }
        }
        for (path, rule) in &result.arrays {
            let mut trie = &mut result.trie;
            for key in path {
                trie = trie.children.entry(key.clone()).or_default();
            }
            trie.rule = Some(rule.clone());
        }
        Ok(result)
    }
    /// Parse a strict JSON policy file with `version: 1` and an `arrays` list.
    pub fn from_json(source: &str) -> Result<Self, Diagnostic> {
        // Reuse strict duplicate-key validation before serde's object deserialization.
        Document::parse(source)?;
        let file: PolicyFile =
            serde_json::from_str(source).map_err(|e| policy_error(e.to_string()))?;
        if file.version != 1 {
            return Err(policy_error("unsupported policy version"));
        }
        Self::new(file.arrays)
    }
    /// Validate every present target and all record identities in this snapshot.
    /// Missing paths are permitted; traversal through an array is not.
    pub fn validate(&self, document: &Document) -> Result<(), Diagnostic> {
        validate_trie(&document.root, &self.trie)
    }

    pub(crate) fn check_path_budget(
        &self,
        document: &Document,
        remaining: &mut usize,
    ) -> Result<(), Diagnostic> {
        check_paths(&document.root, Some(&self.trie), 0, remaining)
    }
}

// Check before importing snapshots or constructing any expanded paths. A short
// input can otherwise repeat a very long ancestor name across thousands of report
// entries and exhaust memory. Traversal borrows names and carries only a byte count.
fn check_paths(
    node: &Node,
    trie: Option<&Trie>,
    parent_bytes: usize,
    remaining: &mut usize,
) -> Result<(), Diagnostic> {
    let error = || Diagnostic::budget("combined semantic path byte limit exceeded");
    *remaining = remaining.checked_sub(parent_bytes).ok_or_else(error)?;
    match &node.kind {
        Kind::Object(entries) => {
            for entry in entries {
                let key = entry.key.as_ref().unwrap();
                let bytes = parent_bytes
                    .checked_add(32)
                    .and_then(|v| v.checked_add(key.len()))
                    .ok_or_else(error)?;
                check_paths(
                    &entry.node,
                    trie.and_then(|t| t.children.get(key)),
                    bytes,
                    remaining,
                )?;
            }
        }
        Kind::Array(entries) => {
            let rule = trie.and_then(|t| t.rule.as_ref());
            let ids = rule.map(|r| identities(node, r)).transpose()?;
            for (i, entry) in entries.iter().enumerate() {
                let identity_bytes = ids.as_ref().map_or(0, |ids| ids[i].len());
                let bytes = parent_bytes
                    .checked_add(32)
                    .and_then(|v| v.checked_add(identity_bytes))
                    .ok_or_else(error)?;
                check_paths(&entry.node, None, bytes, remaining)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn validate_trie(node: &Node, trie: &Trie) -> Result<(), Diagnostic> {
    if let Some(rule) = &trie.rule {
        identities(node, rule)?;
    }
    if trie.children.is_empty() {
        return Ok(());
    }
    match &node.kind {
        Kind::Object(entries) => {
            let members: BTreeMap<_, _> = entries
                .iter()
                .map(|e| (e.key.as_ref().unwrap(), &e.node))
                .collect();
            for (key, child) in &trie.children {
                if let Some(node) = members.get(key) {
                    validate_trie(node, child)?;
                }
            }
        }
        Kind::Array(_) => {
            return Err(policy_error(
                "policy path traverses an array; positional paths are unsupported",
            ));
        }
        _ => (),
    }
    Ok(())
}

pub(crate) fn identities<'a>(
    node: &'a Node,
    rule: &ArrayPolicy,
) -> Result<Vec<&'a str>, Diagnostic> {
    let Kind::Array(entries) = &node.kind else {
        return Err(policy_error(format!("{} must be an array", rule.path)));
    };
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for entry in entries {
        let id = match &entry.node.kind {
            Kind::Object(fields) => fields
                .iter()
                .find(|e| e.key.as_ref() == Some(&rule.key))
                .and_then(|e| match &e.node.kind {
                    Kind::String(s) => Some(s.as_str()),
                    _ => None,
                }),
            _ => None,
        }
        .ok_or_else(|| {
            Diagnostic::new(
                Code::Identity,
                format!(
                    "{} requires object records with string field {:?}",
                    rule.path, rule.key
                ),
                entry.origin.as_ref().map(|o| o.span),
            )
        })?;
        if !seen.insert(id) {
            return Err(Diagnostic::new(
                Code::Identity,
                format!("duplicate identity {id:?} at {}", rule.path),
                entry.origin.as_ref().map(|o| o.span),
            ));
        }
        ids.push(id);
    }
    Ok(ids)
}

fn pointer(raw: &str) -> Result<Vec<String>, Diagnostic> {
    if raw.is_empty() {
        return Ok(vec![]);
    }
    if !raw.starts_with('/') {
        return Err(policy_error("policy paths must be JSON Pointers"));
    }
    raw[1..]
        .split('/')
        .map(|part| {
            let mut out = String::new();
            let mut chars = part.chars();
            while let Some(c) = chars.next() {
                if c == '~' {
                    out.push(match chars.next() {
                        Some('0') => '~',
                        Some('1') => '/',
                        _ => return Err(policy_error("invalid JSON Pointer escape")),
                    });
                } else {
                    out.push(c);
                }
            }
            Ok(out)
        })
        .collect()
}
fn policy_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::Policy, message, None)
}

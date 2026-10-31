use crate::model::{Comment, Entry, Kind, Node, comments_eq, content_eq, merge_eq, value_eq};
use crate::plan::Provenance;
use crate::policy::identities;
use crate::source::import_comments;
use crate::{
    ArrayOrder, Conflict, ConflictKind, Diagnostic, Document, Limits, MergePlan, Origin,
    PathSegment, Policy, Side,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Build an immutable three-way merge plan with default resource limits.
/// Input/policy errors are diagnostics; semantic conflicts are retained in the plan.
pub fn merge(
    base: &Document,
    ours: &Document,
    theirs: &Document,
    policy: &Policy,
) -> Result<MergePlan, Diagnostic> {
    merge_with_limits(base, ours, theirs, policy, Limits::default())
}

/// Merge with explicit input/output and ordered-pair budgets.
/// Ours supplies presentation. All snapshots are validated before reconciliation.
pub fn merge_with_limits(
    base: &Document,
    ours: &Document,
    theirs: &Document,
    policy: &Policy,
    limits: Limits,
) -> Result<MergePlan, Diagnostic> {
    for (side, doc) in [Side::Base, Side::Ours, Side::Theirs]
        .into_iter()
        .zip([base, ours, theirs])
    {
        doc.check_limits(limits)
            .and_then(|()| policy.validate(doc))
            .map_err(|mut error| {
                error.input = Some(side);
                error
            })?;
    }
    let mut remaining_paths = limits.max_path_bytes;
    for (side, doc) in [Side::Base, Side::Ours, Side::Theirs]
        .into_iter()
        .zip([base, ours, theirs])
    {
        policy
            .check_path_budget(doc, &mut remaining_paths)
            .map_err(|mut error| {
                error.input = Some(side);
                error
            })?;
    }
    let mut merger = Merger {
        policy,
        conflicts: vec![],
        remaining_pairs: limits.max_order_pairs,
    };
    let b = base.imported(Side::Base);
    let o = ours.imported(Side::Ours);
    let t = theirs.imported(Side::Theirs);
    let root = merger.node(Some(&b), &o, &t, &[], Some(&[]))?;
    let headers = [
        import_comments(&base.header, Side::Base),
        import_comments(&ours.header, Side::Ours),
        import_comments(&theirs.header, Side::Theirs),
    ];
    let footers = [
        import_comments(&base.footer, Side::Base),
        import_comments(&ours.footer, Side::Ours),
        import_comments(&theirs.footer, Side::Theirs),
    ];
    let header = merger.comments(&headers[0], &headers[1], &headers[2], &[]);
    let footer = merger.comments(&footers[0], &footers[1], &footers[2], &[]);
    let mut provenance = Vec::new();
    if merger.conflicts.is_empty()
        && let Some(node) = &root
    {
        collect_provenance(node, &[], &mut provenance);
        let origins = header
            .iter()
            .chain(&footer)
            .map(|c| c.origin.clone())
            .collect::<Vec<_>>();
        if !origins.is_empty() {
            provenance.push(Provenance {
                path: vec![],
                origins,
            });
        }
    }
    Ok(MergePlan {
        inputs: [base.clone(), ours.clone(), theirs.clone()],
        root,
        header,
        footer,
        conflicts: merger.conflicts,
        provenance,
        policy: policy.clone(),
        limits,
    })
}

struct Merger<'a> {
    policy: &'a Policy,
    conflicts: Vec<Conflict>,
    remaining_pairs: usize,
}

impl Merger<'_> {
    fn conflict(
        &mut self,
        kind: ConflictKind,
        path: &[PathSegment],
        alternatives: [Option<Origin>; 3],
        witness: Vec<String>,
    ) {
        let [base, ours, theirs] = alternatives;
        self.conflicts.push(Conflict {
            kind,
            path: path.into(),
            base,
            ours,
            theirs,
            order_witness: witness,
        });
    }

    fn comments(
        &mut self,
        base: &[Comment],
        ours: &[Comment],
        theirs: &[Comment],
        path: &[PathSegment],
    ) -> Vec<Comment> {
        if comments_eq(ours, theirs) || comments_eq(base, theirs) {
            return ours.into();
        }
        if comments_eq(base, ours) {
            return theirs.into();
        }
        let origin = |comments: &[Comment]| {
            let first = comments.first()?;
            Some(Origin {
                side: first.origin.side,
                span: crate::Span {
                    start: first.origin.span.start,
                    end: comments.last().unwrap().origin.span.end,
                },
            })
        };
        self.conflict(
            ConflictKind::Comment,
            path,
            [origin(base), origin(ours), origin(theirs)],
            vec![],
        );
        ours.into()
    }

    fn node(
        &mut self,
        base: Option<&Arc<Node>>,
        ours: &Arc<Node>,
        theirs: &Arc<Node>,
        path: &[PathSegment],
        policy_path: Option<&[String]>,
    ) -> Result<Option<Arc<Node>>, Diagnostic> {
        let alternatives = [
            base.and_then(|n| n.raw.clone()),
            ours.raw.clone(),
            theirs.raw.clone(),
        ];
        let rule = policy_path.and_then(|p| self.policy.arrays.get(p)).cloned();
        let kind = match (&ours.kind, &theirs.kind, base.map(|b| &b.kind)) {
            (Kind::Object(o), Kind::Object(t), None | Some(Kind::Object(_))) => {
                let b = base
                    .and_then(|b| {
                        if let Kind::Object(e) = &b.kind {
                            Some(e.as_slice())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(&[]);
                Kind::Object(self.members(b, o, t, path, policy_path, None)?)
            }
            (Kind::Array(o), Kind::Array(t), None | Some(Kind::Array(_))) if rule.is_some() => {
                let rule = rule.as_ref().unwrap();
                let b = base
                    .and_then(|b| {
                        if let Kind::Array(e) = &b.kind {
                            Some(e.as_slice())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(&[]);
                let ids = |n: &Node| {
                    identities(n, rule)
                        .map(|ids| ids.into_iter().map(str::to_owned).collect::<Vec<_>>())
                };
                let bi = base.map(|b| ids(b)).transpose()?.unwrap_or_default();
                let oi = ids(ours)?;
                let ti = ids(theirs)?;
                let mut entries = self.members(b, o, t, path, None, Some((&bi, &oi, &ti)))?;
                let present: BTreeSet<_> = entries
                    .iter()
                    .map(|e| record_id(e, &rule.key).to_owned())
                    .collect();
                let order = if rule.order == ArrayOrder::Ordered {
                    match crate::order::reconcile_order(
                        &bi,
                        &oi,
                        &ti,
                        &present,
                        &mut self.remaining_pairs,
                    )? {
                        Ok(order) => order,
                        Err(witness) => {
                            self.conflict(ConflictKind::Order, path, alternatives.clone(), witness);
                            return Ok(None);
                        }
                    }
                } else {
                    let ours_ids: BTreeSet<_> = oi.iter().collect();
                    let mut order: Vec<_> = oi
                        .iter()
                        .filter(|id| present.contains(*id))
                        .cloned()
                        .collect();
                    order.extend(present.iter().filter(|id| !ours_ids.contains(id)).cloned());
                    order
                };
                let rank: BTreeMap<_, _> = order
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (id.as_str(), i))
                    .collect();
                entries.sort_by_key(|e| rank[record_id(e, &rule.key)]);
                Kind::Array(entries)
            }
            _ => {
                if self.equivalent(ours, theirs, policy_path)
                    || base.is_some_and(|b| self.equivalent(b, theirs, policy_path))
                {
                    return Ok(Some(ours.clone()));
                }
                if base.is_some_and(|b| self.equivalent(b, ours, policy_path)) {
                    return Ok(Some(self.adopt_theirs(theirs, policy_path)?));
                }
                self.conflict(
                    if matches!(ours.kind, Kind::Array(_)) && matches!(theirs.kind, Kind::Array(_))
                    {
                        ConflictKind::AtomicArray
                    } else {
                        ConflictKind::Value
                    },
                    path,
                    alternatives,
                    vec![],
                );
                return Ok(None);
            }
        };
        let dangling = self.comments(
            base.map_or(&[], |b| b.dangling.as_slice()),
            &ours.dangling,
            &theirs.dangling,
            path,
        );
        let merged = Node {
            kind,
            dangling,
            raw: None,
            layout: Some(ours.clone()),
        };
        if content_eq(&merged, ours) {
            return Ok(Some(ours.clone()));
        }
        Ok(Some(Arc::new(merged)))
    }

    fn members(
        &mut self,
        base: &[Entry],
        ours: &[Entry],
        theirs: &[Entry],
        path: &[PathSegment],
        policy_path: Option<&[String]>,
        ids: Option<(&[String], &[String], &[String])>,
    ) -> Result<Vec<Entry>, Diagnostic> {
        let maps = |entries: &[Entry], ids: Option<&[String]>| -> BTreeMap<String, Entry> {
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    (
                        ids.map_or_else(|| e.key.clone().unwrap(), |ids| ids[i].clone()),
                        e.clone(),
                    )
                })
                .collect()
        };
        let b = maps(base, ids.map(|i| i.0));
        let o = maps(ours, ids.map(|i| i.1));
        let t = maps(theirs, ids.map(|i| i.2));
        let keys: BTreeSet<_> = b.keys().chain(o.keys()).chain(t.keys()).cloned().collect();
        let mut merged = BTreeMap::new();
        for key in keys {
            let mut location = path.to_vec();
            location.push(if ids.is_some() {
                PathSegment::Record(key.clone())
            } else {
                PathSegment::Key(key.clone())
            });
            let child_policy = if ids.is_some() {
                None
            } else {
                policy_path.map(|p| {
                    let mut p = p.to_vec();
                    p.push(key.clone());
                    p
                })
            };
            let (bv, ov, tv) = (b.get(&key), o.get(&key), t.get(&key));
            let origins = [
                bv.and_then(|e| e.origin.clone()),
                ov.and_then(|e| e.origin.clone()),
                tv.and_then(|e| e.origin.clone()),
            ];
            let entry = match (bv, ov, tv) {
                (_, Some(o), Some(t)) => {
                    let Some(node) = self.node(
                        bv.map(|b| &b.node),
                        &o.node,
                        &t.node,
                        &location,
                        child_policy.as_deref(),
                    )?
                    else {
                        continue;
                    };
                    let mut e = o.clone();
                    e.node = node;
                    e.leading = self.comments(
                        bv.map_or(&[], |e| &e.leading),
                        &o.leading,
                        &t.leading,
                        &location,
                    );
                    e.internal = self.comments(
                        bv.map_or(&[], |e| &e.internal),
                        &o.internal,
                        &t.internal,
                        &location,
                    );
                    e.trailing = self.comments(
                        bv.map_or(&[], |e| &e.trailing),
                        &o.trailing,
                        &t.trailing,
                        &location,
                    );
                    if !comments_eq(&e.leading, &o.leading) {
                        e.leading_layout = t.leading_layout.clone();
                    }
                    if !comments_eq(&e.trailing, &o.trailing) {
                        e.trailing_layout = t.trailing_layout.clone();
                    }
                    // Prefix can be copied only when its embedded comments match the selected ones.
                    if !comments_eq(&e.internal, &o.internal) {
                        e.prefix = if comments_eq(&e.internal, &t.internal) {
                            t.prefix.clone()
                        } else {
                            None
                        };
                    }
                    Some(e)
                }
                (None, Some(e), None) => Some(e.clone()),
                (None, None, Some(e)) => {
                    let mut e = e.clone();
                    e.node = self.adopt_theirs(&e.node, child_policy.as_deref())?;
                    Some(e)
                }
                (Some(b), Some(e), None) | (Some(b), None, Some(e)) => {
                    if !self.entry_equivalent(b, e, child_policy.as_deref()) {
                        self.conflict(ConflictKind::DeleteEdit, &location, origins, vec![]);
                    }
                    None
                }
                _ => None,
            };
            if let Some(entry) = entry {
                merged.insert(key, entry);
            }
        }
        // Object order is presentation: ours first, then theirs-only in their source order.
        let mut result = Vec::new();
        let sequence = |entries: &[Entry], ids: Option<&[String]>| {
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| ids.map_or_else(|| e.key.clone().unwrap(), |ids| ids[i].clone()))
                .collect::<Vec<_>>()
        };
        for key in sequence(ours, ids.map(|i| i.1))
            .into_iter()
            .chain(sequence(theirs, ids.map(|i| i.2)))
        {
            if let Some(e) = merged.remove(&key) {
                result.push(e);
            }
        }
        Ok(result)
    }

    fn adopt_theirs(
        &self,
        node: &Arc<Node>,
        path: Option<&[String]>,
    ) -> Result<Arc<Node>, Diagnostic> {
        let mut result = node.as_ref().clone();
        match &mut result.kind {
            Kind::Object(entries) => {
                for entry in entries {
                    let child = path.map(|p| {
                        let mut p = p.to_vec();
                        p.push(entry.key.clone().unwrap());
                        p
                    });
                    entry.node = self.adopt_theirs(&entry.node, child.as_deref())?;
                }
            }
            Kind::Array(entries) => {
                if let Some(rule) = path.and_then(|p| self.policy.arrays.get(p))
                    && rule.order == ArrayOrder::Unordered
                {
                    entries.sort_by(|a, b| record_id(a, &rule.key).cmp(record_id(b, &rule.key)));
                }
            }
            _ => return Ok(node.clone()),
        }
        if content_eq(&result, node) {
            return Ok(node.clone());
        }
        result.raw = None;
        result.layout = Some(node.clone());
        Ok(Arc::new(result))
    }

    fn entry_equivalent(&self, a: &Entry, b: &Entry, path: Option<&[String]>) -> bool {
        a.key == b.key
            && comments_eq(&a.leading, &b.leading)
            && comments_eq(&a.internal, &b.internal)
            && comments_eq(&a.trailing, &b.trailing)
            && self.equivalent(&a.node, &b.node, path)
    }

    fn equivalent(&self, a: &Node, b: &Node, path: Option<&[String]>) -> bool {
        if !comments_eq(&a.dangling, &b.dangling) {
            return false;
        }
        match (&a.kind, &b.kind) {
            (Kind::Object(a), Kind::Object(b)) => {
                let a: BTreeMap<_, _> = a.iter().map(|e| (e.key.as_ref().unwrap(), e)).collect();
                let b: BTreeMap<_, _> = b.iter().map(|e| (e.key.as_ref().unwrap(), e)).collect();
                a.len() == b.len()
                    && a.iter().all(|(key, e)| {
                        let child = path.map(|p| {
                            let mut p = p.to_vec();
                            p.push((*key).clone());
                            p
                        });
                        b.get(key)
                            .is_some_and(|f| self.entry_equivalent(e, f, child.as_deref()))
                    })
            }
            (Kind::Array(ae), Kind::Array(be)) => {
                let Some(rule) = path.and_then(|p| self.policy.arrays.get(p)) else {
                    return merge_eq(a, b);
                };
                // All source nodes passed here have already passed policy validation.
                let ai = identities(a, rule).expect("validated record identities");
                let bi = identities(b, rule).expect("validated record identities");
                if rule.order == ArrayOrder::Ordered && ai != bi {
                    return false;
                }
                let a: BTreeMap<_, _> = ai.into_iter().zip(ae).collect();
                let b: BTreeMap<_, _> = bi.into_iter().zip(be).collect();
                a.len() == b.len()
                    && a.iter()
                        .all(|(id, e)| b.get(id).is_some_and(|f| self.entry_equivalent(e, f, None)))
            }
            _ => value_eq(a, b),
        }
    }
}

fn record_id<'a>(entry: &'a Entry, key: &str) -> &'a str {
    let Kind::Object(fields) = &entry.node.kind else {
        unreachable!()
    };
    let field = fields
        .iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap();
    let Kind::String(id) = &field.node.kind else {
        unreachable!()
    };
    id
}

fn collect_provenance(node: &Node, path: &[PathSegment], output: &mut Vec<Provenance>) {
    let mut origins: Vec<_> = node
        .raw
        .iter()
        .chain(node.layout.iter().filter_map(|n| n.raw.as_ref()))
        .cloned()
        .chain(node.dangling.iter().map(|c| c.origin.clone()))
        .collect();
    if let Kind::Object(entries) | Kind::Array(entries) = &node.kind {
        for (i, e) in entries.iter().enumerate() {
            origins.extend(
                e.leading
                    .iter()
                    .chain(&e.internal)
                    .chain(&e.trailing)
                    .map(|c| c.origin.clone()),
            );
            if let Some(prefix) = &e.prefix {
                origins.push(prefix.clone());
            }
            let mut child = path.to_vec();
            child.push(
                e.key
                    .clone()
                    .map_or(PathSegment::Index(i), PathSegment::Key),
            );
            collect_provenance(&e.node, &child, output);
        }
    }
    if !origins.is_empty() {
        output.push(Provenance {
            path: path.into(),
            origins,
        });
    }
}

use jsonc_merge::{
    ArrayOrder, ArrayPolicy, ConflictKind, DiagnosticCode, Document, Limits, Policy, Side, merge,
    merge_with_limits,
};

#[test]
fn comment_conflicts_point_to_the_actual_comment_alternatives() {
    let texts = ["// base\n{}", "// ours\n{}", "// theirs\n{}"];
    let docs = texts.map(|t| Document::parse(t).unwrap());
    let plan = merge(&docs[0], &docs[1], &docs[2], &Policy::default()).unwrap();
    let conflict = &plan.conflicts()[0];
    assert_eq!(conflict.kind, ConflictKind::Comment);
    for (alternative, text) in [&conflict.base, &conflict.ours, &conflict.theirs]
        .into_iter()
        .zip(texts)
    {
        let origin = alternative.as_ref().unwrap();
        assert!(text[origin.span.start..origin.span.end].starts_with("// "));
    }
}

#[test]
fn root_policies_and_numeric_object_property_paths() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "".into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    }])
    .unwrap();
    let b = Document::parse(r#"[{"id":"a"},{"id":"b"}]"#).unwrap();
    let o = Document::parse(r#"[{"id":"b"},{"id":"a"}]"#).unwrap();
    let t = Document::parse(r#"[{"id":"a","x":1},{"id":"b"}]"#).unwrap();
    let plan = merge(&b, &o, &t, &policy).unwrap();
    let out = plan.render().unwrap();
    assert!(
        Document::parse(out)
            .unwrap()
            .semantically_eq(&Document::parse(r#"[{"id":"b"},{"id":"a","x":1}]"#).unwrap())
    );
    for p in plan.report().provenance {
        for origin in p.origins {
            let source = match origin.side {
                Side::Base => b.source(),
                Side::Ours => o.source(),
                Side::Theirs => t.source(),
            };
            assert!(source.get(origin.span.start..origin.span.end).is_some());
        }
    }
    let policy = Policy::new(vec![ArrayPolicy {
        path: "/a/0".into(),
        key: "id".into(),
        order: ArrayOrder::Unordered,
    }])
    .unwrap();
    policy
        .validate(&Document::parse(r#"{"a":{"0":[{"id":"é"},{"id":"e\u0301"}]}}"#).unwrap())
        .unwrap();
}

#[test]
fn merge_checks_snapshot_limits_and_identifies_input_errors() {
    let doc = Document::parse("[1,2]").unwrap();
    let limits = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    let error = merge_with_limits(&doc, &doc, &doc, &Policy::default(), limits).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::Budget);
    assert_eq!(error.input, Some(Side::Base));
    let policy = Policy::new(vec![ArrayPolicy {
        path: "".into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    }])
    .unwrap();
    let base = Document::parse("[]").unwrap();
    let invalid = Document::parse(r#"[{}]"#).unwrap();
    let error = merge(&base, &invalid, &base, &policy).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::Identity);
    assert_eq!(error.input, Some(Side::Ours));
}

#[test]
fn output_budget_failure_is_explicit_and_does_not_modify_inputs() {
    let b = Document::parse("{}").unwrap();
    let o = Document::parse(r#"{"a":1}"#).unwrap();
    let t = Document::parse(r#"{"b":2}"#).unwrap();
    let limits = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    let plan = merge_with_limits(&b, &o, &t, &Policy::default(), limits).unwrap();
    assert!(plan.is_clean());
    assert_eq!(plan.render().unwrap_err().code, DiagnosticCode::Budget);
    assert_eq!(o.source(), r#"{"a":1}"#);
}

use jsonc_merge::{
    ArrayOrder, ArrayPolicy, DiagnosticCode, Document, Limits, Policy, Side, merge_with_limits,
};

#[test]
fn expanded_path_budget_has_an_exact_portable_boundary() {
    // One one-byte key: 32 component bytes + 1 key byte, once per input.
    let doc = Document::parse(r#"{"x":0}"#).unwrap();
    let limits = Limits {
        max_path_bytes: 99,
        ..Limits::default()
    };
    assert!(
        merge_with_limits(&doc, &doc, &doc, &Policy::default(), limits)
            .unwrap()
            .is_clean()
    );
    let limits = Limits {
        max_path_bytes: 98,
        ..Limits::default()
    };
    let error = merge_with_limits(&doc, &doc, &doc, &Policy::default(), limits).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::Budget);
    assert_eq!(error.input, Some(Side::Theirs));
}

#[test]
fn repeated_long_ancestor_names_are_bounded_before_report_expansion() {
    let key = "雪".repeat(1000);
    let source = serde_json::json!({key: (0..100).collect::<Vec<_>>()}).to_string();
    let doc = Document::parse(source).unwrap();
    let limits = Limits {
        max_path_bytes: 10_000,
        ..Limits::default()
    };
    assert_eq!(
        merge_with_limits(&doc, &doc, &doc, &Policy::default(), limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    assert_eq!(doc.source().matches('雪').count(), 1000);
}

#[test]
fn record_identity_path_components_are_included() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "".into(),
        key: "id".into(),
        order: ArrayOrder::Unordered,
    }])
    .unwrap();
    let doc = Document::parse(r#"[{"id":"abc"}]"#).unwrap();
    // Record path: 35; its id property path: 35+34. Three inputs = 312.
    let limits = Limits {
        max_path_bytes: 312,
        ..Limits::default()
    };
    assert!(
        merge_with_limits(&doc, &doc, &doc, &policy, limits)
            .unwrap()
            .is_clean()
    );
    let limits = Limits {
        max_path_bytes: 311,
        ..Limits::default()
    };
    assert_eq!(
        merge_with_limits(&doc, &doc, &doc, &policy, limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
}

#[test]
fn comment_budget_is_independent_of_nodes_and_rechecked_by_merge() {
    let limits = Limits {
        max_comments: 2,
        ..Limits::default()
    };
    Document::parse_with_limits("/* header */null// footer", limits).unwrap();
    assert_eq!(
        Document::parse_with_limits("/* 1 *//* 2 */null/* 3 */", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let doc = Document::parse("/* 1 *//* 2 */null/* 3 */").unwrap();
    assert_eq!(
        merge_with_limits(&doc, &doc, &doc, &Policy::default(), limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let limits = Limits {
        max_comments: 0,
        ..Limits::default()
    };
    Document::parse_with_limits(r#"{"text":"// not a comment"}"#, limits).unwrap();
}

#[test]
fn node_preflight_allows_object_keys_and_rejects_oversized_token_streams() {
    let limits = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    Document::parse_with_limits(r#"{"x":0}"#, limits).unwrap();
    Document::parse_with_limits(r#"{"x":{}}"#, limits).unwrap();
    assert_eq!(
        Document::parse_with_limits("[0,0,0,0,0,0]", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let limits = Limits {
        max_nodes: 0,
        ..Limits::default()
    };
    assert_eq!(
        Document::parse_with_limits("null", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    assert!(Document::parse_with_limits("null\u{a0}", Limits::default()).is_err());
}

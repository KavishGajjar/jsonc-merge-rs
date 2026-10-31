use jsonc_merge::{
    ArrayOrder, ArrayPolicy, ConflictKind, DiagnosticCode, Document, Limits, Policy, merge,
    merge_with_limits,
};
use serde_json::{Value, json};

fn policy(order: ArrayOrder) -> Policy {
    Policy::new(vec![ArrayPolicy {
        path: "/items".into(),
        key: "id".into(),
        order,
    }])
    .unwrap()
}
fn run(b: &str, o: &str, t: &str, order: ArrayOrder) -> jsonc_merge::MergePlan {
    merge(
        &Document::parse(b).unwrap(),
        &Document::parse(o).unwrap(),
        &Document::parse(t).unwrap(),
        &policy(order),
    )
    .unwrap()
}
fn ids(out: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(out).unwrap();
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect()
}
fn document(names: &[&str]) -> String {
    json!({"items":names.iter().map(|s| json!({"id":s})).collect::<Vec<_>>()}).to_string()
}

#[test]
fn reorder_and_field_edit_follow_identity_and_comments() {
    let b = "{\n \"items\": [\n // alpha\n {\"id\":\"a\",\"x\":1}, // a tail\n // beta\n {\"id\":\"b\",\"x\":1}\n ]\n}";
    let o = "{\n \"items\": [\n // beta\n {\"id\":\"b\",\"x\":1},\n // alpha\n {\"id\":\"a\",\"x\":1} // a tail\n ]\n}";
    let t = b.replace("\"a\",\"x\":1", "\"a\",\"x\":2");
    let p = run(b, o, &t, ArrayOrder::Ordered);
    assert!(p.is_clean(), "{:?}", p.conflicts());
    let out = p.render().unwrap();
    assert!(out.find("// beta").unwrap() < out.find("// alpha").unwrap());
    assert!(out.contains("\"a\",\"x\":2"));
    let a_line = out.lines().find(|s| s.contains("// a tail")).unwrap();
    assert!(a_line.contains("\"a\""));
}

#[test]
fn concurrent_additions_and_membership_first() {
    let p = run(
        r#"{"items":[]}"#,
        r#"{"items":[{"id":"x","a":1}]}"#,
        r#"{"items":[{"id":"x","b":2}]}"#,
        ArrayOrder::Ordered,
    );
    let out: Value = serde_json::from_str(&p.render().unwrap()).unwrap();
    assert_eq!(out, json!({"items":[{"id":"x","a":1,"b":2}]}));
    let p = run(
        &document(&["a", "b", "c"]),
        &document(&["a", "c"]),
        &document(&["c", "b", "a"]),
        ArrayOrder::Ordered,
    );
    assert_eq!(ids(&p.render().unwrap()), ["c", "a"]);
    let p = run(
        r#"{"items":[{"id":"a","x":1}]}"#,
        r#"{"items":[]}"#,
        r#"{"items":[{"id":"a","x":2}]}"#,
        ArrayOrder::Ordered,
    );
    assert_eq!(p.conflicts()[0].kind, ConflictKind::DeleteEdit);
}

#[test]
fn new_relations_and_cycles_are_explicit() {
    let p = run(
        &document(&["a", "b", "c"]),
        &document(&["b", "a", "c"]),
        &document(&["a", "c", "b"]),
        ArrayOrder::Ordered,
    );
    assert_eq!(p.conflicts()[0].kind, ConflictKind::Order);
    let witness = &p.conflicts()[0].order_witness;
    assert!(witness.len() >= 3);
    assert_eq!(witness.first(), witness.last());
    let p = run(
        &document(&[]),
        &document(&["a", "b"]),
        &document(&["b", "a"]),
        ArrayOrder::Ordered,
    );
    assert_eq!(p.conflicts()[0].kind, ConflictKind::Order);
    let p = run(
        &document(&[]),
        &document(&["雪"]),
        &document(&["a"]),
        ArrayOrder::Ordered,
    );
    assert_eq!(ids(&p.render().unwrap()), ["a", "雪"]);
}

#[test]
fn unordered_preserves_ours_then_sorted_theirs_only() {
    let p = run(
        &document(&["a", "b"]),
        &document(&["b", "a", "z"]),
        &document(&["x", "a", "b", "c"]),
        ArrayOrder::Unordered,
    );
    assert_eq!(ids(&p.render().unwrap()), ["b", "a", "z", "c", "x"]);
    let p = run("{}", "{}", &document(&["z", "a"]), ArrayOrder::Unordered);
    assert_eq!(ids(&p.render().unwrap()), ["a", "z"]);
    let p = run(
        "null",
        "null",
        &document(&["z", "a"]),
        ArrayOrder::Unordered,
    );
    assert_eq!(ids(&p.render().unwrap()), ["a", "z"]);
}

#[test]
fn unordered_reorders_do_not_turn_parent_deletion_or_type_change_into_conflicts() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "/outer/items".into(),
        key: "id".into(),
        order: ArrayOrder::Unordered,
    }])
    .unwrap();
    let b = Document::parse(r#"{"outer":{"items":[{"id":"a"},{"id":"b"}]}}"#).unwrap();
    let t = Document::parse(r#"{"outer":{"items":[{"id":"b"},{"id":"a"}]}}"#).unwrap();
    for ours in ["{}", "null"] {
        let ours = Document::parse(ours).unwrap();
        let plan = merge(&b, &ours, &t, &policy).unwrap();
        assert!(plan.is_clean(), "{:?}", plan.conflicts());
        assert!(
            Document::parse(plan.render().unwrap())
                .unwrap()
                .semantically_eq(&ours)
        );
    }
}

#[test]
fn order_budget_is_not_a_conflict_or_silent_fallback() {
    let doc = Document::parse(document(&["a", "b", "c"])).unwrap();
    let limits = Limits {
        max_order_pairs: 2,
        ..Limits::default()
    };
    assert_eq!(
        merge_with_limits(&doc, &doc, &doc, &policy(ArrayOrder::Ordered), limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let limits = Limits {
        max_order_pairs: 3,
        ..Limits::default()
    };
    assert!(
        merge_with_limits(&doc, &doc, &doc, &policy(ArrayOrder::Ordered), limits)
            .unwrap()
            .is_clean()
    );
}

#[test]
fn large_unordered_arrays_keep_all_records_and_concurrent_edits() {
    let records: Vec<_> = (0..10_000)
        .map(|i| json!({"id":format!("r{i:05}"),"value":0}))
        .collect();
    let base = json!({"items":records});
    let mut ours = base.clone();
    ours["items"].as_array_mut().unwrap().reverse();
    let mut theirs = base.clone();
    theirs["items"][0]["value"] = json!(1);
    theirs["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"new","value":2}));
    let plan = run(
        &base.to_string(),
        &ours.to_string(),
        &theirs.to_string(),
        ArrayOrder::Unordered,
    );
    let result: Value = serde_json::from_str(&plan.render().unwrap()).unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 10_001);
    assert_eq!(items[0]["id"], "r09999");
    assert_eq!(items[9_999]["id"], "r00000");
    assert_eq!(items[9_999]["value"], 1);
    assert_eq!(items[10_000], json!({"id":"new","value":2}));
}

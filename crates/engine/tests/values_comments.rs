use jsonc_merge::{ConflictKind, DiagnosticCode, Document, Policy, merge};

fn plan(b: &str, o: &str, t: &str) -> jsonc_merge::MergePlan {
    merge(
        &Document::parse(b).unwrap(),
        &Document::parse(o).unwrap(),
        &Document::parse(t).unwrap(),
        &Policy::default(),
    )
    .unwrap()
}
fn clean(b: &str, o: &str, t: &str, want: &str) -> String {
    let p = plan(b, o, t);
    assert!(p.is_clean(), "{:?}", p.conflicts());
    let output = p.render().unwrap();
    assert!(
        Document::parse(&output)
            .unwrap()
            .semantically_eq(&Document::parse(want).unwrap()),
        "{output}"
    );
    output
}

#[test]
fn recursive_values_and_absent_object_additions() {
    clean(
        r#"{"a":1,"b":1}"#,
        r#"{"a":2,"b":1}"#,
        r#"{"a":1,"b":2}"#,
        r#"{"a":2,"b":2}"#,
    );
    clean(
        "{}",
        r#"{"new":{"a":1}}"#,
        r#"{"new":{"b":2}}"#,
        r#"{"new":{"a":1,"b":2}}"#,
    );
    clean(
        r#"{"a":1,"b":2}"#,
        r#"{"b":2}"#,
        r#"{"a":1,"b":3}"#,
        r#"{"b":3}"#,
    );
    clean("1", "1.0", "1e0", "1");
    clean("null", "false", "null", "false");
    clean("[]", "[1]", "[]", "[1]");
    for (b, o, t, kind) in [
        ("1", "2", "3", ConflictKind::Value),
        ("[1]", "[2]", "[3]", ConflictKind::AtomicArray),
        (r#"{"a":1}"#, "{}", r#"{"a":2}"#, ConflictKind::DeleteEdit),
    ] {
        let p = plan(b, o, t);
        assert_eq!(p.conflicts()[0].kind, kind);
        assert_eq!(p.render().unwrap_err().code, DiagnosticCode::Conflict);
    }
}

#[test]
fn preservation_and_surgical_changes() {
    let original = "// header\r\n{\r\n\t\"escaped\\u006bey\" : 1e0 , // keep\r\n\t\"b\" : [ 2, 3, ],\r\n}\r\n// footer\r\n";
    assert_eq!(clean(original, original, original, original), original);
    let theirs = original.replace("1e0", "2.00");
    let result = clean(original, original, &theirs, &theirs);
    assert_eq!(
        result, theirs,
        "only the changed numeric token should be replaced"
    );
    let p = plan(original, original, &theirs);
    let edits = p.source_edits().unwrap();
    assert_eq!(edits.len(), 1);
    assert!(original.is_char_boundary(edits[0].span.start));
    assert_eq!(p.apply_to(original).unwrap(), result);
    assert_eq!(
        p.apply_to(&(original.to_owned() + " ")).unwrap_err().code,
        DiagnosticCode::StalePlan
    );
    let snapshot = Document::parse(original).unwrap();
    let shared = snapshot.clone();
    merge(
        &snapshot,
        &snapshot,
        &Document::parse(&theirs).unwrap(),
        &Policy::default(),
    )
    .unwrap()
    .render()
    .unwrap();
    assert_eq!(shared.source(), original);
}

#[test]
fn insertion_does_not_steal_inline_comment() {
    let b = "{\n  \"a\": 1 // belongs to a\n}\n";
    let t = "{\n  \"a\": 1, // belongs to a\n  \"b\": 2\n}\n";
    let output = clean(b, b, t, t);
    let line = output.lines().find(|l| l.contains("belongs to a")).unwrap();
    assert!(line.contains("\"a\""));
    assert!(!line.contains("\"b\""));
}

#[test]
fn owned_comment_changes_merge_or_conflict() {
    let b =
        "// header\n{\n // leading\n \"a\" /* inside */: 1, // tail\n // closing\n}\n// footer\n";
    let o = b.replace("leading", "ours leading");
    let t = b.replace(": 1", ": 2");
    let out = clean(b, &o, &t, &t);
    for text in [
        "ours leading",
        "/* inside */",
        "// tail",
        "// closing",
        "// header",
        "// footer",
    ] {
        assert!(out.contains(text), "lost {text}: {out}");
    }
    let t = b.replace("leading", "theirs leading");
    assert!(
        plan(b, &o, &t)
            .conflicts()
            .iter()
            .any(|c| c.kind == ConflictKind::Comment)
    );
    assert_eq!(
        plan(b, "{}", &o).conflicts()[0].kind,
        ConflictKind::DeleteEdit
    );
    let header_o = b.replace("// header", "// OURS");
    let header_t = b.replace("// header", "// THEIRS");
    assert!(
        plan(b, &header_o, &header_t)
            .conflicts()
            .iter()
            .any(|c| c.kind == ConflictKind::Comment)
    );
}

#[test]
fn awkward_separator_and_multiline_comments_stay_valid() {
    for b in [
        "{\"a\":1 /*a,b*/\n,\n/*next*/\"b\":2\n}",
        "{\n\"a\":1 /* multi\n line */,\n\"b\":2\n}",
        "[\n // only\n 1 // tail\n ,\n // close\n]",
    ] {
        assert_eq!(clean(b, b, b, b), b);
    }
    let b = "{\"a\":1 /*a,b*/\n,\n/*next*/\"b\":2\n}";
    let t = "{\"a\":1 /*a,b*/\n,\n/*next*/\"b\":2,\"c\":3\n}";
    let out = clean(b, b, t, t);
    assert!(out.contains("/*a,b*/"));
    assert!(out.contains("/*next*/"));
}

#[test]
fn object_order_is_not_a_value_change_inside_an_atomic_array() {
    clean(
        r#"[{"a":1,"b":2}]"#,
        r#"[{"b":2,"a":1}]"#,
        r#"[{"a":3,"b":2}]"#,
        r#"[{"a":3,"b":2}]"#,
    );
}

#[test]
fn utf8_source_edits_and_deterministic_reports() {
    let p = plan(r#"{"é":"雪"}"#, r#"{"é":"雪"}"#, r#"{"é":"雲"}"#);
    let edits = p.source_edits().unwrap();
    assert_eq!(edits[0].replacement, "雲");
    assert_eq!(p.report().schema_version, 1);
    assert!(!p.report().provenance.is_empty());
    assert_eq!(
        serde_json::to_string(&p.report()).unwrap(),
        serde_json::to_string(&p.clone().report()).unwrap()
    );
}

use jsonc_merge::{ArrayOrder, ArrayPolicy, DiagnosticCode, Document, Limits, Policy, merge};

#[test]
fn strict_jsonc_profile_and_unicode() {
    for text in [
        "null",
        "true",
        "-0.5e+20",
        "{\"a\":1,}",
        "[/*c*/1,2,]",
        "\"𝄞\"",
        r#""\ud834\udd1e""#,
    ] {
        Document::parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
    }
    for text in [
        "",
        "// only",
        "{a:1}",
        "{'a':1}",
        "[1 2]",
        "[1,,2]",
        "01",
        "+1",
        "0x10",
        ".5",
        "5.",
        "NaN",
        "Infinity",
        "\"x\nx\"",
        r#""\ud800""#,
        r#""\x41""#,
        "/*never",
        "true false",
        "[",
        "{",
        "\"",
        "\"\\",
        "1e+",
        "[}",
    ] {
        let error = Document::parse(text).unwrap_err();
        if let Some(span) = error.span {
            assert!(
                text.get(span.start..span.end).is_some(),
                "invalid error span for {text:?}: {span:?}"
            );
        }
    }
    let doc = Document::parse("{\n  \"é\": \"𝄞\"\n}").unwrap();
    assert_eq!(doc.line_column(4), Some((2, 3)));
    let non_boundary = doc.source().find('é').unwrap() + 1;
    assert_eq!(doc.line_column(non_boundary), None);
}

#[test]
fn decoded_duplicate_keys_are_rejected_with_span() {
    for text in [r#"{"a":1,"\u0061":2}"#, r#"{"x":{"a":0,"a":0}}"#] {
        let e = Document::parse(text).unwrap_err();
        assert_eq!(e.code, DiagnosticCode::DuplicateKey);
        assert!(e.span.unwrap().end <= text.len());
    }
}

#[test]
fn numbers_compare_exactly_without_expanding_exponents() {
    for (a, b) in [
        ("1.00", "10e-1"),
        ("-0", "0e99999999999999999999999"),
        ("12300e-2", "123"),
        ("1e99999999999999999999999", "10e99999999999999999999998"),
    ] {
        assert!(
            Document::parse(a)
                .unwrap()
                .semantically_eq(&Document::parse(b).unwrap()),
            "{a} != {b}"
        );
    }
    for (a, b) in [
        ("9007199254740992", "9007199254740993"),
        ("-1", "1"),
        ("1e1000", "1e1001"),
    ] {
        assert!(
            !Document::parse(a)
                .unwrap()
                .semantically_eq(&Document::parse(b).unwrap())
        );
    }
}

#[test]
fn resource_limits_have_explicit_boundary_errors() {
    let limits = Limits {
        max_input_bytes: 4,
        ..Limits::default()
    };
    Document::parse_with_limits("null", limits).unwrap();
    assert_eq!(
        Document::parse_with_limits("false", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let limits = Limits {
        max_depth: 2,
        ..Limits::default()
    };
    Document::parse_with_limits("[[]]", limits).unwrap();
    assert_eq!(
        Document::parse_with_limits("[[[]]]", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let limits = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    Document::parse_with_limits("[0]", limits).unwrap();
    assert_eq!(
        Document::parse_with_limits("[0,1]", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    let limits = Limits {
        max_number_bytes: 3,
        ..Limits::default()
    };
    Document::parse_with_limits("123", limits).unwrap();
    assert_eq!(
        Document::parse_with_limits("1234", limits)
            .unwrap_err()
            .code,
        DiagnosticCode::Budget
    );
    // Brackets in strings/comments do not consume nesting budget.
    Document::parse_with_limits(r#"["[[[[",/* {{{ */0]"#, limits).unwrap();
}

#[test]
fn policies_use_exact_object_paths_and_validate_all_inputs() {
    let policy = Policy::from_json(
        r#"{"version":1,"arrays":[{"path":"/a~1b/~0","key":"id","order":"ordered"}]}"#,
    )
    .unwrap();
    policy
        .validate(&Document::parse(r#"{"a/b":{"~":[{"id":"x"}]}}"#).unwrap())
        .unwrap();
    policy.validate(&Document::parse("{}").unwrap()).unwrap();
    for text in [
        r#"{"a/b":{"~":0}}"#,
        r#"{"a/b":{"~":[{"id":1}]}}"#,
        r#"{"a/b":{"~":[{}]}}"#,
        r#"{"a/b":{"~":[{"id":"x"},{"id":"\u0078"}]}}"#,
    ] {
        let doc = Document::parse(text).unwrap();
        assert!(policy.validate(&doc).is_err());
        assert!(
            merge(&doc, &doc, &doc, &policy).is_err(),
            "must validate before equality shortcuts"
        );
    }
    for text in [
        r#"{"version":2,"arrays":[]}"#,
        r#"{"version":1,"version":1,"arrays":[]}"#,
        r#"{"version":1,"arrays":[{"path":"/a~2","key":"id","order":"ordered"}]}"#,
        r#"{"version":1,"arrays":[],"typo":1}"#,
    ] {
        assert!(Policy::from_json(text).is_err());
    }
    let rule = |path: &str| ArrayPolicy {
        path: path.into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    };
    assert!(Policy::new(vec![rule("/a"), rule("/a")]).is_err());
    assert!(Policy::new(vec![rule("/a"), rule("/a/0")]).is_err());
    let policy = Policy::new(vec![rule("/a/0")]).unwrap();
    assert!(
        policy
            .validate(&Document::parse(r#"{"a":[[]]}"#).unwrap())
            .is_err()
    );
}

#[test]
fn immutable_snapshots_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Document>();
    assert_send_sync::<jsonc_merge::MergePlan>();
}

#[test]
fn whitespace_extensions_are_not_silently_accepted() {
    for text in ["\u{a0}null", "[1,\u{c}2]", "\u{feff}{}", "[1\u{2003},2]"] {
        assert!(Document::parse(text).is_err(), "accepted {text:?}");
    }
    Document::parse(" \t\r\n null ").unwrap();
    Document::parse("/* \u{a0} */ \"\u{a0}\"").unwrap();
    let doc = Document::parse("{\r\n\"a\":1,\r\"b\":2\n}").unwrap();
    assert_eq!(
        doc.line_column(doc.source().find('b').unwrap()),
        Some((3, 2))
    );
    assert_eq!(
        doc.line_column(doc.source().find('}').unwrap()),
        Some((4, 1))
    );
}

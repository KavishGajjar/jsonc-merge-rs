use jsonc_merge::{ArrayOrder, ArrayPolicy, Document, Policy, merge};
mod support;

fn document(ids: &[usize], changed: bool, crlf: bool) -> String {
    let mut out = String::from("// document header\n{\n \"items\": [\n");
    for (i, id) in ids.iter().enumerate() {
        out.push_str(&format!(
            "  // record {id}\n  {{\"id\":\"r{id}\", /* internal {id} */ \"value\":{}}}",
            usize::from(changed && *id == 1)
        ));
        if i + 1 < ids.len() {
            out.push(',');
        }
        out.push_str(&format!("   /* tail, {id}\n continuation */\n"));
    }
    out.push_str(" // array closing\n ],\n \"untouched\": [ 1e0, 2, ], // keep\n // object closing\n}\n// document footer\n");
    if crlf { out.replace('\n', "\r\n") } else { out }
}

#[test]
fn moved_and_deleted_records_keep_comment_ownership_across_line_endings() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "/items".into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    }])
    .unwrap();
    let sequences: &[&[usize]] = &[
        &[],
        &[0],
        &[1],
        &[0, 1],
        &[1, 0],
        &[2, 0, 1],
        &[0, 2, 1],
        &[0, 1, 2],
    ];
    for crlf in [false, true] {
        for ours in sequences {
            for theirs in sequences {
                let b = Document::parse(document(&[0, 1], false, crlf)).unwrap();
                let o = Document::parse(document(ours, false, crlf)).unwrap();
                let t = Document::parse(document(theirs, true, crlf)).unwrap();
                let plan = merge(&b, &o, &t, &policy).unwrap();
                let names =
                    |ids: &[usize]| ids.iter().map(|id| format!("r{id}")).collect::<Vec<_>>();
                let expected = support::oracle(&names(&[0, 1]), &names(ours), &names(theirs));
                let delete_edit = !ours.contains(&1) && theirs.contains(&1);
                if delete_edit || expected.is_none() {
                    assert!(
                        !plan.is_clean(),
                        "missing conflict: o={ours:?} t={theirs:?}"
                    );
                    continue;
                }
                assert!(
                    plan.is_clean(),
                    "spurious conflict: o={ours:?} t={theirs:?}: {:?}",
                    plan.conflicts()
                );
                let out = plan
                    .render()
                    .unwrap_or_else(|e| panic!("o={ours:?} t={theirs:?}: {e}"));
                let expected = serde_json::json!({
                    "items": expected.unwrap().iter().map(|id| serde_json::json!({"id":id,"value":usize::from(id == "r1" && theirs.contains(&1))})).collect::<Vec<_>>(),
                    "untouched":[1,2]
                });
                assert!(
                    Document::parse(&out)
                        .unwrap()
                        .semantically_eq(&Document::parse(expected.to_string()).unwrap()),
                    "o={ours:?} t={theirs:?}: {out}"
                );
                assert!(out.contains("\"untouched\": [ 1e0, 2, ]"));
                for text in [
                    "// document header",
                    "// document footer",
                    "// object closing",
                    "// array closing",
                ] {
                    assert!(out.contains(text), "lost {text}: {out}");
                }
                for id in 0..3 {
                    let occurs = out.contains(&format!("\"id\":\"r{id}\""));
                    assert_eq!(occurs, out.contains(&format!("// record {id}")), "{out}");
                    assert_eq!(occurs, out.contains(&format!("/* tail, {id}")), "{out}");
                }
                if crlf {
                    assert!(out.contains("\r\n"));
                }
            }
        }
    }
}

#[test]
fn changing_an_internal_or_trailing_comment_preserves_selected_tokens() {
    let b = "{\n // leading\n \"a\" /* internal */: 1,   /* tail */\n \"b\": 1\n}";
    for needle in ["leading", "internal", "tail"] {
        let o = b.replace(needle, &format!("new {needle}"));
        let t = b.replace("\"b\": 1", "\"b\": 2");
        let plan = merge(
            &Document::parse(b).unwrap(),
            &Document::parse(&o).unwrap(),
            &Document::parse(t).unwrap(),
            &Policy::default(),
        )
        .unwrap();
        let out = plan.render().unwrap();
        assert!(out.contains(&format!("new {needle}")), "{out}");
        assert!(out.contains("\"b\": 2"));
    }
}

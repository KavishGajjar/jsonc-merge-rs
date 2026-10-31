//! Independent specification oracle: enumerate every total order and accept the
//! first that satisfies all selected pair constraints. No graph/toposort code.
use jsonc_merge::{ArrayOrder, ArrayPolicy, Document, Policy, merge};
use serde_json::{Value, json};
mod support;
use support::{oracle, permutations};

fn doc(seq: &[String]) -> Document {
    Document::parse(
        json!({"items":seq.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()} ).to_string(),
    )
    .unwrap()
}
fn compare(b: &[String], o: &[String], t: &[String], policy: &Policy) {
    let expected = oracle(b, o, t);
    let plan = merge(&doc(b), &doc(o), &doc(t), policy).unwrap();
    match expected {
        None => assert!(!plan.is_clean(), "missed conflict b={b:?} o={o:?} t={t:?}"),
        Some(expected) => {
            assert!(
                plan.is_clean(),
                "spurious conflict b={b:?} o={o:?} t={t:?}: {:?}",
                plan.conflicts()
            );
            let out: Value = serde_json::from_str(&plan.render().unwrap()).unwrap();
            let actual: Vec<_> = out["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap().to_owned())
                .collect();
            assert_eq!(actual, expected, "b={b:?} o={o:?} t={t:?}");
        }
    }
}

#[test]
fn exhaustive_four_record_reorders() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "/items".into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    }])
    .unwrap();
    let base: Vec<_> = ["a", "b", "c", "d"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let orders = permutations(&base);
    for ours in &orders {
        for theirs in &orders {
            compare(&base, ours, theirs, &policy);
        }
    }
}

#[test]
fn exhaustive_insert_delete_orders_with_three_identities() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "/items".into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    }])
    .unwrap();
    let universe: Vec<_> = ["a", "b", "c"].into_iter().map(str::to_owned).collect();
    let mut sequences = Vec::new();
    for mask in 0..8 {
        let subset: Vec<_> = universe
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, s)| s.clone())
            .collect();
        sequences.extend(permutations(&subset));
    }
    for base in &sequences {
        for ours in &sequences {
            for theirs in &sequences {
                compare(base, ours, theirs, &policy);
            }
        }
    }
}

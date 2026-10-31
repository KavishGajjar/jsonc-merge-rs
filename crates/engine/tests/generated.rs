use jsonc_merge::{Document, Policy, merge};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

struct Generator(u64);
impl Generator {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 24
    }
    fn value(&mut self, depth: usize) -> Value {
        match self.next() % if depth == 0 { 4 } else { 6 } {
            0 => Value::Null,
            1 => json!(self.next() % 5),
            2 => json!(self.next().is_multiple_of(2)),
            3 => json!(["a", "雪", "~x/y"][self.next() as usize % 3]),
            4 => json!([self.value(depth - 1), self.value(depth - 1)]),
            _ => {
                let mut map = Map::new();
                for key in ["a", "b", "c"] {
                    if !self.next().is_multiple_of(3) {
                        map.insert(key.into(), self.value(depth - 1));
                    }
                }
                Value::Object(map)
            }
        }
    }
    fn change(&mut self, original: &Value, depth: usize) -> Value {
        if self.next().is_multiple_of(3) {
            return original.clone();
        }
        if let Value::Object(object) = original {
            let mut result = object.clone();
            let key = ["a", "b", "c"][self.next() as usize % 3];
            if self.next().is_multiple_of(3) {
                result.remove(key);
            } else if depth > 0 && result.contains_key(key) {
                let old = result[key].clone();
                result.insert(key.into(), self.change(&old, depth - 1));
            } else {
                result.insert(key.into(), self.value(depth));
            }
            Value::Object(result)
        } else {
            self.value(depth)
        }
    }
}

// Small independent JSON-only reference. Equality shortcuts precede recursion;
// production does not use this implementation or serde values for decisions.
fn reference(b: Option<&Value>, o: Option<&Value>, t: Option<&Value>) -> Result<Option<Value>, ()> {
    if o == t || b == t {
        return Ok(o.cloned());
    }
    if b == o {
        return Ok(t.cloned());
    }
    if let (Some(Value::Object(o)), Some(Value::Object(t))) = (o, t) {
        let empty = Map::new();
        let b = match b {
            Some(Value::Object(b)) => b,
            None => &empty,
            _ => return Err(()),
        };
        let keys: BTreeSet<_> = b.keys().chain(o.keys()).chain(t.keys()).collect();
        let mut result = Map::new();
        for key in keys {
            if let Some(value) = reference(b.get(key), o.get(key), t.get(key))? {
                result.insert(key.clone(), value);
            }
        }
        return Ok(Some(Value::Object(result)));
    }
    Err(())
}

#[test]
fn seeded_json_merge_matches_independent_reference_and_branch_symmetry() {
    let mut generator = Generator(0x55be_1234_7aef_0041);
    for case in 0..2000 {
        let b = generator.value(3);
        let o = generator.change(&b, 2);
        let t = generator.change(&b, 2);
        let expected = reference(Some(&b), Some(&o), Some(&t));
        let bd = Document::parse(b.to_string()).unwrap();
        let od = Document::parse(o.to_string()).unwrap();
        let td = Document::parse(t.to_string()).unwrap();
        for (ours, theirs) in [(&od, &td), (&td, &od)] {
            let plan = merge(&bd, ours, theirs, &Policy::default()).unwrap();
            match &expected {
                Err(()) => assert!(!plan.is_clean(), "case {case}: b={b} o={o} t={t}"),
                Ok(Some(value)) => {
                    assert!(
                        plan.is_clean(),
                        "case {case}: b={b} o={o} t={t}: {:?}",
                        plan.conflicts()
                    );
                    assert_eq!(
                        serde_json::from_str::<Value>(&plan.render().unwrap()).unwrap(),
                        *value,
                        "case {case}"
                    );
                }
                Ok(None) => unreachable!("root cannot be deleted"),
            }
        }
        // One-sided identity law and immutable snapshot preservation.
        let output = merge(&bd, &bd, &td, &Policy::default())
            .unwrap()
            .render()
            .unwrap();
        assert!(Document::parse(output).unwrap().semantically_eq(&td));
        assert_eq!(bd.source(), b.to_string());
    }
}

#[test]
fn supported_depth_and_large_flat_documents() {
    let text = format!("{}0{}", "{\"a\":".repeat(128), "}".repeat(128));
    let doc = Document::parse(&text).unwrap();
    assert_eq!(
        merge(&doc, &doc, &doc, &Policy::default())
            .unwrap()
            .render()
            .unwrap(),
        text
    );
    let value: Map<String, Value> = (0..5000).map(|i| (format!("key_{i}"), json!(i))).collect();
    let doc = Document::parse(Value::Object(value).to_string()).unwrap();
    assert_eq!(
        merge(&doc, &doc, &doc, &Policy::default())
            .unwrap()
            .render()
            .unwrap(),
        doc.source()
    );
}

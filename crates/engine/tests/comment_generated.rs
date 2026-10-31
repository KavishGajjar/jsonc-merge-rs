use jsonc_merge::{ArrayOrder, ArrayPolicy, Document, Policy, merge};
use serde_json::{Value, json};

struct Generator(u64);
impl Generator {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn value(&mut self, depth: usize) -> Value {
        if depth == 0 {
            return json!(self.next() % 7);
        }
        match self.next() % 4 {
            0 => json!([self.value(depth - 1), self.value(depth - 1)]),
            1 => {
                let mut fields = serde_json::Map::new();
                for key in ["a", "b", "雪"] {
                    if !self.next().is_multiple_of(3) {
                        fields.insert(key.into(), self.value(depth - 1));
                    }
                }
                Value::Object(fields)
            }
            _ => json!(["雪", "/* text */", "escaped\nline"][self.next() as usize % 3]),
        }
    }

    fn source(&mut self, value: &Value) -> String {
        let (entries, open, close): (Vec<_>, _, _) = match value {
            Value::Object(fields) => (
                fields.iter().map(|(k, v)| (Some(k.as_str()), v)).collect(),
                '{',
                '}',
            ),
            Value::Array(elements) => (elements.iter().map(|v| (None, v)).collect(), '[', ']'),
            _ => return value.to_string(),
        };
        let newline = ["\n", "\r\n", "\r"][self.next() as usize % 3];
        let mut source = format!("{open}{newline}");
        for (key, value) in entries {
            source.push_str(
                [" // leading\n", " /* leading\n, body */ ", ""][self.next() as usize % 3],
            );
            if let Some(key) = key {
                source.push_str(&serde_json::to_string(key).unwrap());
                source.push_str(
                    [" /* internal */ : ", ": // internal\n", ":"][self.next() as usize % 3],
                );
            }
            source.push_str(&self.source(value));
            source.push_str(
                [", // trailing", " /* trailing\n, body */,", ","][self.next() as usize % 3],
            );
            source.push_str(newline);
        }
        source.push_str(" /* closing */");
        source.push_str(newline);
        source.push(close);
        source
    }
}

#[test]
fn one_sided_jsonc_changes_always_render_with_their_values_and_comments() {
    let mut generator = Generator(0x6287_9504_fadd_a001);
    for case in 0..1000 {
        let base_value = generator.value(3);
        let theirs_value = generator.value(3);
        let base_text = generator.source(&base_value);
        let theirs_text = generator.source(&theirs_value);
        let base = Document::parse(&base_text).unwrap();
        let theirs = Document::parse(&theirs_text).unwrap();
        for (ours, theirs) in [(&base, &theirs), (&theirs, &base)] {
            let plan = merge(&base, ours, theirs, &Policy::default()).unwrap();
            assert!(plan.is_clean(), "case {case}: {:?}", plan.conflicts());
            let output = plan.render().unwrap_or_else(|e| {
                panic!("case {case}: {e}\nbase={base_text}\ntheirs={theirs_text}")
            });
            let expected = if std::ptr::eq(ours, &base) {
                theirs
            } else {
                ours
            };
            assert!(
                Document::parse(output).unwrap().semantically_eq(expected),
                "case {case}"
            );
        }
    }
}

#[test]
fn one_sided_keyed_record_changes_always_render() {
    let policy = Policy::new(vec![ArrayPolicy {
        path: "".into(),
        key: "id".into(),
        order: ArrayOrder::Ordered,
    }])
    .unwrap();
    let mut generator = Generator(0x3389_5114_9810_9211);
    for case in 0..500 {
        let values = [generator.value(2), generator.value(2)];
        let mut records = vec![
            json!({"id":"a","value":values[0]}),
            json!({"id":"b","value":values[1]}),
        ];
        let base_text = generator.source(&Value::Array(records.clone()));
        if generator.next().is_multiple_of(2) {
            records.reverse();
        }
        records[0]["value"] = generator.value(2);
        let theirs_text = generator.source(&Value::Array(records));
        let base = Document::parse(&base_text).unwrap();
        let theirs = Document::parse(&theirs_text).unwrap();
        let plan = merge(&base, &base, &theirs, &policy).unwrap();
        assert!(plan.is_clean(), "case {case}: {:?}", plan.conflicts());
        let output = plan
            .render()
            .unwrap_or_else(|e| panic!("case {case}: {e}\nbase={base_text}\ntheirs={theirs_text}"));
        assert!(Document::parse(output).unwrap().semantically_eq(&theirs));
    }
}

//! Test-only total-order oracle, independent of production graph sorting.
use std::collections::BTreeSet;

pub fn permutations<T: Clone>(xs: &[T]) -> Vec<Vec<T>> {
    if xs.is_empty() {
        return vec![vec![]];
    }
    let mut out = Vec::new();
    for i in 0..xs.len() {
        let rest: Vec<_> = xs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, x)| x.clone())
            .collect();
        for mut p in permutations(&rest) {
            p.insert(0, xs[i].clone());
            out.push(p);
        }
    }
    out
}
fn position(seq: &[String], x: &str, y: &str) -> Option<bool> {
    Some(seq.iter().position(|s| s == x)? < seq.iter().position(|s| s == y)?)
}
pub fn oracle(b: &[String], o: &[String], t: &[String]) -> Option<Vec<String>> {
    let set: BTreeSet<_> = o
        .iter()
        .chain(t)
        .filter(|x| !b.contains(x) || (o.contains(x) && t.contains(x)))
        .cloned()
        .collect();
    let sorted: Vec<_> = set.into_iter().collect();
    let mut orders = permutations(&sorted);
    orders.sort();
    orders.into_iter().find(|candidate| {
        for x in 0..sorted.len() {
            for y in x + 1..sorted.len() {
                let (x, y) = (&sorted[x], &sorted[y]);
                let base = position(b, x, y);
                let ours = position(o, x, y);
                let theirs = position(t, x, y);
                // Inversion-set formulation: a changed branch relation supersedes
                // an unchanged one; new pairs must satisfy both available branches.
                let allowed: Vec<bool> = if let Some(base) = base {
                    let changed: Vec<_> = [ours, theirs]
                        .into_iter()
                        .flatten()
                        .filter(|r| *r != base)
                        .collect();
                    if changed.is_empty() {
                        vec![ours.or(theirs).unwrap_or(base)]
                    } else {
                        changed
                    }
                } else {
                    [ours, theirs].into_iter().flatten().collect()
                };
                if !allowed
                    .iter()
                    .all(|r| Some(*r) == position(candidate, x, y))
                {
                    return false;
                }
            }
        }
        true
    })
}

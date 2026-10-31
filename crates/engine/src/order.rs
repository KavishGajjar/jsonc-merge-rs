use crate::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};

/// Select pairwise precedences, then return the lexicographically first legal order.
/// Err carries an actual directed cycle, or a contradictory new pair.
pub(crate) fn reconcile_order(
    base: &[String],
    ours: &[String],
    theirs: &[String],
    survivors: &BTreeSet<String>,
    remaining_pairs: &mut usize,
) -> Result<Result<Vec<String>, Vec<String>>, Diagnostic> {
    let ids: Vec<_> = survivors.iter().cloned().collect();
    let pairs = ids
        .len()
        .checked_mul(ids.len().saturating_sub(1))
        .and_then(|v| v.checked_div(2))
        .ok_or_else(|| Diagnostic::budget("order pair count overflow"))?;
    *remaining_pairs = remaining_pairs
        .checked_sub(pairs)
        .ok_or_else(|| Diagnostic::budget("ordered-record pair limit exceeded"))?;
    fn positions(seq: &[String]) -> BTreeMap<&str, usize> {
        seq.iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect()
    }
    let bp = positions(base);
    let op = positions(ours);
    let tp = positions(theirs);
    let mut edges = vec![Vec::new(); ids.len()];
    let mut incoming = vec![Vec::new(); ids.len()];
    let mut indegree = vec![0; ids.len()];
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            let relation =
                |p: &BTreeMap<&str, usize>| Some(p.get(ids[i].as_str())? < p.get(ids[j].as_str())?);
            let b = relation(&bp);
            let o = relation(&op);
            let t = relation(&tp);
            let chosen = match (b, o, t) {
                (_, Some(x), Some(y)) if x == y => Some(x),
                (Some(b), Some(o), Some(t)) => Some(if o == b { t } else { o }),
                (None, Some(_), Some(_)) => {
                    return Ok(Err(vec![ids[i].clone(), ids[j].clone(), ids[i].clone()]));
                }
                (_, Some(x), None) | (_, None, Some(x)) => Some(x),
                (b, None, None) => b,
            };
            if let Some(forward) = chosen {
                let (a, b) = if forward { (i, j) } else { (j, i) };
                edges[a].push(b);
                incoming[b].push(a);
                indegree[b] += 1;
            }
        }
    }
    let mut ready: BTreeSet<usize> = (0..ids.len()).filter(|i| indegree[*i] == 0).collect();
    let mut result = Vec::new();
    while let Some(i) = ready.pop_first() {
        result.push(ids[i].clone());
        for j in &edges[i] {
            indegree[*j] -= 1;
            if indegree[*j] == 0 {
                ready.insert(*j);
            }
        }
    }
    if result.len() == ids.len() {
        return Ok(Ok(result));
    }
    // Every remaining vertex has a remaining predecessor. Follow predecessors to a cycle.
    let mut at = indegree.iter().position(|n| *n > 0).unwrap();
    let mut seen = BTreeMap::new();
    let mut trail = Vec::new();
    loop {
        if let Some(start) = seen.insert(at, trail.len()) {
            let mut cycle: Vec<_> = trail[start..]
                .iter()
                .rev()
                .map(|i: &usize| ids[*i].clone())
                .collect();
            cycle.push(cycle[0].clone());
            return Ok(Err(cycle));
        }
        trail.push(at);
        at = *incoming[at].iter().find(|i| indegree[**i] > 0).unwrap();
    }
}

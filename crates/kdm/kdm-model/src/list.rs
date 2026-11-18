//! Ordered lists and orders: making a replica list hold exactly the objects a view holds with as
//! few operations as possible ([`sync_list`], Kition Slide's), and keeping a table's order in its
//! records' `order` numbers rewriting as few as possible ([`assign_orders`]).

use std::collections::{HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};

use crate::KdmResult;

/// Makes `owner.field` list exactly `target` (distinct objects): entries not in `target`, and
/// repeated entries, are removed (their objects are not deleted: the caller decides that); the
/// entries already there keep their place as far as possible (the longest run already in
/// `target`'s order stays put) and the rest are moved; new entries are inserted. So moving one
/// element is one `list_move`. Answers how many list operations it wrote.
pub(crate) fn sync_list(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, target: &[ObjectId]) -> KdmResult<usize> {
    let mut current = tx.list_items(owner, field)?;
    if current == target {
        return Ok(0);
    }
    let wanted: HashSet<&ObjectId> = target.iter().collect();
    let mut ops = 0;
    let mut seen = HashSet::new();
    let keep: Vec<bool> = current.iter().map(|o| wanted.contains(o) && seen.insert(o.clone())).collect();
    for index in (0..current.len()).rev() {
        if !keep.get(index).copied().unwrap_or(false) {
            tx.list_remove(owner, field, index)?;
            ops += 1;
        }
    }
    let mut index = 0;
    current.retain(|_| {
        let kept = keep.get(index).copied().unwrap_or(false);
        index += 1;
        kept
    });
    // What is left already starts the target (a new list, appended entities): add the rest at
    // the end, without looking anything up.
    if target.starts_with(&current) {
        let base = current.len();
        for (k, object) in target.iter().skip(base).enumerate() {
            tx.list_insert(owner, field, base + k, object)?;
            ops += 1;
        }
        return Ok(ops);
    }
    // Target positions of what is already there, in its current order; the longest increasing
    // run of them stays.
    let position: HashMap<&ObjectId, usize> = target.iter().enumerate().map(|(i, o)| (o, i)).collect();
    let order: Vec<usize> = current.iter().filter_map(|o| position.get(o).copied()).collect();
    let stays: HashSet<usize> = longest_increasing(&order).into_iter().filter_map(|i| order.get(i).copied()).collect();
    for (j, object) in target.iter().enumerate() {
        if stays.contains(&j) {
            continue;
        }
        let after = match j.checked_sub(1).and_then(|p| target.get(p)) {
            Some(prev) => current.iter().position(|o| o == prev).map_or(0, |i| i + 1),
            None => 0,
        };
        match current.iter().position(|o| o == object) {
            Some(from) => {
                let to = if from < after { after - 1 } else { after };
                if from != to {
                    tx.list_move(owner, field, from, to)?;
                    let moved = current.remove(from);
                    current.insert(to, moved);
                    ops += 1;
                }
            }
            None => {
                tx.list_insert(owner, field, after, object)?;
                current.insert(after, object.clone());
                ops += 1;
            }
        }
    }
    Ok(ops)
}

/// The indexes into `values` of one longest strictly increasing subsequence of `values`.
pub(crate) fn longest_increasing<T: PartialOrd + Copy>(values: &[T]) -> Vec<usize> {
    // tails[k]: index into `values` of the smallest tail of an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; values.len()];
    for (i, v) in values.iter().enumerate() {
        let k = tails.partition_point(|&t| values.get(t).is_some_and(|x| x < v));
        if let Some(slot) = prev.get_mut(i) {
            *slot = k.checked_sub(1).and_then(|p| tails.get(p).copied());
        }
        if k == tails.len() {
            tails.push(i);
        } else if let Some(t) = tails.get_mut(k) {
            *t = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(i);
        at = prev.get(i).copied().flatten();
    }
    out.reverse();
    out
}

/// `order` numbers for items in their new order, given each item's current number (`None`: a new
/// item): the longest run of current numbers already in increasing order is kept, the others get
/// numbers between their neighbours (after the last: one more; before the first: one less). So
/// appending, deleting, or moving one record writes at most one number. Falls back to `0, 1, 2…`
/// when the numbers between two neighbours run out of precision.
pub(crate) fn assign_orders(current: &[Option<f64>]) -> Vec<f64> {
    let known: Vec<(usize, f64)> = current.iter().enumerate().filter_map(|(i, v)| v.filter(|x| x.is_finite()).map(|x| (i, x))).collect();
    let values: Vec<f64> = known.iter().map(|(_, v)| *v).collect();
    let kept: HashMap<usize, f64> = longest_increasing(&values).into_iter().filter_map(|k| known.get(k).copied()).collect();
    let n = current.len();
    let mut out: Vec<f64> = vec![0.0; n];
    let mut i = 0;
    let mut prev: Option<f64> = None;
    while i < n {
        if let Some(v) = kept.get(&i) {
            if let Some(slot) = out.get_mut(i) {
                *slot = *v;
            }
            prev = Some(*v);
            i += 1;
            continue;
        }
        // A gap: positions i..j, before the next kept number (if any).
        let mut j = i;
        while j < n && !kept.contains_key(&j) {
            j += 1;
        }
        let next = kept.get(&j).copied();
        let len = (j - i) as f64;
        for (k, slot) in out.iter_mut().enumerate().take(j).skip(i) {
            let step = (k - i + 1) as f64;
            *slot = match (prev, next) {
                (Some(p), Some(q)) => p + (q - p) * step / (len + 1.0),
                (Some(p), None) => p + step,
                (None, Some(q)) => q - (len + 1.0 - step),
                (None, None) => step - 1.0,
            };
        }
        prev = out.get(j.saturating_sub(1)).copied().or(prev);
        i = j;
    }
    if out.windows(2).all(|w| matches!(w, [a, b] if a < b)) && out.iter().all(|v| v.is_finite()) { out } else { (0..n).map(|i| i as f64).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::kdm::{ActorId, DocId, Origin, Replica};

    fn replica() -> Replica {
        Replica::create(
            <crate::CadModel as kition_kit::session::KitModel>::schema(),
            DocId::new(),
            ActorId::from_u64(1).unwrap_or_else(|_| panic!("actor")),
        )
        .unwrap_or_else(|error| panic!("create: {error}"))
    }

    fn with_list(n: usize) -> (Replica, Vec<ObjectId>) {
        let mut r = replica();
        let root = crate::root_id();
        let objects = r
            .transact(&Origin::system("t"), |tx| {
                tx.create_with_id("Drawing", &root, Default::default())?;
                let mut out = Vec::new();
                for i in 0..n {
                    let fields = serde_json::Map::from_iter([
                        ("handle".to_owned(), serde_json::json!(format!("{:X}", 0x100 + i))),
                        ("common".to_owned(), serde_json::json!({})),
                        ("kind".to_owned(), serde_json::json!({})),
                    ]);
                    let o = tx.create("Entity", fields)?;
                    tx.list_insert(&root, "model", i, &o)?;
                    out.push(o);
                }
                Ok(out)
            })
            .unwrap_or_else(|error| panic!("{error}"));
        (r, objects)
    }

    fn sync(r: &mut Replica, target: &[ObjectId]) -> usize {
        let root = crate::root_id();
        let ops = r.transact(&Origin::system("t"), |tx| sync_list(tx, &root, "model", target)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(r.list(&root, "model").unwrap_or_default(), target);
        ops
    }

    #[test]
    fn one_move_is_one_operation() {
        let (mut r, o) = with_list(5);
        assert_eq!(sync(&mut r, &[o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone(), o[0].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[0].clone(), o[1].clone(), o[2].clone(), o[3].clone(), o[4].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[0].clone(), o[3].clone(), o[1].clone(), o[2].clone(), o[4].clone()]), 1);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 3);
        assert_eq!(sync(&mut r, &[o[4].clone(), o[3].clone(), o[2].clone(), o[1].clone(), o[0].clone()]), 0);
    }

    #[test]
    fn removes_and_moves_together() {
        let (mut r, o) = with_list(4);
        assert_eq!(sync(&mut r, &[o[3].clone(), o[0].clone(), o[2].clone()]), 2);
        assert_eq!(sync(&mut r, &[]), 3);
    }

    #[test]
    fn longest_increasing_run() {
        assert_eq!(longest_increasing(&[3, 0, 1, 2]), vec![1, 2, 3]);
        assert_eq!(longest_increasing::<usize>(&[]), Vec::<usize>::new());
        assert_eq!(longest_increasing(&[2, 1, 0]).len(), 1);
    }

    #[test]
    fn orders_keep_what_they_can() {
        // Appending: the others keep theirs, the new one goes after the last.
        assert_eq!(assign_orders(&[Some(0.0), Some(1.0), None]), vec![0.0, 1.0, 2.0]);
        // Deleting the middle one changes nothing else.
        assert_eq!(assign_orders(&[Some(0.0), Some(2.0)]), vec![0.0, 2.0]);
        // Moving the last to the front: one new number, before the first.
        assert_eq!(assign_orders(&[Some(2.0), Some(0.0), Some(1.0)]), vec![-1.0, 0.0, 1.0]);
        // Inserting between: halfway.
        assert_eq!(assign_orders(&[Some(0.0), None, Some(1.0)]), vec![0.0, 0.5, 1.0]);
        // Nothing known: 0, 1, 2.
        assert_eq!(assign_orders(&[None, None, None]), vec![0.0, 1.0, 2.0]);
        // Ties (two people appended at once) are not increasing: renumbered around the kept run.
        let out = assign_orders(&[Some(3.0), Some(3.0)]);
        assert!(out[0] < out[1]);
    }
}

//! Handles stored as data, inside the JSON of a register: written as the object id of the entity
//! they name, read back as that entity's handle (`tests/id_fields.rs` lists every model field
//! that holds a handle, so a new one fails a test until it is handled here):
//! - a dimension's associations: `assoc[].handle` and an intersection snap's `other`;
//! - a group's `members`;
//! - a constraint's `refs[].handle`;
//! - the parametric settings' `barExceptions` and `dimExceptions`.
//!
//! A handle that names no entity of the view (a file's dangling reference) is written as
//! `"#<hex>"` and read back as that handle; an object id the reader does not hold (an entity a
//! merge deleted) reads as "none": the reference is dropped.
//!
//! Header values are written tagged (`{"i": …}`, `{"r": …}`, `{"s": …}`, `{"p": [x, y, z]}`):
//! the KDM writes a whole double as an integer, and an untagged real would read back as an
//! integer variable.

use kition_cad_doc::geom::Vec3;
use kition_cad_doc::{EntityKind, HVal, Handle};
use kition_kit::kdm::ObjectId;
use serde_json::{Value, json};

use crate::{Ids, KdmResult, kdm_err};

/// The largest integer every KDM implementation reads back exactly.
const SAFE: u64 = 1 << 53;

/// A handle as written: the object id of the entity it names, else `"#<hex>"`.
pub(crate) fn handle_out(h: Handle, ids: &Ids) -> Value {
    match ids.entities.object(h) {
        Some(object) => Value::String(object.as_str().to_owned()),
        None => Value::String(format!("#{:X}", h.0)),
    }
}

/// A handle read back: `None` when it names an entity the reader does not hold.
pub(crate) fn handle_in(v: &Value, ids: &Ids) -> Option<Handle> {
    match v {
        Value::String(s) => match s.strip_prefix('#') {
            Some(hex) => Handle::parse_hex(hex),
            None => ids.entities.local(&ObjectId::from_string(s.as_str())),
        },
        Value::Number(n) => n.as_u64().map(Handle),
        _ => None,
    }
}

/// Every handle-valued slot of an entity's `kind` JSON (a dimension's associations).
fn each_kind_handle(v: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    if v.get("type").and_then(Value::as_str) != Some("dimension") {
        return;
    }
    let Some(assoc) = v.get_mut("assoc").and_then(Value::as_array_mut) else { return };
    for a in assoc {
        if let Some(h) = a.get_mut("handle") {
            f(h);
        }
        if let Some(other) = a.get_mut("snap").and_then(|s| s.get_mut("intersection")).and_then(|i| i.get_mut("other")) {
            f(other);
        }
    }
}

/// An entity's `kind` register.
pub(crate) fn kind_out(kind: &EntityKind, ids: &Ids) -> KdmResult<Value> {
    if !crate::finite::is_finite(kind) {
        return Err(kdm_err("the drawing holds an object with infinite or undefined coordinates; it cannot be shared"));
    }
    let mut v = serde_json::to_value(kind).map_err(kdm_err)?;
    if matches!(kind, EntityKind::Table(_))
        && let Some(cells) = v.get_mut("cells")
    {
        let rows = cells.as_array().ok_or_else(|| kdm_err("table cells must be rows"))?;
        *cells = Value::Object(
            rows.iter()
                .enumerate()
                .map(|(r, row)| {
                    let cols = row.as_array().into_iter().flatten().enumerate().map(|(c, v)| (c.to_string(), v.clone())).collect();
                    (r.to_string(), Value::Object(cols))
                })
                .collect(),
        );
    }
    each_kind_handle(&mut v, &mut |slot| {
        if let Some(h) = slot.as_u64() {
            *slot = handle_out(Handle(h), ids);
        }
    });
    Ok(v)
}

/// An entity's `kind` read back; a dimension's associations with entities the reader does not
/// hold are dropped.
pub fn kind_in(mut v: Value, ids: &Ids) -> KdmResult<EntityKind> {
    if v.get("type").and_then(Value::as_str) == Some("table")
        && let Some(cells) = v.get_mut("cells")
    {
        fn ordered(v: &Value) -> KdmResult<Vec<Value>> {
            let m = v.as_object().ok_or_else(|| kdm_err("table cells must be indexed objects"))?;
            let mut rows = std::collections::BTreeMap::new();
            for (k, v) in m {
                rows.insert(k.parse::<usize>().map_err(kdm_err)?, v.clone());
            }
            if rows.keys().copied().ne(0..rows.len()) {
                return Err(kdm_err("table cell indices must be consecutive"));
            }
            Ok(rows.into_values().collect())
        }
        let rows = ordered(cells)?;
        *cells = Value::Array(rows.iter().map(|r| ordered(r).map(Value::Array)).collect::<KdmResult<_>>()?);
    }
    if v.get("type").and_then(Value::as_str) == Some("dimension")
        && let Some(assoc) = v.get_mut("assoc").and_then(Value::as_array_mut)
    {
        assoc.retain_mut(|a| {
            let mut ok = true;
            let mut fix = |slot: &mut Value| match handle_in(slot, ids) {
                Some(h) => *slot = json!(h.0),
                None => ok = false,
            };
            if let Some(h) = a.get_mut("handle") {
                fix(h);
            }
            if let Some(other) = a.get_mut("snap").and_then(|s| s.get_mut("intersection")).and_then(|i| i.get_mut("other")) {
                fix(other);
            }
            ok
        });
    }
    serde_json::from_value(v).map_err(kdm_err)
}

/// Whether a written handle names an object the predicate says is gone (a `"#<hex>"` handle
/// names no object, so it never is).
fn gone_ref(v: &Value, gone: &dyn Fn(&ObjectId) -> bool) -> bool {
    matches!(v, Value::String(s) if !s.starts_with('#') && gone(&ObjectId::from_string(s.as_str())))
}

/// Drops a dimension's associations with entities that are gone from its `kind` register;
/// answers whether it dropped any.
pub(crate) fn drop_gone_assoc(v: &mut Value, gone: &dyn Fn(&ObjectId) -> bool) -> bool {
    if v.get("type").and_then(Value::as_str) != Some("dimension") {
        return false;
    }
    let Some(assoc) = v.get_mut("assoc").and_then(Value::as_array_mut) else { return false };
    let before = assoc.len();
    assoc.retain(|a| {
        let other = a.get("snap").and_then(|s| s.get("intersection")).and_then(|i| i.get("other"));
        !a.get("handle").is_some_and(|h| gone_ref(h, gone)) && !other.is_some_and(|h| gone_ref(h, gone))
    });
    assoc.len() != before
}

/// Drops the handles naming entities that are gone from a JSON array at `v`; answers whether
/// it dropped any.
pub(crate) fn drop_gone(v: &mut Value, gone: &dyn Fn(&ObjectId) -> bool) -> bool {
    let Some(items) = v.as_array_mut() else { return false };
    let before = items.len();
    items.retain(|item| !gone_ref(item, gone));
    items.len() != before
}

/// Whether any of a constraint's `refs` names an entity that is gone.
pub(crate) fn refs_gone(v: &Value, gone: &dyn Fn(&ObjectId) -> bool) -> bool {
    v.as_array().is_some_and(|items| items.iter().any(|r| r.get("handle").is_some_and(|h| gone_ref(h, gone))))
}

/// Handles in a JSON array at `v` (group members, exceptions), written.
pub(crate) fn handles_out(v: &mut Value, ids: &Ids) {
    if let Some(items) = v.as_array_mut() {
        for item in items {
            if let Some(h) = item.as_u64() {
                *item = handle_out(Handle(h), ids);
            }
        }
    }
}

/// Handles in a JSON array at `v`, read back; those the reader does not hold are dropped.
pub(crate) fn handles_in(v: &mut Value, ids: &Ids) {
    if let Some(items) = v.as_array_mut() {
        items.retain_mut(|item| match handle_in(item, ids) {
            Some(h) => {
                *item = json!(h.0);
                true
            }
            None => false,
        });
    }
}

/// The parametric settings' exception lists, through `f`.
pub(crate) fn each_parametric_list(v: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    if let Some(settings) = v.get_mut("settings") {
        for key in ["barExceptions", "dimExceptions"] {
            if let Some(list) = settings.get_mut(key) {
                f(list);
            }
        }
    }
}

/// A constraint's `refs`, through `f` (each reference's `handle`).
pub(crate) fn each_ref_handle(v: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    if let Some(items) = v.as_array_mut() {
        for item in items {
            if let Some(h) = item.get_mut("handle") {
                f(h);
            }
        }
    }
}

fn num_out(x: f64) -> Value {
    if x.is_finite() {
        json!(x)
    } else {
        // "NaN", "inf", "-inf": what `f64::from_str` reads back.
        Value::String(format!("{x}"))
    }
}

fn num_in(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// A header variable's register.
pub(crate) fn hval_out(v: &HVal) -> Value {
    match v {
        HVal::Int(i) if i.unsigned_abs() <= SAFE => json!({ "i": i }),
        HVal::Int(i) => json!({ "i": i.to_string() }),
        HVal::Real(r) => json!({ "r": num_out(*r) }),
        HVal::Str(s) => json!({ "s": s }),
        HVal::Point(p) => json!({ "p": [num_out(p.x), num_out(p.y), num_out(p.z)] }),
    }
}

/// A header variable read back (an untagged value, from another writer, as the model reads it).
pub(crate) fn hval_in(v: Value) -> Option<HVal> {
    if let Some(i) = v.get("i") {
        return match i {
            Value::String(s) => s.parse().ok().map(HVal::Int),
            other => other.as_i64().map(HVal::Int),
        };
    }
    if let Some(r) = v.get("r") {
        return num_in(r).map(HVal::Real);
    }
    if let Some(s) = v.get("s") {
        return s.as_str().map(|s| HVal::Str(s.to_owned()));
    }
    if let Some(p) = v.get("p").and_then(Value::as_array) {
        let c = |i: usize| p.get(i).and_then(num_in);
        return Some(HVal::Point(Vec3::new(c(0)?, c(1)?, c(2)?)));
    }
    serde_json::from_value(v).ok()
}

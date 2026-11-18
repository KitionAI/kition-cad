//! `grip.*` commands: the undoable, scriptable form of grip editing (see `crate::grips`).

use kition_cad_doc::Handle;
use kition_cad_geom::{Mat3, Vec2};
use serde_json::{Value, json};

use super::curves;
use super::*;
use crate::grips::{GripMode, mode_matrix, stretch_selection};
use crate::{EngineError, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("grip.move", "Grip Stretch", run_grip_move).params(
            "{handle?, handles?, index, to} | {handles?, index, base?: [x,y], baseHandle?, to, mode: \"move\", copy?} (the first stretches the grip; the second moves about it)",
        ),
        CommandSpec::new("grip.stretch", "Grip Stretch", run_grip_stretch).params("{handle?, handles?, index, to}"),
        CommandSpec::new("grip.rotate", "Grip Rotate", run_grip_rotate).params("{handles?, base?: [x,y], baseHandle?, index?, angle? (degrees), to?: [x,y], copy?: bool} (the base point: base, or baseHandle with index; the angle: angle, or to)"),
        CommandSpec::new("grip.scale", "Grip Scale", run_grip_scale).params("{handles?, base?: [x,y], baseHandle?, index?, factor?: number, to?: [x,y], copy?: bool} (the base point: base, or baseHandle with index; the scale: factor, or to)"),
        CommandSpec::new("grip.mirror", "Grip Mirror", run_grip_mirror)
            .params("{handles?, base?: [x,y], baseHandle?, index?, to (second mirror point), copy?: bool} (the first mirror point: base, or baseHandle with index)"),
    ]
}

fn index_param(cmd: &str, p: &Value) -> Result<usize> {
    let i = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(cmd, "`index` (grip number) is required"))?;
    usize::try_from(i).map_err(|_| bad(cmd, "index out of range"))
}

fn stretch(s: &mut Session, cmd: &str, p: &Value) -> Result<Value> {
    let h = match p.get("handle") {
        None | Some(Value::Null) => targets(s, p)?.first().copied().ok_or_else(|| bad(cmd, "`handle` is required"))?,
        Some(v) => curves::handle_value(v).ok_or_else(|| bad(cmd, "`handle` is not an object handle"))?,
    };
    let i = index_param(cmd, p)?;
    let to = point_req(cmd, p, "to")?;
    if curves::is_locked(s, h) {
        return Err(EngineError::Other("The object is on a locked layer.".into()));
    }
    curves::entity(s, h)?;
    let edits = stretch_selection(s.doc()?, h, i, to, targets(s, p)?).ok_or_else(|| bad(cmd, "no such grip, or the result would be degenerate"))?;
    let d = s.doc_mut()?;
    for (o, k) in &edits {
        d.modify_entity(*o, |e| e.kind = k.clone())?;
    }
    Ok(json!({ "handle": h.hex(), "handles": edits.iter().map(|(o, _)| o.hex()).collect::<Vec<_>>() }))
}

/// Base point: `base`, or grip `index` of `baseHandle` (default: the first target).
fn base_point(s: &Session, cmd: &str, p: &Value, hs: &[Handle]) -> Result<Vec2> {
    if let Some(b) = point_param(p, "base") {
        return Ok(b);
    }
    let h = match p.get("baseHandle") {
        None | Some(Value::Null) => hs.first().copied().ok_or_else(|| bad(cmd, "`base` or `baseHandle` + `index` is required"))?,
        Some(v) => curves::handle_value(v).ok_or_else(|| bad(cmd, "`baseHandle` is not an object handle"))?,
    };
    let i = index_param(cmd, p)?;
    let g = curves::entity(s, h)?.kind.grips();
    g.get(i).copied().ok_or_else(|| bad(cmd, "no such grip"))
}

fn apply(s: &mut Session, hs: &[Handle], m: &Mat3, copy: bool) -> Result<Value> {
    let r = super::modify::transform_entities(s, hs, m, copy)?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

fn run_grip_stretch(s: &mut Session, p: &Value) -> Result<Value> {
    stretch(s, "grip.stretch", p)
}

fn run_grip_move(s: &mut Session, p: &Value) -> Result<Value> {
    let mode_move = str_param(p, "mode").is_some_and(|m| m.eq_ignore_ascii_case("move")) || (p.get("index").is_none() && p.get("base").is_some());
    if !mode_move {
        return stretch(s, "grip.move", p);
    }
    let hs = targets(s, p)?;
    let base = base_point(s, "grip.move", p, &hs)?;
    let to = point_req("grip.move", p, "to")?;
    let m = mode_matrix(GripMode::Move, base, to).unwrap_or(Mat3::IDENTITY);
    apply(s, &hs, &m, bool_or(p, "copy", false))
}

fn run_grip_rotate(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = base_point(s, "grip.rotate", p, &hs)?;
    let m = match p.get("angle").and_then(Value::as_f64).filter(|a| a.is_finite()) {
        Some(a) => Mat3::rotate_about(base, a.to_radians()),
        None => mode_matrix(GripMode::Rotate, base, point_req("grip.rotate", p, "to")?).ok_or_else(|| bad("grip.rotate", "invalid point"))?,
    };
    apply(s, &hs, &m, bool_or(p, "copy", false))
}

fn run_grip_scale(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = base_point(s, "grip.scale", p, &hs)?;
    let m = match p.get("factor").and_then(Value::as_f64).filter(|a| a.is_finite()) {
        Some(f) if f > 0.0 => Mat3::scale_about(base, f),
        Some(_) => return Err(bad("grip.scale", "factor must be positive")),
        None => {
            mode_matrix(GripMode::Scale, base, point_req("grip.scale", p, "to")?).ok_or_else(|| bad("grip.scale", "scale factor would be zero"))?
        }
    };
    apply(s, &hs, &m, bool_or(p, "copy", false))
}

fn run_grip_mirror(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = base_point(s, "grip.mirror", p, &hs)?;
    let to = point_req("grip.mirror", p, "to")?;
    let m = mode_matrix(GripMode::Mirror, base, to).ok_or_else(|| bad("grip.mirror", "the mirror line needs two distinct points"))?;
    apply(s, &hs, &m, bool_or(p, "copy", false))
}

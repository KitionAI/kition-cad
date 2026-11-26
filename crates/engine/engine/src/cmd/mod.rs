//! The command registry. Ids follow the Kition command naming standard (`line.add`,
//! `object.fillet`); the classic CAD name a person types at the command line (`line`, `fillet`) is
//! each command's `typed` name ([`names`]). Menu paths follow AutoCAD's menu bar so the catalog
//! doubles as the parity metric.

mod annotate;
mod array;
pub use array::array_operation;
mod audit;
mod blocks;
pub use blocks::{attribute_references, attsync_block, purge_premise_key};
pub mod clipboard;
pub mod constraints;
mod draw;
mod draw2;
mod edit;
pub mod file;
mod gripcmds;
pub(crate) mod groups;
pub use groups::ungroup_targets;
mod hatch;
pub use hatch::{HOVER_HATCH_LIMIT, boundary_outline, boundary_sources};
mod inquiry;
mod layer;
mod layout;
pub use layout::{layout_operation, viewport_layer_names};
mod measure;
mod mleader_content;
mod modify;
pub use modify::cutting_edge_handles;
mod modify2;
pub use modify2::{chspace_context, lengthened};
mod mtext_opts;
pub mod names;
mod pline_opts;
mod props;
pub use props::record_eligibility;
mod qselect;
mod settings;
mod spline_opts;
mod table;
mod trimextend;
mod utility;
pub use utility::{eval as calculate, find_matches};
mod view;

pub mod curves;
pub mod helpers;
pub mod machines;

use kition_cad_doc::Handle;
use kition_cad_geom::Vec2;
use serde::Serialize;
use serde_json::Value;

use crate::{EngineError, Interactive, Result, Session};

pub type Run = fn(&mut Session, &Value) -> Result<Value>;
pub type Enabled = fn(&Session) -> std::result::Result<(), String>;
pub type Factory = fn(&Session) -> Result<Box<dyn Interactive>>;

pub struct CommandSpec {
    pub id: &'static str,
    /// The name typed at the command line (`line` for `line.add`); the id where [`names`] lists
    /// none. The command line prints it in capitals. Programmatic calls take the id alone.
    pub typed: &'static str,
    pub label: &'static str,
    /// Menu placement, e.g. `["Draw", "Circle", "Center, Radius"]`. Empty = not in menus.
    pub menu: &'static [&'static str],
    pub shortcut: Option<&'static str>,
    pub aliases: &'static [&'static str],
    pub params: &'static str,
    pub enabled: Enabled,
    pub run: Run,
    pub undoable: bool,
    pub interactive: Option<Factory>,
    /// Can run inside another command with a leading apostrophe.
    pub transparent: bool,
}

impl CommandSpec {
    pub const fn new(id: &'static str, label: &'static str, run: Run) -> Self {
        CommandSpec {
            id,
            typed: id,
            label,
            menu: &[],
            shortcut: None,
            aliases: &[],
            params: "",
            enabled: has_doc,
            run,
            undoable: true,
            interactive: None,
            transparent: false,
        }
    }
    pub const fn menu(mut self, m: &'static [&'static str]) -> Self {
        self.menu = m;
        self
    }
    pub const fn key(mut self, k: &'static str) -> Self {
        self.shortcut = Some(k);
        self
    }
    pub const fn alias(mut self, a: &'static [&'static str]) -> Self {
        self.aliases = a;
        self
    }
    pub const fn params(mut self, p: &'static str) -> Self {
        self.params = p;
        self
    }
    pub const fn enabled(mut self, e: Enabled) -> Self {
        self.enabled = e;
        self
    }
    pub const fn noundo(mut self) -> Self {
        self.undoable = false;
        self
    }
    pub const fn interactive(mut self, f: Factory) -> Self {
        self.interactive = Some(f);
        self
    }
    pub const fn transparent(mut self) -> Self {
        self.transparent = true;
        self
    }
    pub fn info(&self, s: &Session) -> CommandInfo {
        let e = (self.enabled)(s);
        CommandInfo {
            id: self.id,
            typed: self.typed,
            label: self.label,
            menu: self.menu.to_vec(),
            shortcut: self.shortcut,
            aliases: self.aliases.to_vec(),
            params: self.params,
            interactive: self.interactive.is_some(),
            enabled: e.is_ok(),
            disabled_reason: e.err(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: &'static str,
    pub typed: &'static str,
    pub label: &'static str,
    pub menu: Vec<&'static str>,
    pub shortcut: Option<&'static str>,
    pub aliases: Vec<&'static str>,
    pub params: &'static str,
    pub interactive: bool,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

pub fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}
pub fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.state().map(|_| ()).map_err(|_| "no drawing open".into())
}
pub fn has_selection(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.selection().is_empty() { Err("nothing selected".into()) } else { Ok(()) }
}

pub fn command_specs() -> &'static [CommandSpec] {
    static SPECS: std::sync::OnceLock<Vec<CommandSpec>> = std::sync::OnceLock::new();
    SPECS.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(file::specs());
        v.extend(audit::specs());
        v.extend(edit::specs());
        v.extend(qselect::specs());
        v.extend(view::specs());
        v.extend(draw::specs());
        v.extend(draw2::specs());
        v.extend(annotate::specs());
        v.extend(hatch::specs());
        v.extend(blocks::specs());
        v.extend(modify::specs());
        v.extend(array::specs());
        v.extend(modify2::specs());
        v.extend(groups::specs());
        v.extend(gripcmds::specs());
        v.extend(layer::specs());
        v.extend(layout::specs());
        v.extend(props::specs());
        v.extend(inquiry::specs());
        v.extend(measure::specs());
        v.extend(settings::specs());
        v.extend(utility::specs());
        v.extend(table::specs());
        v.extend(constraints::specs());
        for spec in &mut v {
            if let Some(name) = names::typed(spec.id) {
                spec.typed = name;
            }
        }
        v
    })
}

/// The command with this id. Ids are exact: `execute` and every other programmatic path take
/// the id alone, never a typed name or an alias.
pub fn find_command(id: &str) -> Option<&'static CommandSpec> {
    command_specs().iter().find(|c| c.id == id)
}

/// The name command `id` is typed by at the command line, or `id` itself for no command.
pub fn typed_name(id: &str) -> &str {
    find_command(id).map_or(id, |c| c.typed)
}

/// The command a lower-case name names at the command line: by its typed name, then by its id
/// (in any case), then by an alias.
fn find_typed(lower: &str) -> Option<&'static CommandSpec> {
    let specs = command_specs();
    specs
        .iter()
        .find(|c| c.typed == lower)
        .or_else(|| specs.iter().find(|c| c.id.eq_ignore_ascii_case(lower)))
        .or_else(|| specs.iter().find(|c| c.aliases.iter().any(|a| a.eq_ignore_ascii_case(lower))))
}

/// The command a name typed at the command line starts: after a leading `'` (transparent) and the
/// `_`, `.` and `-` prefixes, a dashed form with a name of its own (-ATTEDIT, -BLOCK) before the
/// command it prefixes. A name is a command's typed name (LINE), its id (`line.add`) or an alias
/// (L). [`crate::Session::start`] and the command line's gate ([`crate::Session::step_gate`]) read
/// a name the same way.
pub fn resolve_typed(name: &str) -> Option<&'static CommandSpec> {
    let name = name.trim();
    let name = name.strip_prefix('\'').unwrap_or(name);
    let dashed = name.trim_start_matches(['_', '.']).to_ascii_lowercase();
    let plain = name.trim_start_matches(['_', '.', '-']).to_ascii_lowercase();
    find_typed(&dashed).or_else(|| find_typed(&plain))
}

/// The id of the command a typed name, id or alias (AutoCAD-style short names such as `L`, `C`,
/// `PL`) names at the command line; the name itself, in lower case, when it names none.
pub fn resolve_alias(name: &str) -> String {
    let l = name.to_ascii_lowercase();
    find_typed(&l).map(|c| c.id.to_string()).unwrap_or(l)
}

/// An error as the command line shows it: a command is named as it is typed (`line`), where the
/// error itself names the id (`line.add`) for whoever called the command.
pub fn typed_error(e: &EngineError) -> String {
    match e {
        EngineError::BadParams { cmd, msg } => EngineError::BadParams { cmd: typed_name(cmd).into(), msg: msg.clone() }.to_string(),
        EngineError::Disabled(cmd, msg) => EngineError::Disabled(typed_name(cmd).into(), msg.clone()).to_string(),
        other => other.to_string(),
    }
}

// ---------- param helpers ----------

pub(crate) fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}
pub(crate) fn f64_or(p: &Value, key: &str, default: f64) -> f64 {
    p.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(default)
}
pub(crate) fn f64_req(cmd: &str, p: &Value, key: &str) -> Result<f64> {
    p.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| bad(cmd, format!("`{key}` (number) is required")))
}
/// An optional size: `None` when absent or null, else a finite number above zero (or zero too
/// with `zero_ok`). Anything else is refused, not clamped or made absolute, so a bad value never
/// makes a degenerate object.
pub(crate) fn size_param(cmd: &str, p: &Value, key: &str, zero_ok: bool) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64().filter(|x| x.is_finite()) {
            Some(x) if x > 0.0 || (zero_ok && x == 0.0) => Ok(Some(x)),
            _ => Err(bad(cmd, format!("`{key}` must be {}", if zero_ok { "zero or positive" } else { "positive" }))),
        },
    }
}
/// `pts` without consecutive repeats (closer than `1e-12`), as the interactive LINE ignores a
/// point picked twice.
pub(crate) fn distinct_points(pts: &[Vec2]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::with_capacity(pts.len());
    for p in pts {
        if !out.last().is_some_and(|l| l.near(*p, 1e-12)) {
            out.push(*p);
        }
    }
    out
}
pub(crate) fn bool_or(p: &Value, key: &str, default: bool) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(default)
}
pub(crate) fn str_param<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}
pub fn point_value(v: &Value) -> Option<Vec2> {
    if let Some(a) = v.as_array() {
        let x = a.first()?.as_f64()?;
        let y = a.get(1)?.as_f64()?;
        let p = Vec2::new(x, y);
        return p.is_finite().then_some(p);
    }
    if let Some(s) = v.as_str() {
        return crate::prompt::parse_point(s, Vec2::ZERO);
    }
    let x = v.get("x")?.as_f64()?;
    let y = v.get("y")?.as_f64()?;
    let p = Vec2::new(x, y);
    p.is_finite().then_some(p)
}
pub(crate) fn point_param(p: &Value, key: &str) -> Option<Vec2> {
    p.get(key).and_then(point_value)
}
pub(crate) fn point_req(cmd: &str, p: &Value, key: &str) -> Result<Vec2> {
    point_param(p, key).ok_or_else(|| bad(cmd, format!("`{key}` ([x, y]) is required")))
}
pub(crate) fn points_param(p: &Value, key: &str) -> Option<Vec<Vec2>> {
    p.get(key)?.as_array()?.iter().map(point_value).collect()
}
/// Handles from `handles` (hex strings or numbers), `handle`, or else the current selection. A
/// `handle`/`handles` that can't be read is an error, never a silent fall back to the selection.
pub(crate) fn targets(s: &Session, p: &Value) -> Result<Vec<Handle>> {
    let handle = |v: &Value| v.as_str().and_then(Handle::parse_hex).or_else(|| v.as_u64().map(Handle));
    let not_handle = |key: &str, v: &Value| EngineError::Other(format!("`{key}`: {v} is not an object handle (a hex string such as \"1A2\")"));
    match p.get("handles") {
        None | Some(Value::Null) => {}
        Some(Value::Array(a)) => return a.iter().map(|v| handle(v).ok_or_else(|| not_handle("handles", v))).collect(),
        Some(v) => return Err(EngineError::Other(format!("`handles` must be an array of object handles, got {v}"))),
    }
    match p.get("handle") {
        None | Some(Value::Null) => Ok(s.selection()),
        Some(v) => Ok(vec![handle(v).ok_or_else(|| not_handle("handle", v))?]),
    }
}
pub(crate) fn ok() -> Result<Value> {
    Ok(Value::Null)
}

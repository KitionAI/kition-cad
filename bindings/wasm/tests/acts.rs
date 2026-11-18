//! Every command declares the editor state it reads (`KitCommand::reads`) and the parameter `on`
//! fills (`KitCommand::targets`): what the engine's code does, found by reading it (task card
//! C4/C5/C6-cad). Each `CommandSpec::new(id, label, run)` (and the `gc!`/`dc!` constraint
//! commands) is followed from its `run` function or closure and its `enabled` predicate through the
//! engine's free functions and the commands it runs (`execute("…")`) to:
//! - the selection: the session's selection and previous selection, `targets` (which falls back
//!   to the selection when no handles are given), `has_selection`;
//! - the current space (`part`): the space being edited or shown, MSPACE, the active viewport, the
//!   space's limits and extents as ZOOM reads them;
//! - the current properties new objects take (`active`): `current_common`, `add_entity`, and reads
//!   of the header variables `CLAYER`, `CECOLOR`, `CELTYPE`, `CELWEIGHT`, `CELTSCALE`, `TEXTSTYLE`,
//!   `DIMSTYLE`, `CMLEADERSTYLE`, `CTABLESTYLE`, `CMLSTYLE`;
//! - and the helpers that read a parameter as a handle (`targets`, `handle_param`, `hex_param`,
//!   `h_param`, `handles_at`, `pick_pair`).
//!
//! A command that reaches one and does not declare it, or declares what it never reads, fails
//! here; so does a drawing command (one that reaches `current_common` or `add_entity`) that
//! `acts::takes_place` leaves out. The commands the binding adds are declared in `acts.rs` by hand.
//! To see how the scan reached a reading, set `ACTS_DEBUG=<command id>` or `ACTS_FN=<engine
//! function>`; `ACTS_PRINT=1` prints the scan.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use kition_cad_kit::CadDocument;
use kition_kit::KitDocument;

/// Code that reads the selection: the session's selection, the previous selection, and the
/// helpers that default to the selection.
const SELECTION: &[&str] =
    &[".selection()", "s.selection", "st.selection", "self.selection", "state.selection", ".previous_selection", "targets(", "has_selection"];
/// Code that reads the current space: the space edited or shown, MSPACE and its viewport, and what
/// ZOOM reads of them.
const PART: &[&str] = &[
    ".space()",
    ".layout_space()",
    ".edit_space()",
    "st.space",
    "self.space",
    "state.space",
    ".mspace",
    "active_viewport(",
    "zoom_viewport(",
    ".zoom_limits(",
    ".zoom_extents(",
];
/// Code that gives new objects the current properties.
const DRAWS: &[&str] = &["current_common(", "add_entity("];
/// Header variables new objects take, read with `str`, `i64`, `f64` or `get`.
const CURRENT: &[&str] =
    &["CLAYER", "CECOLOR", "CELTYPE", "CELWEIGHT", "CELTSCALE", "TEXTSTYLE", "DIMSTYLE", "CMLEADERSTYLE", "CTABLESTYLE", "CMLSTYLE"];
/// Helpers that read a parameter as an entity handle, and which of their arguments is the key.
const HANDLE_HELPERS: &[&str] = &["handle_param(", "hex_param(", "h_param(", "handles_at(", "pick_pair("];
/// The engine's plumbing every command passes through, which carries the selection and the space
/// along (to restore them on undo, to remember the previous selection) without a command reading
/// them for what it does.
const PLUMBING: &[&str] = &[
    "execute",
    "execute_inner",
    "after_command",
    "push_undo",
    "set_selection",
    "remember_selection",
    "top",
    "sync_kdm",
    "touch",
    "echo",
    // The command registry: looking a command up reads nothing.
    "find_command",
    "command_specs",
    "specs",
    "resolve_alias",
];

fn engine_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/engine/engine/src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path.file_name().is_some_and(|n| n == "tests.rs" || n.to_string_lossy().ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

/// `source` with every comment blanked (to spaces), and with string and character literals
/// blanked too unless `strings`; same length, byte for byte.
fn blanked(source: &str, strings: bool) -> Vec<u8> {
    let b = source.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let blank = |out: &mut Vec<u8>, from: usize, to: usize, really: bool| {
        if really {
            for byte in out.iter_mut().take(to).skip(from) {
                *byte = b' ';
            }
        }
    };
    while i < b.len() {
        let ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if b[i..].starts_with(b"//") {
            let end = b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |n| i + n);
            blank(&mut out, i, end, true);
            i = end;
        } else if b[i..].starts_with(b"/*") {
            let end = source[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
            blank(&mut out, i, end, true);
            i = end;
        } else if !ident_before && (b[i] == b'r' || b[i..].starts_with(b"br")) && {
            let start = if b[i] == b'b' { i + 2 } else { i + 1 };
            let hashes = b[start..].iter().take_while(|&&c| c == b'#').count();
            b.get(start + hashes) == Some(&b'"')
        } {
            let start = if b[i] == b'b' { i + 2 } else { i + 1 };
            let hashes = b[start..].iter().take_while(|&&c| c == b'#').count();
            let close = format!("\"{}", "#".repeat(hashes));
            let body = start + hashes + 1;
            let end = source[body..].find(&close).map_or(b.len(), |n| body + n + close.len());
            blank(&mut out, i, end, !strings);
            i = end;
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            blank(&mut out, i, (j + 1).min(b.len()), !strings);
            i = j + 1;
        } else if b[i] == b'\'' {
            // A character literal ('a', '\n', '{', 'é'), not a lifetime ('a).
            let rest = &source[i + 1..];
            let len = if let Some(escaped) = rest.strip_prefix('\\') {
                escaped.find('\'').map(|n| n + 2)
            } else {
                rest.chars().next().map(char::len_utf8).filter(|&n| rest[n..].starts_with('\''))
            };
            match len {
                Some(len) => {
                    blank(&mut out, i, i + len + 2, true);
                    i += len + 2;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The position after the bracket matching the one at `open`.
fn matching(code: &[u8], open: usize) -> usize {
    let (left, right) = match code[open] {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        _ => (b'[', b']'),
    };
    let mut depth = 0usize;
    for (at, &c) in code.iter().enumerate().skip(open) {
        if c == left {
            depth += 1;
        } else if c == right {
            depth -= 1;
            if depth == 0 {
                return at + 1;
            }
        }
    }
    code.len()
}

/// A source file cut before its inline test module: its code (comments and literals blanked), its
/// code with string literals kept, and its name.
struct File {
    source: String,
    code: Vec<u8>,
    with_strings: Vec<u8>,
    name: String,
}

fn files() -> Vec<File> {
    let mut paths = Vec::new();
    rust_files(&engine_src(), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let mut source = std::fs::read_to_string(&path).unwrap();
            let inline = source
                .match_indices("#[cfg(test)]\nmod ")
                .map(|(at, _)| at)
                .find(|at| source[*at..].lines().nth(1).is_some_and(|line| line.trim_end().ends_with('{')));
            if let Some(at) = inline {
                source.truncate(at);
            }
            let code = blanked(&source, false);
            let with_strings = blanked(&source, true);
            File { source, code, with_strings, name: path.to_string_lossy().into_owned() }
        })
        .collect()
}

fn ident_at(code: &[u8], at: usize) -> &str {
    let end = code[at..].iter().position(|c| !(c.is_ascii_alphanumeric() || *c == b'_')).map_or(code.len(), |n| at + n);
    std::str::from_utf8(&code[at..end]).unwrap_or("")
}

/// What a piece of code reads directly, the parameters it reads as handles, the free functions it
/// calls, and the commands it runs.
#[derive(Default, Clone)]
struct Reach {
    reads: BTreeSet<&'static str>,
    ids: BTreeSet<String>,
    calls: BTreeSet<String>,
    runs: BTreeSet<String>,
}

/// The string arguments of each call of `helper` in `text` (its code with strings kept).
fn string_args(text: &str, code: &[u8], helper: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices(helper) {
        if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
            continue;
        }
        let open = at + helper.len() - 1;
        let close = matching(code, open);
        let args = &text[open + 1..close.saturating_sub(1).max(open + 1)];
        for piece in args.split(',') {
            if let Some(key) = piece.trim().strip_prefix('"').and_then(|k| k.strip_suffix('"')) {
                out.push(key.to_owned());
            }
        }
    }
    out
}

fn reach(code: &[u8], with_strings: &[u8]) -> Reach {
    let text = std::str::from_utf8(code).unwrap_or("");
    let strings = std::str::from_utf8(with_strings).unwrap_or("");
    let mut found = Reach::default();
    let has = |token: &str| {
        text.match_indices(token).any(|(at, _)| {
            let end = at + token.len();
            let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
            if token.as_bytes().first().is_some_and(|c| ident(*c)) && at > 0 && ident(code[at - 1]) {
                return false;
            }
            let next = code.get(end).copied().unwrap_or(b' ');
            let whole = token.ends_with('(') || token.ends_with(')') || !(next.is_ascii_alphanumeric() || next == b'_');
            // Setting it (`st.space = sp`, `st.selection.clear()`) is no read.
            let rest = text[end..].trim_start();
            let written = (rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>"))
                || [".clear()", ".push(", ".retain(", ".extend(", ".truncate(", ".dedup(", ".take()"].iter().any(|write| rest.starts_with(write));
            whole && !written
        })
    };
    if SELECTION.iter().any(|token| has(token)) {
        found.reads.insert("selection");
    }
    if PART.iter().any(|token| has(token)) {
        found.reads.insert("part");
    }
    if DRAWS.iter().any(|token| has(token)) {
        found.reads.insert("active");
        found.reads.insert("draws");
    }
    // `add_entity` adds to the space being edited.
    if has("add_entity(") {
        found.reads.insert("part");
    }
    let reads_current = CURRENT.iter().any(|var| {
        ["str(", "i64(", "f64(", "get("].iter().any(|read| {
            strings
                .match_indices(&format!("{read}\"{var}\""))
                .any(|(at, _)| at == 0 || !with_strings.get(at - 1).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_'))
        })
    });
    if reads_current {
        found.reads.insert("active");
    }
    found.ids.extend(HANDLE_HELPERS.iter().flat_map(|helper| string_args(strings, code, helper)));
    if has("targets(") {
        found.ids.insert("handles".to_owned());
        found.ids.insert("handle".to_owned());
    }
    for at in 0..code.len() {
        let starts =
            (code[at].is_ascii_alphabetic() || code[at] == b'_') && (at == 0 || !(code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_'));
        if !starts {
            continue;
        }
        let name = ident_at(code, at);
        let after = at + name.len();
        let method = at > 0 && code[at - 1] == b'.';
        if code.get(after) == Some(&b'(') && !method && in_engine(code, at) {
            // `sysvars::get(…)` is the `get` of `sysvars.rs`: kept as `sysvars::get`.
            let module = qualifier(code, at).filter(|m| !["crate", "super", "self", "Self", "cmd"].contains(m));
            found.calls.insert(module.map_or_else(|| name.to_owned(), |m| format!("{m}::{name}")));
        }
        // `s.execute("zoom", …)`, or a command found by id (`find_command("arrayrect")`): the
        // command runs another, and reads what it reads.
        if matches!(name, "find_command" | "execute") && code.get(after) == Some(&b'(') {
            let rest = strings[after + 1..].trim_start();
            if let Some(stripped) = rest.strip_prefix('"')
                && let Some(end) = stripped.find('"')
            {
                found.runs.insert(stripped[..end].to_owned());
            }
        }
    }
    found
}

/// Setters share a suffix with the header readers; only a complete reader name is a read.
#[test]
fn setting_current_properties_does_not_read_them() {
    for source in [r#"d.header.set_str("CLAYER", "Walls");"#, r#"d.header.set_i64("CECOLOR", 1);"#, r#"d.header.set_f64("CELTSCALE", 2.0);"#] {
        let found = reach(&blanked(source, false), &blanked(source, true));
        assert!(!found.reads.contains("active"), "setter is not a reader: {source}");
    }
    for source in
        [r#"d.header.str("CLAYER", "0");"#, r#"d.header.i64("CECOLOR", 256);"#, r#"d.header.f64("CELTSCALE", 1.0);"#, r#"d.header.get("TEXTSTYLE");"#]
    {
        let found = reach(&blanked(source, false), &blanked(source, true));
        assert!(found.reads.contains("active"), "reader must stay detected: {source}");
    }
}

/// Looking up one command does not invoke other command names used as ordinary string data.
#[test]
fn command_lookup_reads_only_its_command_argument() {
    let source = r#"find_command("layer.set"); let table = "block"; find_command("style.rename");"#;
    let found = reach(&blanked(source, false), &blanked(source, true));
    assert_eq!(found.runs, BTreeSet::from(["layer.set".to_owned(), "style.rename".to_owned()]));
}

/// The engine's modules: its files' and folders' names.
fn modules() -> &'static BTreeSet<String> {
    static MODULES: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    MODULES.get_or_init(|| {
        fn walk(dir: &Path, out: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if let Some(stem) = path.file_stem() {
                    out.insert(stem.to_string_lossy().into_owned());
                }
                if path.is_dir() {
                    walk(&path, out);
                }
            }
        }
        let mut out: BTreeSet<String> = ["crate", "super", "self", "Self", "cmd"].into_iter().map(str::to_owned).collect();
        walk(&engine_src(), &mut out);
        out
    })
}

/// Whether the call at `at` is to the engine's own code: unqualified, or through `crate`, `super`,
/// `self`, `Self` or one of its modules; not `Arc::new` or another crate's.
fn in_engine(code: &[u8], at: usize) -> bool {
    let mut start = at;
    while start >= 2 && &code[start - 2..start] == b"::" {
        let mut from = start - 2;
        while from > 0 && (code[from - 1].is_ascii_alphanumeric() || code[from - 1] == b'_') {
            from -= 1;
        }
        start = from;
    }
    start == at || modules().contains(ident_at(code, start))
}

/// The module right before the call at `at` (`sysvars` in `crate::sysvars::get(`), if any.
fn qualifier(code: &[u8], at: usize) -> Option<&str> {
    if at < 2 || &code[at - 2..at] != b"::" {
        return None;
    }
    let mut from = at - 2;
    while from > 0 && (code[from - 1].is_ascii_alphanumeric() || code[from - 1] == b'_') {
        from -= 1;
    }
    Some(ident_at(code, from))
}

/// Every `fn` body, by name (a name defined in several files keeps each, by file).
fn functions(files: &[File]) -> BTreeMap<String, Vec<(String, Reach)>> {
    let mut out: BTreeMap<String, Vec<(String, Reach)>> = BTreeMap::new();
    for file in files {
        let code = &file.code;
        let text = std::str::from_utf8(code).unwrap_or("");
        for (at, _) in text.match_indices("fn ") {
            if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
                continue;
            }
            let name = ident_at(code, at + 3).to_owned();
            if name.is_empty() {
                continue;
            }
            let Some(open) = code[at..].iter().position(|&c| c == b'(').map(|n| at + n) else { continue };
            let params_end = matching(code, open);
            let Some(brace) = code[params_end..].iter().position(|&c| c == b'{' || c == b';').map(|n| params_end + n) else { continue };
            if code[brace] == b';' {
                continue;
            }
            let end = matching(code, brace);
            let mut body =
                if PLUMBING.contains(&name.as_str()) { Reach::default() } else { reach(&code[brace..end], &file.with_strings[brace..end]) };
            // A call of a parameter (`get: fn(…)`) or of a local closure is no engine function.
            let own_names = std::str::from_utf8(&code[open..brace]).unwrap_or("").to_owned() + std::str::from_utf8(&code[brace..end]).unwrap_or("");
            body.calls.retain(|call| {
                !(own_names.contains(&format!("{call}: "))
                    || own_names.contains(&format!("let {call} = |"))
                    || own_names.contains(&format!("let mut {call} = |")))
            });
            out.entry(name).or_default().push((file.name.clone(), body));
        }
    }
    out
}

/// One command: its id, what its `run` and `enabled` code reach, and its file.
struct Spec {
    id: String,
    run: Reach,
    file: String,
}

fn split_args(code: &[u8], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut args = Vec::new();
    let (mut start, mut at) = (from, from);
    while at < to {
        match code[at] {
            b'(' | b'[' | b'{' => at = matching(code, at),
            b'|' => {
                // A closure's parameters `|s, p|` hold a comma.
                at += 1;
                while at < to && code[at] != b'|' {
                    at += 1;
                }
                at += 1;
            }
            b',' => {
                args.push((start, at));
                at += 1;
                start = at;
            }
            _ => at += 1,
        }
    }
    if code[start..to].iter().any(|c| !c.is_ascii_whitespace()) {
        args.push((start, to));
    }
    args
}

/// What a `run` or `enabled` argument reaches: a closure's body, or the function it names.
fn argument(file: &File, from: usize, to: usize) -> Reach {
    let mut found = reach(&file.code[from..to], &file.with_strings[from..to]);
    let name = std::str::from_utf8(&file.code[from..to]).unwrap_or("").trim();
    if !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b':') {
        let mut parts = name.rsplit("::");
        let last = parts.next().unwrap_or(name).to_owned();
        let module = parts.next().filter(|m| !["crate", "super", "self", "Self", "cmd"].contains(m));
        if SELECTION.iter().any(|token| token.trim_end_matches('(') == last) {
            found.reads.insert("selection");
        }
        found.calls.insert(module.map_or_else(|| last.clone(), |m| format!("{m}::{last}")));
    }
    found
}

fn specs(files: &[File]) -> Vec<Spec> {
    let mut out = Vec::new();
    for file in files {
        let code = &file.code;
        let text = std::str::from_utf8(code).unwrap_or("");
        for (at, _) in text.match_indices("CommandSpec::new(") {
            let open = at + "CommandSpec::new".len();
            let end = matching(code, open);
            let args = split_args(code, open + 1, end - 1);
            if args.len() < 3 {
                continue;
            }
            let id_source = &file.source[args[0].0..args[0].1];
            let Some(id) = id_source.split('"').nth(1) else { continue };
            let mut run = argument(file, args[2].0, args[2].1);
            // The builder chain: `.enabled(predicate)` is checked before `run`.
            let mut at = end;
            loop {
                let rest = code[at..].iter().position(|c| !c.is_ascii_whitespace()).map_or(code.len(), |n| at + n);
                if code.get(rest) != Some(&b'.') {
                    break;
                }
                let method = ident_at(code, rest + 1).to_owned();
                let open = rest + 1 + method.len();
                if code.get(open) != Some(&b'(') {
                    break;
                }
                let close = matching(code, open);
                if method == "enabled" {
                    let enabled = argument(file, open + 1, close - 1);
                    run.reads.extend(enabled.reads);
                    run.calls.extend(enabled.calls);
                }
                at = close;
            }
            out.push(Spec { id: id.to_owned(), run, file: file.name.clone() });
        }
        // The constraint commands made by `gc!` and `dc!`: `run_gc` and `run_dc`.
        for (mac, run) in [("gc!(", "run_gc"), ("dc!(", "run_dc")] {
            for (at, _) in text.match_indices(mac) {
                if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
                    continue;
                }
                let open = at + mac.len() - 1;
                let end = matching(code, open);
                let Some(id) = file.source[open..end].split('"').nth(1) else { continue };
                let mut reach = Reach::default();
                reach.calls.insert(run.to_owned());
                out.push(Spec { id: id.to_owned(), run: reach, file: file.name.clone() });
            }
        }
    }
    out
}

/// What a function reaches, to a fixed point over the calls: what it reads, the parameters it reads
/// as handles, and the commands it runs.
type Reached = (BTreeSet<&'static str>, BTreeSet<String>, BTreeSet<String>);

/// What every command reads and the parameters it reads as handles, through the functions and
/// commands it reaches.
fn derived() -> BTreeMap<String, Reached> {
    let files = files();
    let functions = functions(&files);
    let specs = specs(&files);
    let mut reached: BTreeMap<(String, String), Reached> = BTreeMap::new();
    for (name, defs) in &functions {
        for (file, body) in defs {
            reached.insert((name.clone(), file.clone()), (body.reads.clone(), body.ids.clone(), body.runs.clone()));
        }
    }
    // A name defined in several files is the caller's own file's when it has one, else every one.
    // A call through a module (`sysvars::get`) is that module's file's.
    let resolve = |call: &str, file: &str, reached: &BTreeMap<(String, String), Reached>| -> Reached {
        let (module, name) = call.rsplit_once("::").map_or((None, call), |(m, n)| (Some(m), n));
        let Some(defs) = functions.get(name) else { return Reached::default() };
        let in_module: Vec<_> = module
            .map(|m| defs.iter().filter(|(f, _)| Path::new(f).file_stem().is_some_and(|stem| stem == m) || f.contains(&format!("/{m}/"))).collect())
            .unwrap_or_default();
        let own: Vec<_> = defs.iter().filter(|(f, _)| f == file).collect();
        let chosen: Vec<_> = if !in_module.is_empty() {
            in_module
        } else if own.is_empty() {
            defs.iter().collect()
        } else {
            own
        };
        let mut out = Reached::default();
        for (f, _) in chosen {
            if let Some((reads, ids, runs)) = reached.get(&(name.to_owned(), f.clone())) {
                out.0.extend(reads.iter().copied());
                out.1.extend(ids.iter().cloned());
                out.2.extend(runs.iter().cloned());
            }
        }
        out
    };
    loop {
        let mut changed = false;
        for (name, defs) in &functions {
            for (file, body) in defs {
                let mut now = reached.get(&(name.clone(), file.clone())).cloned().unwrap_or_default();
                for call in body.calls.iter().filter(|call| *call != name) {
                    let (reads, ids, runs) = resolve(call, file, &reached);
                    now.0.extend(reads);
                    now.1.extend(ids);
                    now.2.extend(runs);
                }
                if reached.get(&(name.clone(), file.clone())) != Some(&now) {
                    reached.insert((name.clone(), file.clone()), now);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    if let Ok(wanted) = std::env::var("ACTS_FN") {
        for (file, body) in functions.get(&wanted).into_iter().flatten() {
            eprintln!("fn {wanted} in {file}: direct {:?} {:?}", body.reads, body.ids);
            for call in &body.calls {
                eprintln!("  calls {call}: {:?}", resolve(call, file, &reached));
            }
        }
    }
    let mut commands: BTreeMap<String, Reached> = BTreeMap::new();
    for spec in &specs {
        let mut now: Reached = (spec.run.reads.clone(), spec.run.ids.clone(), spec.run.runs.clone());
        for call in &spec.run.calls {
            let (reads, ids, runs) = resolve(call, &spec.file, &reached);
            now.0.extend(reads);
            now.1.extend(ids);
            now.2.extend(runs);
        }
        if std::env::var("ACTS_DEBUG").is_ok_and(|id| id == spec.id) {
            eprintln!("{} direct {:?}", spec.id, spec.run.reads);
            for call in &spec.run.calls {
                eprintln!("{} calls {call}: {:?}", spec.id, resolve(call, &spec.file, &reached));
            }
        }
        let entry = commands.entry(spec.id.clone()).or_default();
        entry.0.extend(now.0);
        entry.1.extend(now.1);
        entry.2.extend(now.2);
    }
    // Commands that run other commands read what those read (not their parameters).
    loop {
        let mut changed = false;
        for spec in &specs {
            let mut now = commands[&spec.id].clone();
            for other in now.2.clone().iter().filter(|other| **other != spec.id) {
                now.0.extend(
                    commands
                        .get(other)
                        .map(|(reads, ..)| reads.iter().copied().filter(|read| *read != "draws").collect::<BTreeSet<_>>())
                        .unwrap_or_default(),
                );
            }
            if commands[&spec.id] != now {
                commands.insert(spec.id.clone(), now);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    commands
}

/// The target parameter `ids` names, as `schema` takes it: `handles` when the code reads it (through
/// `targets`, which also takes `handle`), else the one handle parameter the code reads.
fn target_of(ids: &BTreeSet<String>, schema: Option<&serde_json::Value>) -> Option<&'static str> {
    let ok = |key: &str| schema.is_some_and(|schema| schema.get(key).is_some() && kition_kit::schema::check_targets(schema, key).is_ok());
    if ids.contains("handles") && ok("handles") {
        return Some("handles");
    }
    let named: Vec<&String> = ids.iter().filter(|key| ok(key)).collect();
    match named.as_slice() {
        [only] if only.as_str() == "handle" => Some("handle"),
        _ => None,
    }
}

#[test]
fn every_command_declares_the_editor_state_its_code_reads_and_its_target() {
    let derived = derived();
    let doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let commands = doc.commands();
    if std::env::var("ACTS_PRINT").is_ok() {
        for command in &commands {
            if let Some((reads, ids, _)) = derived.get(&command.id) {
                let own = command.params.as_deref().and_then(|p| kition_kit::schema::from_kit_params(p).ok()).map(|c| c.schema);
                let has_own = |key: &str| own.as_ref().is_some_and(|schema| schema.get(key).is_some());
                let place = reads.contains("draws") && !has_own("layer") && !has_own("layout");
                println!("ACTS {} {:?} {:?} place={place} {:?}", command.id, reads, target_of(ids, command.schema.as_ref()), ids);
            }
        }
    }
    let mut wrong = Vec::new();
    for command in &commands {
        let declared: BTreeSet<&'static str> = command.reads.iter().map(|state| state.as_str()).collect();
        let (expected, target, draws): (BTreeSet<&'static str>, Option<&str>, bool) = match derived.get(&command.id) {
            Some((reads, ids, _)) => {
                let draws = reads.contains("draws");
                (reads.iter().copied().filter(|read| *read != "draws").collect(), target_of(ids, command.schema.as_ref()), draws)
            }
            None => {
                let Some((_, state, target)) = kition_cad_kit::acts::BINDING_COMMANDS.iter().find(|(id, ..)| *id == command.id) else {
                    wrong.push(format!("{}: neither an engine command the scan found nor one acts::BINDING_COMMANDS declares", command.id));
                    continue;
                };
                (state.iter().map(|state| state.as_str()).collect(), *target, false)
            }
        };
        if declared != expected {
            wrong.push(format!("{}: declares {declared:?}, its code reads {expected:?}", command.id));
        }
        // [corrected C8-cad-pin: constraints.rs:801-810,871-916 compares raw
        // handles with saved references/exceptions and never resolves entities.
        // targets() reads selection syntax here, not an Entity address.]
        let target = if matches!(command.id.as_str(), "delconstraint" | "constraintbar" | "dcdisplay") {
            assert_eq!(target, Some("handles"), "the native opaque list is still scanned");
            let source = std::fs::read_to_string(engine_src().join("cmd/constraints.rs")).unwrap();
            assert!(source.contains("c.refs.iter().any(|r| hs.contains(&r.handle))"));
            assert!(source.contains("exceptions.retain(|x| x != h)"));
            let code = blanked(&source, false);
            let text = std::str::from_utf8(&code).unwrap();
            for method in ["run_delconstraint", "bar_cmd", "set_bars"] {
                let start = text.find(&format!("fn {method}(")).unwrap();
                let open = start + code[start..].iter().position(|b| *b == b'{').unwrap();
                let end = matching(&code, open);
                assert!(!text[open..end].contains("entity("), "{method} must keep its handle membership opaque");
            }
            None
        } else {
            target
        };
        if command.targets != target {
            wrong.push(format!("{}: declares targets {:?}, its code reads {target:?} as handles", command.id, command.targets));
        }
        let own = command.params.as_deref().and_then(|p| kition_kit::schema::from_kit_params(p).ok()).map(|c| c.schema);
        let has_own = |key: &str| own.as_ref().is_some_and(|schema| schema.get(key).is_some());
        let place = draws && !has_own("layer") && !has_own("layout");
        if kition_cad_kit::acts::takes_place(&command.id) != place {
            wrong.push(format!(
                "{}: acts::takes_place says {}, its code {} new objects",
                command.id,
                !place,
                if draws { "draws" } else { "draws no" }
            ));
        }
    }
    assert!(wrong.is_empty(), "{} commands declare other editor state or targets than their code:\n{}", wrong.len(), wrong.join("\n"));
    let listed: BTreeSet<&str> = commands.iter().map(|command| command.id.as_str()).collect();
    for (id, ..) in kition_cad_kit::acts::BINDING_COMMANDS {
        assert!(listed.contains(id), "acts::BINDING_COMMANDS names {id}, which the kit does not list");
    }
}

/// The parameter names an agent's `on` fills are what the host's step checker takes (kition-sdk's
/// `check_commands` checks `targets` against each schema), and a drawing command takes `layer` and
/// `layout` in its schema.
#[test]
fn the_published_commands_pass_the_sdks_checks() {
    let doc = CadDocument::create("Plan.dxf", 420, 297).unwrap();
    let commands = doc.commands();
    kition_kit::testing::check_commands(&commands, &[]).unwrap_or_else(|problems| panic!("{}", problems.join("\n")));
    let by_id = |id: &str| commands.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no {id}"));
    for id in ["erase", "move", "properties.set"] {
        assert_eq!(by_id(id).targets, Some("handles"), "{id}");
        assert!(by_id(id).reads.contains(&kition_kit::EditorState::Selection), "{id}");
    }
    for id in ["line", "circle", "rectang", "text", "dimlinear"] {
        let command = by_id(id);
        assert!(command.reads.contains(&kition_kit::EditorState::Active) && command.reads.contains(&kition_kit::EditorState::Part), "{id}");
        let schema = command.schema.as_ref().unwrap();
        assert_eq!((schema["layer"]["type"].as_str(), schema["layout"]["type"].as_str()), (Some("string"), Some("string")), "{id}");
    }
    assert!(by_id("file.render").reads.is_empty() && by_id("entities.measure").reads.is_empty());
}

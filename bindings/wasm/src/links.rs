//! The files a drawing links, as [`KitDocument::inspect`](kition_kit::KitDocument::inspect)
//! reports them (KHP A42): external references (blocks that name a drawing file, kind `embed`)
//! and raster images (an IMAGE's file, kind `media`).
//!
//! Links are data. The kit reads no linked file: an external reference draws nothing (the check
//! reports `cad.unresolved-xref`) and an image draws its frame. Whether a linked file is there is
//! asked of the host, and only for a path it may lend ([`lent_path`]): relative to the drawing,
//! inside the workspace. Anything else (an absolute, drive or UNC path, a `file:` or remote URL, a
//! backslash, a hidden part) is reported as not there without asking anyone, so resolving a link
//! reads nothing outside the workspace and opens no connection (KHP A29, A37).

use kition_cad_doc::{Drawing, EntityKind};
use serde_json::{Value, json};

/// At most this many links are reported (and asked of the host).
const MAX_LINKS: usize = 1000;

/// `path` as the host may lend it: relative to the drawing, with no scheme, drive, root, `~`,
/// backslash or hidden part (`.name`). `None` for anything else. The host resolves `..` and
/// refuses whatever leaves the workspace.
pub fn lent_path(path: &str) -> Option<&str> {
    let plain = !path.is_empty()
        && !path.starts_with(['/', '~'])
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| part == "." || part == ".." || !part.starts_with('.'));
    plain.then_some(path)
}

/// Whether the host lends a file at `target` (never asked for a path it may not lend).
fn lent(target: &str) -> Option<&str> {
    let path = lent_path(target)?;
    matches!(kition_kit::host::exists(path), Ok(true)).then_some(path)
}

/// The links of `d`: `[{id, kind, target, exists, path?}]`, external references by block
/// (`block:<name>`) then images by handle, at most [`MAX_LINKS`]. `target` is the path as the
/// drawing gives it, never an instruction; `path` the workspace path the host lends it at, when
/// it is there.
pub fn report(d: &Drawing) -> Vec<Value> {
    let mut out = Vec::new();
    let mut link = |id: String, kind: &str, target: &str| {
        if out.len() >= MAX_LINKS {
            return;
        }
        let path = lent(target);
        let mut entry = json!({ "id": id, "kind": kind, "target": target, "exists": path.is_some() });
        if let Some(path) = path {
            entry["path"] = json!(path);
        }
        out.push(entry);
    };
    for (name, block) in &d.blocks {
        if let Some(target) = block.xref_path.as_deref().filter(|target| !target.is_empty()) {
            link(format!("block:{}", d.record_id("block", name, 0).unwrap_or(name)), "embed", target);
        }
    }
    let layouts = d.layouts.iter().flat_map(|layout| layout.entities.iter());
    let blocks = d.blocks.values().flat_map(|block| block.entities.iter());
    for entity in d.model.iter().chain(layouts).chain(blocks) {
        if let EntityKind::Image(image) = &entity.kind
            && !image.path.is_empty()
        {
            link(entity.handle.hex(), "media", &image.path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_paths_inside_the_workspace_relative_to_the_drawing_are_lent() {
        for lent in ["site.dxf", "refs/site.dxf", "../refs/site.dxf", "./site.dxf"] {
            assert_eq!(lent_path(lent), Some(lent), "{lent}");
        }
        for refused in [
            "",
            "/etc/passwd",
            "~/site.dxf",
            "C:\\refs\\site.dxf",
            "C:/refs/site.dxf",
            "\\\\server\\share\\site.dxf",
            "//server/share/site.dxf",
            "file:///refs/site.dxf",
            "http://127.0.0.1:1/site.dxf",
            ".hidden/site.dxf",
            "refs/.site.dxf",
        ] {
            assert_eq!(lent_path(refused), None, "{refused}");
        }
    }
}

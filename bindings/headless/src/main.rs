//! `kition-cad`: the CAD kit for Kition's agent. The same document the in-page editor runs
//! (`kition-cad-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange.

const SANDBOXED: &[&str] = &[];

/// `inspect()`'s editor state (`kition_cad_kit::EDITOR_STATE`, JSON Pointers) is what an agent
/// does not read as the drawing's.
fn main() -> std::process::ExitCode {
    kition_headless::run_manifest_model::<kition_cad_kit::CadDocument>(include_str!("../../../kit.json"), SANDBOXED, kition_cad_kit::EDITOR_STATE)
}

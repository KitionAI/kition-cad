//! `kition-cad`: the CAD kit for Kition's agent. The same document the in-page editor runs
//! (`kition-cad-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange, and the identity a file
//! carries (`document.identity`, `document.fork`): a `.dxf` is the drawing alone and embeds no
//! replica (the kit makes one under a new id each time it opens a file), so a file carries no
//! identity and a fork is the same bytes.

use kition_headless::{FileIdentity, Headless, KitInfo};

const SANDBOXED: &[&str] = &[];

/// `inspect()`'s editor state (`kition_cad_kit::EDITOR_STATE`, JSON Pointers) is what an agent
/// does not read as the drawing's.
fn main() -> std::process::ExitCode {
    let kit = match KitInfo::from_manifest(include_str!("../../../kit.json"), SANDBOXED, kition_cad_kit::EDITOR_STATE) {
        Ok(kit) => kit,
        Err(error) => {
            eprintln!("kit: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let backend = Headless::<kition_cad_kit::CadDocument>::with_replica_port(kit.clone()).with_file_identity(FileIdentity::NotEmbedded);
    kition_headless::serve_backend(&kit, backend)
}

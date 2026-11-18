//! The parameters of this kit's commands that name files or folders on disk (KHP A28), declared
//! as typed files in the commands' schemas (`kition_kit::files`): a host lends them, and a
//! headless kit keeps those commands from agents (the host opens, saves and exports documents).

use kition_kit::files::FileParam;

/// Every file or folder parameter of the kit's commands.
pub const FILES: &[FileParam] = &[
    FileParam::read("open", "path", &[]),
    FileParam::write("qsave", "path", &[]),
    FileParam::create("saveas", "path", &[]),
    FileParam::read("recover", "path", &[]),
    FileParam::read("insert", "file", &[]),
    FileParam::create("wblock", "path", &[]),
    FileParam::create("plot", "path", &["pdf"]),
    FileParam::create("exportpdf", "path", &["pdf"]),
    // `load` reads the plot style table, `save` writes it.
    FileParam::write("stylesmanager", "path", &["ctb", "stb"]),
];

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;

    use super::*;

    /// Parameters whose names read like files but are not: ids, property paths, directions,
    /// folders of the kit's own panels, in-memory data. ARRAY and ARRAYPATH use `path` for the
    /// handle of the curve the copies follow.
    const NOT_FILES: &[(&str, &str)] = &[("arraypath", "path"), ("array", "path")];

    #[test]
    fn every_file_parameter_is_declared() {
        let doc = crate::CadDocument::create("Untitled.dxf", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let problems = kition_kit::files::check(&doc.commands(), FILES, NOT_FILES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}

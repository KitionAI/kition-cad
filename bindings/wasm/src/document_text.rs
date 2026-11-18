//! `document.text` (KitText 1.0, kition-sdk `docs/platform/sets/text.md`): the drawing's words
//! as plain text, so a host can index them for search. A drawing's words live in
//! its text entities, outside the object model a host reads.
//!
//! Model space first, then each layout in tab order, each one part labelled with its tab's name.
//! Within a space, entities in drawing order: `TEXT`, `MTEXT` (formatting codes stripped), the
//! text of multileaders, table cells (a row per line, cells separated by tabs), dimension text
//! overrides (as the dimension shows them, with `<>` measured), and for each block reference the
//! text of its block (once per block) and its visible attribute values. `%%` codes become their
//! characters. Entities that are invisible or on layers that are off or frozen are left out, as
//! are attribute definitions. Reading changes nothing.

use std::collections::HashSet;

use kition_cad_doc::{Drawing, Entity, EntityKind, EntityStore, Layer, MAX_BLOCK_DEPTH};
use kition_kit::text::{DocumentText, TextBuilder};

/// The label of model space's part.
pub const MODEL: &str = "Model";

/// `%%d`, `%%p`, `%%c` and the other `%%` codes as their characters, underline toggles dropped.
fn controls(text: &str) -> String {
    kition_cad_fonts::decode_controls(text).into_iter().map(|(character, _, _)| character).collect()
}

/// MTEXT contents as plain text.
fn mtext(contents: &str) -> String {
    controls(&kition_cad_fonts::plain_mtext(contents))
}

/// Whether an entity shows: visible itself, and its layer on and thawed. Inside a block, an
/// entity on layer 0 takes the reference's layer, which was already checked.
fn shows(d: &Drawing, e: &Entity, in_block: bool) -> bool {
    if !e.common.visible {
        return false;
    }
    if in_block && e.common.layer == "0" {
        return true;
    }
    d.layer(&e.common.layer).is_none_or(Layer::visible)
}

struct Reader<'a> {
    d: &'a Drawing,
    /// The blocks whose text was read already (each once per drawing).
    blocks: HashSet<String>,
    /// The current space's blocks of words.
    words: Vec<String>,
}

impl Reader<'_> {
    fn space(&mut self, store: &EntityStore, depth: usize) {
        for e in store.iter() {
            if shows(self.d, e, depth > 0) {
                self.entity(e, depth);
            }
        }
    }

    fn block(&mut self, words: &str) {
        let words = words.replace("\r\n", "\n").replace('\r', "\n");
        if !words.trim().is_empty() {
            self.words.push(words);
        }
    }

    fn entity(&mut self, e: &Entity, depth: usize) {
        match &e.kind {
            EntityKind::Text(text) => self.block(&controls(&text.value)),
            EntityKind::MText(text) => self.block(&mtext(&text.contents)),
            EntityKind::MLeader(leader) => {
                if let Some(text) = &leader.text {
                    self.block(&mtext(&text.contents));
                }
            }
            EntityKind::Table(table) => {
                let rows: Vec<String> = table
                    .cells
                    .iter()
                    .map(|row| row.iter().map(|cell| mtext(&cell.text)).filter(|cell| !cell.trim().is_empty()).collect::<Vec<_>>().join("\t"))
                    .filter(|row| !row.is_empty())
                    .collect();
                self.block(&rows.join("\n"));
            }
            // Only a typed-over dimension text is words; a measured value alone is a number.
            EntityKind::Dimension(dimension) if !dimension.text.trim().is_empty() => {
                self.block(&kition_cad_render::dimension_in(self.d, dimension).value);
            }
            EntityKind::Insert(insert) => {
                if depth < MAX_BLOCK_DEPTH
                    && let Some(block) = self.d.block(&insert.block)
                    && !block.anonymous
                    && block.xref_path.is_none()
                    && self.blocks.insert(block.name.to_ascii_uppercase())
                {
                    self.space(&block.entities, depth + 1);
                }
                for attribute in insert.attribs.iter().filter(|attribute| !attribute.invisible) {
                    self.block(&controls(&attribute.text.value));
                }
            }
            _ => {}
        }
    }
}

/// The drawing's words, space by space (see the module docs), at most `limit` characters.
pub fn document_text(d: &Drawing, limit: usize) -> DocumentText {
    let mut reader = Reader { d, blocks: HashSet::new(), words: Vec::new() };
    let mut layouts: Vec<_> = d.layouts.iter().collect();
    layouts.sort_by_key(|layout| layout.tab_order);
    let spaces = std::iter::once((MODEL, &d.model)).chain(layouts.into_iter().map(|layout| (layout.name.as_str(), &layout.entities)));
    let mut out = TextBuilder::new(limit);
    for (label, store) in spaces {
        reader.space(store, 0);
        let words = std::mem::take(&mut reader.words);
        if !words.is_empty() && !out.part(label, &words.join("\n")) {
            break;
        }
    }
    out.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_become_characters() {
        assert_eq!(controls("%%c10 %%p0.1 45%%d %%uX%%u 5%%%"), "⌀10 ±0.1 45° X 5%");
        assert_eq!(mtext("{\\fArial|b1;NOTES:}\\P1. \\S1/2; %%c3"), "NOTES:\n1. 1/2 ⌀3");
    }
}

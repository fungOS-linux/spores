//! Reading a Word file's paragraphs from its own bytes.
//!
//! A `.docx` is a zip whose `word/document.xml` is Word's own outline. Two
//! facts make it a document rather than a blob: paragraphs carry an identity
//! (`w14:paraId`, a 32-bit hex number Word maintains), and each carries a style
//! name that says what it is.
//!
//! Nothing here needs a database. The identity is in the file, so a reader can
//! be a guest: same bytes, same blocks, same anchors, with no store between two
//! readers that have to agree.

use std::io::{Cursor, Read};

use anyhow::{Context as _, Result};
use quick_xml::Reader;
use quick_xml::events::Event;

/// One paragraph, as the document layer sees it.
pub struct Block {
    /// `w14:paraId` in hex, or `#n` when Word did not write one.
    pub id: String,
    pub text: String,
    /// `Heading1`, `Quote`, or nothing at all.
    pub style: Option<String>,
    /// Whether the identity came from the file (Word) or from its position.
    pub identified: bool,
}

pub struct Document {
    pub blocks: Vec<Block>,
    pub identified: usize,
    pub synthetic: usize,
}

impl Document {
    pub fn headings(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| {
                block
                    .style
                    .as_deref()
                    .is_some_and(|style| style.starts_with("Heading"))
            })
            .count()
    }
}

/// Parse a `.docx` into its paragraphs.
pub fn read(bytes: &[u8]) -> Result<Document> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("not a zip container")?;
    let mut part = zip
        .by_name("word/document.xml")
        .context("no word/document.xml: this is a zip, but not a Word file")?;
    let mut xml = String::new();
    part.read_to_string(&mut xml)
        .context("reading document.xml")?;

    let blocks = paragraphs(&xml)?;
    let identified = blocks.iter().filter(|block| block.identified).count();
    Ok(Document {
        synthetic: blocks.len() - identified,
        identified,
        blocks,
    })
}

/// Walk `document.xml` and keep the paragraphs.
fn paragraphs(xml: &str) -> Result<Vec<Block>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut blocks: Vec<Block> = Vec::new();
    let mut open: Option<Block> = None;
    let mut in_text = false;

    loop {
        match reader.read_event().context("reading document.xml")? {
            Event::Start(event) | Event::Empty(event) => {
                match event.name().local_name().into_inner() {
                    "p" => {
                        let para = attr(&event, "paraId");
                        let identified = para.is_some();
                        open = Some(Block {
                            id: match para {
                                Some(hex) => hex,
                                None => format!("#{}", blocks.len()),
                            },
                            identified,
                            style: None,
                            text: String::new(),
                        });
                    }
                    "pStyle" => {
                        if let Some(block) = open.as_mut()
                            && let Some(style) = attr(&event, "val")
                        {
                            block.style = Some(style);
                        }
                    }
                    "t" => in_text = true,
                    _ => {}
                }
            }
            Event::Text(text) => {
                if in_text && let Some(block) = open.as_mut() {
                    block.text.push_str(&text.xml10_content());
                }
            }
            Event::End(event) => match event.name().local_name().into_inner() {
                "t" => in_text = false,
                "p" => {
                    if let Some(block) = open.take() {
                        blocks.push(block);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(blocks)
}

/// A namespaced attribute by its local name: `w14:paraId` is `paraId`.
fn attr(event: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for attribute in event.attributes().flatten() {
        if attribute.key.local_name().into_inner() == key {
            return Some(attribute.value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">
<w:body>{body}</w:body></w:document>"#
        )
    }

    #[test]
    fn paragraphs_are_blocks_by_their_own_ids() {
        let body = concat!(
            r#"<w:p w14:paraId="1A2B3C4D"><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Scope</w:t></w:r></w:p>"#,
            r#"<w:p w14:paraId="5E6F7A8B"><w:r><w:t>A Spore reads blocks.</w:t></w:r></w:p>"#,
        );
        let blocks = paragraphs(&document(body)).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Scope");
        assert_eq!(blocks[0].style.as_deref(), Some("Heading1"));
        assert_eq!(blocks[0].id, "1A2B3C4D");
        assert!(blocks[0].identified);
        assert_eq!(blocks[1].text, "A Spore reads blocks.");
    }

    #[test]
    fn a_paragraph_without_word_s_identity_says_so() {
        let blocks = paragraphs(&document(r#"<w:p><w:r><w:t>typed</w:t></w:r></w:p>"#)).unwrap();
        assert_eq!(blocks[0].id, "#0");
        assert!(!blocks[0].identified);
    }
}

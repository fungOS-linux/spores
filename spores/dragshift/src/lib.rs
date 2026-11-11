//! DragShift: convert and edit a file, as a Spore.
//!
//! The module owns what an operation *is*: which conversions and tools apply to
//! which kind of source, what the result should be called, and what the undo
//! receipt records. It owns no codec.
//!
//! That boundary is deliberate. A `wasm32-wasip2` guest can decode PNG, JPEG,
//! WebP, GIF, TIFF, BMP and ICO with pure-Rust crates, but it cannot decode HEIC
//! or AVIF (libheif/libavif are C) and it cannot rasterise a PDF (pdfium is C++).
//! Rather than offer those and fail at drop time, they are returned as
//! unavailable with a reason, so the ring can show what it will not do.

use std::fmt;

/// What a source file is, as far as this Spore is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Pdf,
    Word,
    Text,
    Unknown,
}

impl Kind {
    /// Classify by file name. Case-insensitive; an unrecognised extension is
    /// `Unknown` rather than an error, because a drop can be anything.
    pub fn of(name: &str) -> Self {
        let extension = extension_of(name).to_ascii_lowercase();
        match extension.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "tiff" | "tif" | "bmp" | "ico" | "heic"
            | "heif" | "avif" => Kind::Image,
            "pdf" => Kind::Pdf,
            "docx" => Kind::Word,
            "txt" | "md" | "markdown" | "csv" | "tsv" => Kind::Text,
            _ => Kind::Unknown,
        }
    }
}

/// The two halves of the menu. A tap toggles between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    Convert,
    Tools,
}

impl Ring {
    pub fn label(self) -> &'static str {
        match self {
            Ring::Convert => "Convert",
            Ring::Tools => "Tools",
        }
    }
}

/// One slice of a ring.
///
/// `reason` is present exactly when the slice cannot run. A slice that is
/// unavailable without a reason is a bug: the whole point of showing it is to
/// say why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slice {
    pub id: &'static str,
    pub label: &'static str,
    pub available: bool,
    pub reason: Option<&'static str>,
}

impl Slice {
    fn ready(id: &'static str, label: &'static str) -> Self {
        Slice {
            id,
            label,
            available: true,
            reason: None,
        }
    }

    fn blocked(id: &'static str, label: &'static str, reason: &'static str) -> Self {
        Slice {
            id,
            label,
            available: false,
            reason: Some(reason),
        }
    }
}

const NO_HOST_DECODER: &str = "this build has no decoder for it";
const NO_PDF_ENGINE: &str = "PDF rendering needs a host engine";
const NO_DOCUMENT_RENDERER: &str = "the Spore has no document renderer";

/// The slices offered for one source and one ring, in ring order.
///
/// The ring is a fixed arrangement rather than a filtered list, so a slice's
/// position carries muscle memory across sources that support the same verb.
pub fn slices(kind: Kind, ring: Ring) -> Vec<Slice> {
    match (kind, ring) {
        (Kind::Image, Ring::Convert) => vec![
            Slice::ready("png", "PNG"),
            Slice::ready("jpeg", "JPEG"),
            Slice::ready("webp", "WebP"),
            Slice::ready("tiff", "TIFF"),
            Slice::ready("bmp", "BMP"),
            Slice::ready("ico", "ICO"),
            Slice::ready("pdf", "PDF"),
            Slice::blocked("avif", "AVIF", NO_HOST_DECODER),
            Slice::blocked("heic", "HEIC", NO_HOST_DECODER),
        ],
        (Kind::Image, Ring::Tools) => vec![
            Slice::ready("compress", "Compress"),
            Slice::ready("resize", "Resize"),
            Slice::ready("crop", "Crop"),
            Slice::ready("rotate", "Rotate"),
            Slice::ready("annotate", "Annotate"),
            Slice::ready("redact", "Redact"),
            Slice::ready("background", "Add BG"),
            Slice::ready("metadata", "Metadata"),
        ],
        (Kind::Pdf, Ring::Convert) => vec![
            Slice::blocked("png", "PNG", NO_PDF_ENGINE),
            Slice::blocked("jpeg", "JPEG", NO_PDF_ENGINE),
            Slice::ready("txt", "TXT"),
            Slice::ready("docx", "DOCX"),
        ],
        (Kind::Pdf, Ring::Tools) => vec![
            Slice::ready("compress", "Compress"),
            Slice::ready("split", "Split"),
            Slice::ready("rotate", "Rotate"),
            Slice::ready("merge", "Merge"),
            Slice::ready("protect", "Protect"),
            Slice::ready("extract", "Extract images"),
            Slice::ready("metadata", "Metadata"),
        ],
        (Kind::Word, Ring::Convert) => vec![
            Slice::blocked("pdf", "PDF", NO_DOCUMENT_RENDERER),
            Slice::ready("txt", "TXT"),
        ],
        (Kind::Word, Ring::Tools) => Vec::new(),
        (Kind::Text, Ring::Convert) => {
            vec![Slice::ready("pdf", "PDF"), Slice::ready("docx", "DOCX")]
        }
        (Kind::Text, Ring::Tools) => Vec::new(),
        (Kind::Unknown, _) => Vec::new(),
    }
}

/// Where a result goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    /// Next to the source. The default, and what upstream does.
    Sibling,
    /// A `DragShift/` directory beside the source, so a burst of drops does not
    /// scatter files through a folder the person curated.
    Subfolder,
}

/// The name a result should take.
///
/// The original is never overwritten. When the target carries the same
/// extension as the source, a marker is inserted so the two can coexist; the
/// host still resolves collisions, because the Spore cannot see the directory.
pub fn output_name(source: &str, target_extension: &str, destination: Destination) -> String {
    let (directory, file_name) = split_name(source);
    let stem = stem_of(file_name);
    let current = extension_of(file_name).to_ascii_lowercase();
    let target = target_extension.trim_start_matches('.');

    let name = if current == target.to_ascii_lowercase() {
        format!("{stem}-converted.{target}")
    } else {
        format!("{stem}.{target}")
    };

    match destination {
        Destination::Sibling => format!("{directory}{name}"),
        Destination::Subfolder => format!("{directory}DragShift/{name}"),
    }
}

/// What an undo would remove.
///
/// A receipt survives a restart, so it names the written path and nothing else.
/// It is deliberately not a closure or a handle: the Spore has no authority to
/// act on its own record later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub written: Vec<String>,
    pub summary: String,
}

fn split_name(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(index) => path.split_at(index + 1),
        None => ("", path),
    }
}

fn stem_of(file_name: &str) -> &str {
    match file_name.rfind('.') {
        Some(0) | None => file_name,
        Some(index) => &file_name[..index],
    }
}

fn extension_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(0) | None => "",
        Some(index) => &name[index + 1..],
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Kind::Image => "image",
            Kind::Pdf => "pdf",
            Kind::Word => "word",
            Kind::Text => "text",
            Kind::Unknown => "unknown",
        };
        formatter.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_is_classified_by_extension_case_insensitively() {
        assert_eq!(Kind::of("a/photo.JPG"), Kind::Image);
        assert_eq!(Kind::of("scan.pdf"), Kind::Pdf);
        assert_eq!(Kind::of("memo.docx"), Kind::Word);
        assert_eq!(Kind::of("notes.md"), Kind::Text);
        assert_eq!(Kind::of("archive.zip"), Kind::Unknown);
        assert_eq!(Kind::of("no-extension"), Kind::Unknown);
    }

    #[test]
    fn every_unavailable_slice_says_why() {
        // The ring shows what it will not do; a silent dead slice is the failure
        // this asserts against.
        for kind in [Kind::Image, Kind::Pdf, Kind::Word, Kind::Text] {
            for ring in [Ring::Convert, Ring::Tools] {
                for slice in slices(kind, ring) {
                    assert_eq!(
                        slice.available,
                        slice.reason.is_none(),
                        "{} {} slice {} disagrees with itself",
                        kind,
                        ring.label(),
                        slice.id
                    );
                }
            }
        }
    }

    #[test]
    fn an_unknown_source_offers_nothing_rather_than_guessing() {
        assert!(slices(Kind::Unknown, Ring::Convert).is_empty());
        assert!(slices(Kind::Unknown, Ring::Tools).is_empty());
    }

    #[test]
    fn the_ring_keeps_its_order_across_a_family_of_sources() {
        let png = slices(Kind::Image, Ring::Convert);
        let jpeg = slices(Kind::Image, Ring::Convert);
        assert_eq!(png, jpeg);
        assert_eq!(png.first().map(|slice| slice.id), Some("png"));
    }

    #[test]
    fn a_result_is_written_beside_the_source_by_default() {
        assert_eq!(
            output_name("photos/a.jpg", "png", Destination::Sibling),
            "photos/a.png"
        );
    }

    #[test]
    fn converting_to_the_same_extension_never_collides_with_the_original() {
        assert_eq!(
            output_name("a/photo.png", "png", Destination::Sibling),
            "a/photo-converted.png"
        );
    }

    #[test]
    fn the_subfolder_destination_keeps_a_folder_clean() {
        assert_eq!(
            output_name("a/b/photo.jpg", "webp", Destination::Subfolder),
            "a/b/DragShift/photo.webp"
        );
    }

    #[test]
    fn a_dotfile_or_a_bare_name_is_not_mangled() {
        assert_eq!(
            output_name("notes", "pdf", Destination::Sibling),
            "notes.pdf"
        );
        assert_eq!(
            output_name(".hidden", "pdf", Destination::Sibling),
            ".hidden.pdf"
        );
    }
}

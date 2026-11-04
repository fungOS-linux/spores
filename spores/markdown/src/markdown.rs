//! A small, deterministic Markdown parser.
//!
//! It is deliberately a subset. Blocks and inline spans are typed so hosts
//! never re-parse the source, and so a renderer cannot disagree with the
//! editor about what the document says.

use crate::doc::Target;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Link { text: Vec<Inline>, target: Target },
    Image { alt: String, target: Target },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        text: Vec<Inline>,
    },
    Paragraph {
        text: Vec<Inline>,
    },
    Quote {
        text: Vec<Inline>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Inline>>,
    },
    /// A fenced block. Classification is a host concern; see [`HostFormat`].
    Fence {
        info: String,
        text: String,
    },
    Rule,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// Zero-based source line the block starts on.
    pub line: usize,
    pub block: Block,
}

pub fn parse_blocks(source: &str) -> Vec<Located> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();

        if let Some(info) = fence_open(trimmed) {
            let start = i;
            i += 1;
            let mut body = Vec::new();
            while i < lines.len() {
                if fence_open(lines[i].trim_start()).is_some() {
                    i += 1;
                    break;
                }
                body.push(lines[i]);
                i += 1;
            }
            out.push(Located {
                line: start,
                block: Block::Fence {
                    info: info.to_owned(),
                    text: body.join("\n"),
                },
            });
            continue;
        }

        if trimmed.is_empty() {
            out.push(Located {
                line: i,
                block: Block::Blank,
            });
            i += 1;
            continue;
        }

        if let Some((level, text)) = heading(trimmed) {
            out.push(Located {
                line: i,
                block: Block::Heading {
                    level,
                    text: parse_inline(text),
                },
            });
            i += 1;
            continue;
        }

        if is_rule(trimmed) {
            out.push(Located {
                line: i,
                block: Block::Rule,
            });
            i += 1;
            continue;
        }

        if let Some(first) = quote_text(trimmed) {
            let start = i;
            let mut parts = vec![first];
            i += 1;
            while i < lines.len() {
                match quote_text(lines[i].trim_start()) {
                    Some(text) => {
                        parts.push(text);
                        i += 1;
                    }
                    None => break,
                }
            }
            out.push(Located {
                line: start,
                block: Block::Quote {
                    text: parse_inline(&parts.join("\n")),
                },
            });
            continue;
        }

        if let Some((ordered, first)) = list_item(trimmed) {
            let start = i;
            let mut items = vec![first];
            i += 1;
            while i < lines.len() {
                match list_item(lines[i].trim_start()) {
                    Some((same, text)) if same == ordered => {
                        items.push(text);
                        i += 1;
                    }
                    _ => break,
                }
            }
            out.push(Located {
                line: start,
                block: Block::List {
                    ordered,
                    items: items.into_iter().map(parse_inline).collect(),
                },
            });
            continue;
        }

        let start = i;
        let mut parts = vec![trimmed];
        i += 1;
        while i < lines.len() {
            let next = lines[i].trim_start();
            if starts_block(next) {
                break;
            }
            parts.push(next);
            i += 1;
        }
        out.push(Located {
            line: start,
            block: Block::Paragraph {
                text: parse_inline(&parts.join(" ")),
            },
        });
    }

    out
}

fn starts_block(line: &str) -> bool {
    line.is_empty()
        || fence_open(line).is_some()
        || heading(line).is_some()
        || is_rule(line)
        || quote_text(line).is_some()
        || list_item(line).is_some()
}

fn fence_open(line: &str) -> Option<&str> {
    line.strip_prefix("```").map(str::trim)
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    rest.strip_prefix(' ')
        .map(|text| (hashes as u8, text.trim_end()))
}

fn is_rule(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, '-' | '*' | '_') {
        return false;
    }
    let mut count = 1;
    for c in chars {
        if c == first {
            count += 1;
        } else if c != ' ' {
            return false;
        }
    }
    count >= 3
}

fn quote_text(line: &str) -> Option<&str> {
    line.strip_prefix('>')
        .map(|rest| rest.strip_prefix(' ').unwrap_or(rest).trim_end())
}

fn list_item(line: &str) -> Option<(bool, &str)> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(text) = line.strip_prefix(marker) {
            return Some((false, text.trim_end()));
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    line.get(digits..)
        .and_then(|rest| rest.strip_prefix(". "))
        .map(|text| (true, text.trim_end()))
}

/// Parse the inline spans of a single block of text.
pub fn parse_inline(input: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;

    while i < input.len() {
        let rest = &input[i..];

        if let Some(escaped) = rest.strip_prefix('\\') {
            if let Some(c) = escaped.chars().next() {
                if c.is_ascii_punctuation() {
                    text.push(c);
                    i += 1 + c.len_utf8();
                    continue;
                }
            }
        }

        if rest.starts_with("**") {
            if let Some(end) = find_closing(rest, "**") {
                flush(&mut out, &mut text);
                out.push(Inline::Strong(parse_inline(&rest[2..2 + end])));
                i += 2 + end + 2;
                continue;
            }
        }

        if let Some(code) = rest.strip_prefix('`') {
            if let Some(end) = code.find('`') {
                flush(&mut out, &mut text);
                out.push(Inline::Code(code[..end].to_owned()));
                i += 1 + end + 1;
                continue;
            }
        }

        if rest.starts_with("![") || rest.starts_with('[') {
            if let Some((inline, consumed)) = parse_link_or_image(rest) {
                flush(&mut out, &mut text);
                out.push(inline);
                i += consumed;
                continue;
            }
        }

        if rest.starts_with('*') && !rest.starts_with("**") {
            if let Some(end) = rest[1..].find('*') {
                flush(&mut out, &mut text);
                out.push(Inline::Emphasis(parse_inline(&rest[1..1 + end])));
                i += 1 + end + 1;
                continue;
            }
        }

        let c = rest.chars().next().expect("non-empty rest");
        text.push(c);
        i += c.len_utf8();
    }

    flush(&mut out, &mut text);
    out
}

fn parse_link_or_image(rest: &str) -> Option<(Inline, usize)> {
    let is_image = rest.starts_with("![");
    let open = if is_image { 2 } else { 1 };
    let label_end = open + rest[open..].find(']')?;
    let after = rest.get(label_end + 1..)?;
    let inner = after.strip_prefix('(')?;
    let close = inner.find(')')?;

    // `![](target "title")` — the title is presentation, not identity.
    let raw = inner[..close].split_whitespace().next().unwrap_or("");
    let target = Target::parse(raw).ok()?;
    let label = &rest[open..label_end];

    let inline = if is_image {
        Inline::Image {
            alt: label.to_owned(),
            target,
        }
    } else {
        Inline::Link {
            text: parse_inline(label),
            target,
        }
    };

    Some((inline, label_end + 1 + close + 1))
}

fn find_closing(rest: &str, delimiter: &str) -> Option<usize> {
    rest[delimiter.len()..].find(delimiter)
}

fn flush(out: &mut Vec<Inline>, text: &mut String) {
    if !text.is_empty() {
        out.push(Inline::Text(std::mem::take(text)));
    }
}

/// Plain-text projection of a span tree. Used by the native and Canvas
/// renderers, and by tests that compare surfaces.
pub fn inline_plain(items: &[Inline], out: &mut String) {
    for item in items {
        match item {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(t) => {
                out.push('`');
                out.push_str(t);
                out.push('`');
            }
            Inline::Strong(c) => {
                out.push_str("**");
                inline_plain(c, out);
                out.push_str("**");
            }
            Inline::Emphasis(c) => {
                out.push('*');
                inline_plain(c, out);
                out.push('*');
            }
            Inline::Link { text, target } => {
                inline_plain(text, out);
                out.push_str(" <");
                out.push_str(&target.to_string());
                out.push('>');
            }
            Inline::Image { alt, target } => {
                out.push('!');
                out.push('[');
                out.push_str(alt);
                out.push_str("] <");
                out.push_str(&target.to_string());
                out.push('>');
            }
        }
    }
}

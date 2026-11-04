//! Documents, opaque identifiers, and the host-resolved reference model.
//!
//! A Spore never sees a path or a URL it can fetch. It sees a [`DocId`] and a
//! [`Target`]; the host decides what those mean and whether to resolve them.

use crate::Error;
use std::collections::BTreeMap;

/// An opaque document handle. Validation here is deliberately stricter than a
/// filesystem path: no separators, no traversal, no control characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocId(String);

impl DocId {
    pub fn parse(raw: &str) -> Result<Self, Error> {
        if raw.is_empty() || raw.len() > 128 {
            return Err(Error::Invalid);
        }
        if raw == "." || raw == ".." || raw.contains("..") {
            return Err(Error::Invalid);
        }
        if raw
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control() || c == ':')
        {
            return Err(Error::Invalid);
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for DocId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a Markdown link or image points.
///
/// Nothing in this enum grants the Spore the ability to read the target. The
/// view lists what the host must resolve; the host may refuse any of it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// `asset:sha256:<hex>` — content-addressed, resolved by the host.
    Asset { algorithm: String, digest: String },
    /// `attach:<id>` — an attachment named by this document.
    Attachment { id: String },
    /// `doc:<id>` — another document through the same VFS.
    Document { id: String },
    /// Any other scheme. Recorded, never resolved by the Spore.
    External { scheme: String, reference: String },
}

impl Target {
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let raw = raw.trim();
        if raw.is_empty() || raw.len() > 512 {
            return Err(Error::Invalid);
        }

        if let Some(rest) = raw.strip_prefix("asset:") {
            let (algorithm, digest) = rest.split_once(':').ok_or(Error::Invalid)?;
            let algorithm_ok = !algorithm.is_empty()
                && algorithm.len() <= 32
                && algorithm
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-');
            let digest_ok = !digest.is_empty()
                && digest.len() <= 256
                && digest.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
            if !algorithm_ok || !digest_ok {
                return Err(Error::Invalid);
            }
            return Ok(Self::Asset {
                algorithm: algorithm.to_owned(),
                digest: digest.to_owned(),
            });
        }

        if let Some(id) = raw.strip_prefix("attach:") {
            return Ok(Self::Attachment {
                id: DocId::parse(id)?.as_str().to_owned(),
            });
        }

        if let Some(id) = raw.strip_prefix("doc:") {
            return Ok(Self::Document {
                id: DocId::parse(id)?.as_str().to_owned(),
            });
        }

        // Bare paths and bare hosts are refused: a link must name a scheme so
        // the host can apply policy without guessing.
        let (scheme, reference) = raw.split_once(':').ok_or(Error::Invalid)?;
        if scheme.is_empty()
            || scheme.len() > 32
            || !scheme
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '+' || c == '-')
            || reference.is_empty()
        {
            return Err(Error::Invalid);
        }
        Ok(Self::External {
            scheme: scheme.to_owned(),
            reference: reference.to_owned(),
        })
    }

    /// True only for targets the Spore itself may fetch. There are none: the
    /// host owns every byte the view needs.
    pub fn spore_may_fetch(&self) -> bool {
        false
    }
}

impl core::fmt::Display for Target {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Asset { algorithm, digest } => write!(f, "asset:{algorithm}:{digest}"),
            Self::Attachment { id } => write!(f, "attach:{id}"),
            Self::Document { id } => write!(f, "doc:{id}"),
            Self::External { scheme, reference } => write!(f, "{scheme}:{reference}"),
        }
    }
}

/// The storage boundary. `fungos-vfs` implements this trait when it lands; the
/// in-memory implementation here keeps the boundary real in the meantime.
pub trait Vfs {
    type Error: core::fmt::Debug;

    fn load(&mut self, id: &DocId) -> Result<Option<String>, Self::Error>;
    fn store(&mut self, id: &DocId, source: &str) -> Result<(), Self::Error>;
}

/// A bounded in-process VFS. Not durable; it exists so tests and hosts can run
/// the spore without the real filesystem layer.
#[derive(Debug, Default)]
pub struct MemoryVfs {
    docs: BTreeMap<String, String>,
}

impl MemoryVfs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }
}

impl Vfs for MemoryVfs {
    type Error = Error;

    fn load(&mut self, id: &DocId) -> Result<Option<String>, Self::Error> {
        Ok(self.docs.get(id.as_str()).cloned())
    }

    fn store(&mut self, id: &DocId, source: &str) -> Result<(), Self::Error> {
        self.docs.insert(id.as_str().to_owned(), source.to_owned());
        Ok(())
    }
}

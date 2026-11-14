//! Pending comments on blocks, and nothing else yet.
//!
//! A comment is prose plus an anchor: the document and the block it was left on.
//! A thread is the comments sharing a block. Pending is the first state — it
//! has not been committed into anything, so the only copy is the record.
//!
//! The store is `soil:records` whatever the comment is attached to. Comments are
//! entities: they sync, get edited, get replied to, get resolved, and the book is
//! where entities live. Picking a store by *target* would put a note on a
//! document somewhere other than a note on a transcript, for no reason.
//!
//! Ordering is by the host's revision, not by a clock: `soil:time` is monotonic
//! per process and explicitly not comparable across restarts or devices, so it
//! cannot order two people's comments. The book can.

use serde::{Deserialize, Serialize};

/// One pending comment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    /// `author@block@n` — stable across replicas because it names the anchor.
    pub id: String,
    /// Workspace-relative path of the document it is on.
    pub target: String,
    /// The block id from the container (`w14:paraId`), not a position.
    pub block: String,
    pub author: String,
    pub body: String,
    pub resolved: bool,
}

/// The comments of one document, as this replica sees them.
pub struct Pending {
    target: String,
    author: String,
    comments: Vec<Comment>,
    /// The record revision each comment arrived at, when the host assigns one.
    revisions: Vec<u64>,
    /// The next in-memory revision. The harness build has no host to assign one.
    #[cfg(feature = "demo")]
    next: u64,
}

impl Pending {
    /// The comments left on one block, oldest first.
    pub fn for_block(&self, block: &str) -> Vec<&Comment> {
        self.comments
            .iter()
            .filter(|comment| comment.block == block)
            .collect()
    }

    /// The threads, one per block: the block, and whether anything on it is
    /// still open. A thread is the unit the reader resolves, so it is the unit
    /// the counts are about.
    pub fn threads(&self) -> Vec<(&str, bool)> {
        let mut threads: Vec<(&str, bool)> = Vec::new();
        for comment in &self.comments {
            match threads
                .iter_mut()
                .find(|(block, _)| *block == comment.block.as_str())
            {
                Some((_, open)) => *open |= !comment.resolved,
                None => threads.push((comment.block.as_str(), !comment.resolved)),
            }
        }
        threads
    }

    /// Say something on a block.
    ///
    /// The comment is only kept if the store took it: a comment that was typed
    /// and silently dropped is worse than one that was refused out loud.
    pub fn add(&mut self, block: &str, body: &str) -> Result<Comment, String> {
        self.refresh();
        let n = self
            .comments
            .iter()
            .filter(|comment| comment.block == block)
            .count()
            + 1;
        let comment = Comment {
            id: format!("{}@{block}@{n}", self.author),
            target: self.target.clone(),
            block: block.to_owned(),
            author: self.author.clone(),
            body: body.to_owned(),
            resolved: false,
        };
        self.put(&comment)?;
        self.comments.push(comment.clone());
        Ok(comment)
    }

    /// Mark every unresolved comment on a block resolved, or reopen them.
    pub fn set_resolved(&mut self, block: &str, resolved: bool) -> Result<(), String> {
        self.refresh();
        let mut changed = Vec::new();
        for comment in &mut self.comments {
            if comment.block == block && comment.resolved != resolved {
                comment.resolved = resolved;
                changed.push(comment.clone());
            }
        }
        for comment in changed {
            self.put(&comment)?;
        }
        Ok(())
    }
}

/// Records-backed: the shipped store.
#[cfg(not(feature = "demo"))]
mod store {
    use super::{Comment, Pending};
    use crate::soil::{principal, records};

    /// The collection every pending comment lives in.
    const COLLECTION: &str = "comment.pending";

    /// The body contract, so a reader of the book knows what it is looking at.
    const SCHEMA: &str = "doc.comment@1";

    pub fn open(target: &str) -> Pending {
        let author = match principal::principal::current() {
            Ok(identity) => identity.actor,
            // No attested identity: say so rather than inventing a name that
            // somebody else's replica would then have to reconcile.
            Err(_) => "unattested".to_owned(),
        };
        let mut pending = Pending {
            target: target.to_owned(),
            author,
            comments: Vec::new(),
            revisions: Vec::new(),
        };
        pending.refresh();
        pending
    }

    impl Pending {
        /// Re-read the book. Cheap because a document has few comments.
        pub(super) fn refresh(&mut self) {
            let (mut comments, mut revisions) = (Vec::new(), Vec::new());
            let mut cursor: Option<String> = None;
            loop {
                let Ok(page) = records::records::scan(COLLECTION, cursor.as_deref(), 200) else {
                    return;
                };
                for entry in &page.entries {
                    if entry.deleted {
                        continue;
                    }
                    let Ok(comment) = serde_json::from_slice::<Comment>(&entry.body) else {
                        continue;
                    };
                    if comment.target != self.target {
                        continue;
                    }
                    revisions.push(entry.revision);
                    comments.push(comment);
                }
                cursor = page.next;
                if cursor.is_none() {
                    break;
                }
            }
            // The host's revision is what orders comments across writers.
            let mut order: Vec<usize> = (0..comments.len()).collect();
            order.sort_by_key(|index| revisions[*index]);
            self.comments = order.iter().map(|index| comments[*index].clone()).collect();
            self.revisions = order.iter().map(|index| revisions[*index]).collect();
        }

        pub(super) fn put(&mut self, comment: &Comment) -> Result<(), String> {
            let body = serde_json::to_vec(comment).map_err(|error| error.to_string())?;
            let entry = records::records::Entry {
                key: records::records::Key {
                    collection: COLLECTION.to_owned(),
                    id: comment.id.clone(),
                },
                schema: SCHEMA.to_owned(),
                revision: 0,
                updated_at_ms: 0,
                origin: String::new(),
                deleted: false,
                body,
            };
            records::records::upsert(&entry)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
        }
    }
}

/// In-memory: the harness build, which links no records book.
#[cfg(feature = "demo")]
mod store {
    use super::{Comment, Pending};

    pub fn open(target: &str) -> Pending {
        // The harness seeds one document's threads, and only that document's:
        // opening a second file in the fixed vault has to look like a file with
        // no comments, not like a copy of the first one's.
        let seeded: &[(&str, &str, &str, bool)] = if target.ends_with("nda.docx") {
            &[
                (
                    "1A2B3C4D",
                    "studio",
                    "Headings are Word's, so the anchor survives a re-read.",
                    false,
                ),
                (
                    "1A2B3C4D",
                    "neo",
                    "And the reply stays under it, resolved or not.",
                    false,
                ),
                (
                    "7F0A1B2C",
                    "neo",
                    "Does the carve-out cover roadmaps yet?",
                    false,
                ),
                (
                    "A1B2C3D4",
                    "studio",
                    "Is two years the term this client agreed to?",
                    true,
                ),
                (
                    "A1B2C3D4",
                    "neo",
                    "Yes: confirmed by mail the same day.",
                    true,
                ),
            ]
        } else {
            // Three threads in the three states a thread can be in: open and
            // answered, open and alone, and resolved with a reply under it.
            &[]
        };
        // Ids number a comment within its own thread, the way `add` does, so a
        // seeded comment and a written one are the same kind of row.
        let mut numbers: Vec<(&str, usize)> = Vec::new();
        let comments: Vec<Comment> = seeded
            .iter()
            .map(|(block, author, body, resolved)| {
                let n = match numbers.iter_mut().find(|(seen, _)| seen == block) {
                    Some((_, n)) => {
                        *n += 1;
                        *n
                    }
                    None => {
                        numbers.push((block, 1));
                        1
                    }
                };
                Comment {
                    id: format!("{author}@{block}@{n}"),
                    target: target.to_owned(),
                    block: (*block).to_owned(),
                    author: (*author).to_owned(),
                    body: (*body).to_owned(),
                    resolved: *resolved,
                }
            })
            .collect();
        Pending {
            target: target.to_owned(),
            author: "you".to_owned(),
            revisions: (0..comments.len() as u64).collect(),
            next: comments.len() as u64,
            comments,
        }
    }

    impl Pending {
        pub(super) fn refresh(&mut self) {}

        pub(super) fn put(&mut self, _comment: &Comment) -> Result<(), String> {
            self.next += 1;
            self.revisions.push(self.next);
            Ok(())
        }
    }
}

pub use store::open;

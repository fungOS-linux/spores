//! Initial `soil:unibus` Spore: mail/chat metadata as records.
//!
//! This slice defines the record keys and schemas the Spore owns. The host
//! supplies `soil:unibus` frames and `soil:records` storage; the module never
//! sees a socket, a credential, or a raw file path.

pub const ACCOUNT_COLLECTION: &str = "account";
pub const INBOX_COLLECTION: &str = "inbox";
pub const CONVERSATION_COLLECTION: &str = "conversation";
pub const MAIL_COLLECTION: &str = "email.message";
pub const CHAT_COLLECTION: &str = "chat.message";
pub const CHAT_BODY_COLLECTION: &str = "chat.body";

pub const MAIL_SCHEMA: &str = "email.message.v1";
pub const CHAT_SCHEMA: &str = "chat.message.v1";
pub const CHAT_BODY_SCHEMA: &str = "chat.body.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxKind {
    /// Personal or role inbox.
    Inbox,
    /// Shared queue that needs assignment and a disposition.
    Triage,
    /// Customer support queue with status/SLA fields.
    Support,
}

impl InboxKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Triage => "triage",
            Self::Support => "support",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationStatus {
    New,
    Open,
    Pending,
    Resolved,
    Snoozed,
}

impl ConversationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Open => "open",
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Snoozed => "snoozed",
        }
    }

    pub fn is_closed(self) -> bool {
        matches!(self, Self::Resolved)
    }
}

/// Account and inbox identity are records, not adapter config. The adapter
/// owns the session; the Spore owns the user-facing triage state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountMeta {
    pub id: String,
    pub provider: String,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxMeta {
    pub id: String,
    pub account_id: String,
    pub kind: InboxKind,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationMeta {
    pub id: String,
    pub inbox_id: String,
    pub status: ConversationStatus,
    pub assignee: Option<String>,
    pub priority: u8,
    pub summary: String,
}

pub fn account_key(id: &str) -> String {
    id.to_owned()
}
pub fn inbox_key(account_id: &str, inbox_id: &str) -> String {
    format!("{account_id}/{inbox_id}")
}
pub fn conversation_key(inbox_id: &str, thread_id: &str) -> String {
    format!("{inbox_id}/{thread_id}")
}

/// One record the Spore will `upsert` through `soil:records`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub collection: &'static str,
    pub id: String,
    pub schema: &'static str,
    pub body: Vec<u8>,
}

pub fn mail_key(account_id: &str, mailbox: &str, uidvalidity: u64, uid: u64) -> String {
    format!("{account_id}/{mailbox}/{uidvalidity}/{uid}")
}

pub fn chat_key(account_id: &str, message_id: &str) -> String {
    format!("{account_id}/{message_id}")
}

/// Map a bus kind to the collection it belongs in. Unknown kinds are not
/// records; the Spore reports them and continues.
pub fn collection_for_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "email.message.v1" | "email.delta.v1" => Some(MAIL_COLLECTION),
        "chat.message.v1" | "chat.delta.v1" => Some(CHAT_COLLECTION),
        "chat.fetch.reply.v1" => Some(CHAT_BODY_COLLECTION),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_identity_is_uidvalidity_and_uid_not_message_id() {
        assert_eq!(mail_key("work", "INBOX", 42, 7), "work/INBOX/42/7");
        assert_ne!(
            mail_key("work", "INBOX", 42, 7),
            mail_key("work", "INBOX", 43, 7)
        );
    }

    #[test]
    fn triage_support_state_has_explicit_status_and_assignee() {
        let conversation = ConversationMeta {
            id: conversation_key("support", "thread-1"),
            inbox_id: "support".into(),
            status: ConversationStatus::Open,
            assignee: Some("avery".into()),
            priority: 2,
            summary: "needs a refund decision".into(),
        };
        assert_eq!(conversation.id, "support/thread-1");
        assert!(!conversation.status.is_closed());
        assert_eq!(ConversationStatus::Resolved.as_str(), "resolved");
        assert_eq!(InboxKind::Support.as_str(), "support");
        assert_eq!(inbox_key("mail-work", "triage"), "mail-work/triage");
    }

    #[test]
    fn only_known_kinds_become_records() {
        assert_eq!(collection_for_kind("email.delta.v1"), Some(MAIL_COLLECTION));
        assert_eq!(
            collection_for_kind("chat.message.v1"),
            Some(CHAT_COLLECTION)
        );
        assert_eq!(collection_for_kind("unibus.node_state"), None);
    }
}

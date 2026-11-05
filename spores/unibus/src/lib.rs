//! Initial `soil:unibus` Spore: mail/chat metadata as records.
//!
//! This slice defines the record keys and schemas the Spore owns. The host
//! supplies `soil:unibus` frames and `soil:records` storage; the module never
//! sees a socket, a credential, or a raw file path.

pub const MAIL_COLLECTION: &str = "email.message";
pub const CHAT_COLLECTION: &str = "chat.message";
pub const CHAT_BODY_COLLECTION: &str = "chat.body";

pub const MAIL_SCHEMA: &str = "email.message.v1";
pub const CHAT_SCHEMA: &str = "chat.message.v1";
pub const CHAT_BODY_SCHEMA: &str = "chat.body.v1";

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
    fn only_known_kinds_become_records() {
        assert_eq!(collection_for_kind("email.delta.v1"), Some(MAIL_COLLECTION));
        assert_eq!(
            collection_for_kind("chat.message.v1"),
            Some(CHAT_COLLECTION)
        );
        assert_eq!(collection_for_kind("unibus.node_state"), None);
    }
}

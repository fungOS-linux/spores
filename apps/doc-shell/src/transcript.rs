//! The agent pane's data, behind one trait.
//!
//! The pane watches an agent work. This milestone's source is the document
//! layer's session records — a conversation that already happened, read out of a
//! root. The next source will be a live `delta-acp` stream, which is the same
//! shape of information arriving over a different transport.
//!
//! So the view is written against [`TranscriptSource`] and never against the
//! records: it asks for choices, asks for one transcript, and renders. A live
//! source therefore needs no change in the view, and the view holds no record
//! types it could grow a dependency on.

use crate::doclayer::{Store, TurnView};

/// Which conversation a reader is looking at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChoice {
    pub harness: String,
    pub native_id: String,
    pub name: Option<String>,
    pub turns: usize,
    pub started_at_ms: i64,
}

impl SessionChoice {
    /// `harness/native-id`, the pair a record is filed under.
    pub fn key(&self) -> String {
        format!("{}/{}", self.harness, self.native_id)
    }

    /// The one-line label a header shows.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) if !name.trim().is_empty() => name.clone(),
            _ => self.key(),
        }
    }
}

/// One conversation, resolved to text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptView {
    pub title: String,
    /// Where it came from: harness, id, turn count, what could not be normalized.
    pub subtitle: String,
    pub turns: Vec<TurnView>,
    /// Lines the importer could not normalize, by reason. A transcript that
    /// silently dropped them would be a transcript that lies about coverage.
    pub unnormalized: Vec<(String, u64)>,
}

/// Somewhere a transcript can come from.
pub trait TranscriptSource {
    /// Every conversation available, oldest first.
    fn sessions(&self) -> Vec<SessionChoice>;

    /// One conversation, or why it could not be read.
    fn transcript(&self, choice: &SessionChoice) -> Result<TranscriptView, String>;
}

/// The document layer's session records.
pub struct VfsTranscripts {
    store: Store,
}

impl VfsTranscripts {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl TranscriptSource for VfsTranscripts {
    fn sessions(&self) -> Vec<SessionChoice> {
        self.store
            .sessions()
            .into_iter()
            .map(|summary| SessionChoice {
                harness: summary.harness,
                native_id: summary.native_id,
                name: summary.name,
                turns: summary.turns,
                started_at_ms: summary.started_at_ms,
            })
            .collect()
    }

    fn transcript(&self, choice: &SessionChoice) -> Result<TranscriptView, String> {
        let session = self
            .store
            .session(&choice.harness, &choice.native_id)
            .ok_or_else(|| format!("no session record for {}", choice.key()))?;
        let mut subtitle = format!(
            "{} · {} turns · {}",
            choice.key(),
            session.turns.len(),
            when(session.started_at_ms)
        );
        if !session.models.is_empty() {
            subtitle.push_str(&format!(" · {}", session.models.join(", ")));
        }
        if let Some(cwd) = session.cwd.as_deref() {
            subtitle.push_str(&format!(" · {cwd}"));
        }
        Ok(TranscriptView {
            title: session
                .name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| choice.key()),
            subtitle,
            turns: session
                .turns
                .iter()
                .map(|turn| TurnView::build(&self.store, turn))
                .collect(),
            unnormalized: session
                .unnormalized
                .into_iter()
                .collect::<Vec<(String, u64)>>(),
        })
    }
}

/// `2026-10-21 12:12` from a millisecond stamp, in UTC.
///
/// Hand-rolled because a date is not worth a dependency here, and because a
/// transcript's timestamps are read as "when", not as a local wall clock.
pub fn when(at_ms: i64) -> String {
    let seconds = at_ms.div_euclid(1000);
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (hour, minute) = (rest / 3600, (rest % 3600) / 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}Z")
}

/// Days since the Unix epoch to a civil date. Howard Hinnant's algorithm, kept
/// here because the alternative is a calendar crate for one line of output.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// A source with nothing in it, for a vault nobody has recorded a session in.
pub struct NoTranscripts;

impl TranscriptSource for NoTranscripts {
    fn sessions(&self) -> Vec<SessionChoice> {
        Vec::new()
    }

    fn transcript(&self, _: &SessionChoice) -> Result<TranscriptView, String> {
        Err("this source holds no conversations".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_millisecond_stamp_becomes_a_readable_utc_time() {
        // 2026-10-21T12:12:14.980Z, the stamp in the doc layer's own fixture.
        assert_eq!(when(1_792_584_734_980), "2026-10-21 12:12Z");
        assert_eq!(when(0), "1970-01-01 00:00Z");
    }

    #[test]
    fn a_choice_shows_its_name_when_it_has_one() {
        let named = SessionChoice {
            harness: "pi".into(),
            native_id: "s-1".into(),
            name: Some("prove-it".into()),
            turns: 4,
            started_at_ms: 0,
        };
        assert_eq!(named.label(), "prove-it");
        let unnamed = SessionChoice {
            name: Some("  ".into()),
            ..named.clone()
        };
        assert_eq!(unnamed.label(), "pi/s-1");
        assert_eq!(named.key(), "pi/s-1");
    }
}

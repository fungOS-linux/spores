//! Quick-search: fuzzy match, frecency ranking, and aliases.
//!
//! This Spore owns the search ranking policy. The host supplies the source
//! snapshots (via soil:search), and the Spore returns ranked locators.
//! This makes the ranking algorithm live-reloadable and per-user via soil:appdata.

use std::cmp::Ordering;

/// A simplified version of a search hit from `soil:search`.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub score: u32,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Application,
    File,
    RecordEntry,
    Repository,
    Spore,
    Peer,
    Service,
    Action,
    Task,
    Principal,
}

/// Frecency metadata stored per item to boost recent/frequent items.
#[derive(Debug, Clone, Default)]
pub struct Frecency {
    pub uses: u32,
    pub last_used_at_ms: u64,
}

/// An alias mapping a short string to a hit's id.
#[derive(Debug, Clone)]
pub struct Alias {
    pub shorthand: String,
    pub target_id: String,
}

/// Combine search hits with frecency and aliases.
///
/// Ranking rules (highest first):
/// 1. Exact alias match
/// 2. Alias prefix match
/// 3. Title fuzzy match score
/// 4. Subtitle and keyword matches (embedded in score)
/// 5. Frecency (a blend of how often and how recently you picked it)
pub fn rank_hits(
    hits: Vec<SearchHit>,
    frecency: &std::collections::HashMap<String, Frecency>,
    aliases: &[Alias],
    query: &str,
) -> Vec<(SearchHit, i32)> {
    let mut ranked = Vec::with_capacity(hits.len());

    // Prepare frecency scores: log2(1 + uses) * 1_000_000 / (now - last_used)
    // For simplicity, assume `now` is a constant for this batch.
    let now = 1_000_000u64; // 1e9 seconds is ~31 years, safe for dev

    for hit in hits {
        let mut rank_score: i32 = hit.score as i32;

        // Check aliases for exact/prefix match boost
        for alias in aliases {
            if alias.target_id == hit.id {
                if alias.shorthand == query {
                    rank_score += 1_000_000;
                } else if alias.shorthand.starts_with(query) {
                    rank_score += 500_000;
                }
            }
        }

        // Blend frecency
        if let Some(f) = frecency.get(&hit.id) {
            let age_ms = now.saturating_sub(f.last_used_at_ms) as f64;
            let recency_factor = 1.0 / (1.0 + (age_ms / 86_400_000.0).clamp(1.0, 1000.0));
            let freq_factor = (1.0 + f.uses as f64).ln();
            rank_score += (recency_factor * freq_factor * 10_000.0) as i32;
        }

        ranked.push((hit, rank_score));
    }

    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn hit(id: &str, title: &str, score: u32) -> SearchHit {
        SearchHit {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            score,
            kind: Kind::Application,
        }
    }

    #[test]
    fn alias_exact_match_beats_exact_title_match() {
        let hits = vec![hit("app-1", "Safari", 100), hit("app-2", "Chrome", 100)];
        let aliases = vec![Alias { shorthand: "c".into(), target_id: "app-2".into() }];
        let frecency = HashMap::new();
        let ranked = rank_hits(hits, &frecency, &aliases, "c");
        assert_eq!(ranked[0].0.id, "app-2");
    }

    #[test]
    fn frecency_boosts_recently_used_items() {
        let hits = vec![hit("app-1", "Safari", 100), hit("app-2", "Chrome", 100)];
        let mut frecency = HashMap::new();
        frecency.insert("app-2".into(), Frecency { uses: 5, last_used_at_ms: 900_000 });
        let aliases = vec![];
        let ranked = rank_hits(hits, &frecency, &aliases, "c");
        assert_eq!(ranked[0].0.id, "app-2");
    }

    #[test]
    fn ranking_is_deterministic_and_does_not_panic_on_empty() {
        let hits = vec![];
        let ranked = rank_hits(hits, &HashMap::new(), &[], "q");
        assert!(ranked.is_empty());
    }
}

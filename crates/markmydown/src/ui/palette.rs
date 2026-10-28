//! Quick-open fuzzy search.

use std::path::PathBuf;

/// A workspace document offered by quick open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub title: String,
    pub relative: String,
}

/// Quick-open state.
#[derive(Debug, Clone)]
pub struct QuickOpen {
    pub query: String,
    pub selected: usize,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
}

impl QuickOpen {
    pub fn new(entries: Vec<Entry>) -> Self {
        let filtered = (0..entries.len()).collect();
        Self {
            query: String::new(),
            selected: 0,
            entries,
            filtered,
        }
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }

        let length = self.filtered.len() as i32;
        let mut next = self.selected as i32 + delta;
        next = next.rem_euclid(length);
        self.selected = next as usize;
    }

    pub fn filtered(&self) -> impl Iterator<Item = &Entry> {
        self.filtered
            .iter()
            .filter_map(|index| self.entries.get(*index))
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.filtered
            .get(self.selected)
            .and_then(|index| self.entries.get(*index))
    }

    pub fn is_empty(&self) -> bool {
        self.filtered.is_empty()
    }

    fn refilter(&mut self) {
        let mut scored: Vec<(i32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                fuzzy_score(&self.query, &entry.title)
                    .or_else(|| fuzzy_score(&self.query, &entry.relative))
                    .map(|score| (score, index))
            })
            .collect();

        scored.sort_by(|(left_score, left_index), (right_score, right_index)| {
            right_score.cmp(left_score).then_with(|| {
                self.entries[*left_index]
                    .title
                    .cmp(&self.entries[*right_index].title)
            })
        });

        self.filtered = scored.into_iter().map(|(_, index)| index).collect();
        self.selected = 0;
    }
}

/// Case-insensitive subsequence scoring. Matching at the start of the string
/// or after a separator scores higher; consecutive matches score higher.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query: Vec<char> = query.to_lowercase().chars().collect();
    let candidate_lower = candidate.to_lowercase();
    let candidate_chars: Vec<char> = candidate_lower.chars().collect();

    let mut score = 0;
    let mut query_index = 0;
    let mut previous_match: Option<usize> = None;

    for (index, ch) in candidate_chars.iter().enumerate() {
        if query_index >= query.len() {
            break;
        }

        if *ch == query[query_index] {
            if index == 0 || !candidate_chars[index - 1].is_alphanumeric() {
                score += 12;
            }
            if previous_match == Some(index.saturating_sub(1)) {
                score += 8;
            }
            score += 2;
            previous_match = Some(index);
            query_index += 1;
        }
    }

    if query_index == query.len() {
        Some(score)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry {
                path: PathBuf::from("/w/readme.md"),
                title: String::from("readme"),
                relative: String::from("readme.md"),
            },
            Entry {
                path: PathBuf::from("/w/notes/rust.md"),
                title: String::from("rust"),
                relative: String::from("notes/rust.md"),
            },
            Entry {
                path: PathBuf::from("/w/notes/roadmap.md"),
                title: String::from("roadmap"),
                relative: String::from("notes/roadmap.md"),
            },
        ]
    }

    #[test]
    fn fuzzy_matching_prefers_prefixes_and_subsequences() {
        assert!(fuzzy_score("read", "readme").is_some());
        assert!(fuzzy_score("rdm", "roadmap").is_some());
        assert!(fuzzy_score("xyz", "readme").is_none());

        let prefix = fuzzy_score("rust", "rust").expect("prefix");
        let scattered = fuzzy_score("rust", "roadmap notes rust").expect("scattered");
        assert!(prefix > scattered);
    }

    #[test]
    fn filtering_and_selection_wrap() {
        let mut quick = QuickOpen::new(entries());
        quick.set_query("r");

        assert_eq!(quick.filtered().count(), 3);
        quick.move_selection(-1);
        assert!(quick.selected_entry().is_some());
        quick.move_selection(1);
        assert!(quick.selected_entry().is_some());
    }

    #[test]
    fn empty_query_lists_everything() {
        let quick = QuickOpen::new(entries());
        assert_eq!(quick.filtered().count(), 3);
    }

    #[test]
    fn no_matches_is_empty() {
        let mut quick = QuickOpen::new(entries());
        quick.set_query("zzzz");

        assert!(quick.is_empty());
        assert!(quick.selected_entry().is_none());
    }
}

//! Render cache keyed by content hash.
//!
//! Math and Mermaid output is expensive and deterministic, so it is cached by
//! a hash of the source plus the presentation parameters. The cache is bounded
//! and evicts the oldest entry when full.

use std::collections::{HashMap, VecDeque};

/// A bounded content-addressed cache.
#[derive(Debug)]
pub struct ContentCache<T> {
    capacity: usize,
    entries: HashMap<String, T>,
    order: VecDeque<String>,
}

impl<T> ContentCache<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: String, value: T) {
        if let Some(existing) = self.entries.get_mut(&key) {
            *existing = value;
            return;
        }

        if self.entries.len() >= self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }

        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_retrieves_values() {
        let mut cache = ContentCache::new(2);
        cache.insert(String::from("a"), vec![1]);

        assert_eq!(cache.get("a"), Some(&vec![1]));
        assert!(cache.get("missing").is_none());
    }

    #[test]
    fn evicts_the_oldest_entry() {
        let mut cache = ContentCache::new(2);
        cache.insert(String::from("a"), 1);
        cache.insert(String::from("b"), 2);
        cache.insert(String::from("c"), 3);

        assert!(cache.get("a").is_none());
        assert_eq!(cache.get("b"), Some(&2));
        assert_eq!(cache.get("c"), Some(&3));
        assert_eq!(cache.len(), 2);
    }
}

//! Opening book: a prefix tree over UCI move sequences.
//!
//! Each node records, for every move played from that position, the child node
//! it leads to and how many times we have played it. Looking up a history walks
//! the tree move-by-move and returns the most-frequently-played continuation.

use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct OpeningNode {
    /// UCI move string -> (child node, times played from here).
    children: HashMap<String, (OpeningNode, u32)>,
}

impl OpeningNode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one game into the tree as a comma-separated UCI move string,
    /// e.g. `"e2e4,e7e5,g1f3"`. Empty/whitespace entries are ignored.
    pub fn insert(&mut self, moves: &str) {
        let mut node = self;
        for raw in moves.split(',') {
            let uci = raw.trim();
            if uci.is_empty() {
                continue;
            }
            let entry = node
                .children
                .entry(uci.to_string())
                .or_insert_with(|| (OpeningNode::default(), 0));
            entry.1 += 1;
            node = &mut entry.0;
        }
    }

    /// Look up the book continuation for a move history.
    ///
    /// Walks the tree along `history`, then samples one of the recorded
    /// continuations weighted by how often it was played. `temperature`
    /// controls the randomness:
    /// - `0.0` (or less): deterministic — always the most-played continuation.
    /// - `1.0`: sample directly proportional to play frequency.
    /// - higher: flatter distribution, mixing in rarer book moves.
    ///
    /// Returns `None` if the history leaves the book or the reached node has no
    /// recorded continuations.
    pub fn lookup(&self, history: &[&str], temperature: f32) -> Option<String> {
        let mut node = self;
        for m in history {
            match node.children.get(*m) {
                Some((child, _)) => node = child,
                None => return None,
            }
        }
        node.sample_continuation(temperature)
    }

    /// Sample one continuation from this node's children using `temperature`.
    fn sample_continuation(&self, temperature: f32) -> Option<String> {
        if self.children.is_empty() {
            return None;
        }

        // Deterministic: pick the most-played continuation.
        if temperature <= 0.0 {
            return self
                .children
                .iter()
                .max_by_key(|(_, (_, freq))| *freq)
                .map(|(mv, _)| mv.clone());
        }

        // Weight each continuation by freq^(1/temperature), then sample.
        let inv_t = 1.0 / temperature;
        let weighted: Vec<(&String, f32)> = self
            .children
            .iter()
            .map(|(mv, (_, freq))| (mv, (*freq as f32).powf(inv_t)))
            .collect();

        let total: f32 = weighted.iter().map(|(_, w)| w).sum();

        let mut rng_bytes = [0u8; 4];
        getrandom::getrandom(&mut rng_bytes).expect("rng failed");
        let threshold = u32::from_le_bytes(rng_bytes) as f32 / u32::MAX as f32 * total;

        let mut cumulative = 0.0;
        for (mv, w) in &weighted {
            cumulative += w;
            if cumulative >= threshold {
                return Some((*mv).clone());
            }
        }
        weighted.last().map(|(mv, _)| (*mv).clone())
    }

    pub fn count(&self, history: &[&str]) -> u32 {
        let mut node = self;

        for m in history {
            match node.children.get(*m) {
                Some((child, _)) => node = child,
                None => return 0,
            }
        }

        node.children.iter().map(|(_, child_node)| child_node.1).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_has_no_book_move() {
        let tree = OpeningNode::new();
        assert_eq!(tree.lookup(&[], 0.0), None);
        assert_eq!(tree.lookup(&["e2e4"], 0.0), None);
    }

    #[test]
    fn walks_tree_to_most_played_continuation() {
        let mut tree = OpeningNode::new();
        // Play 1.e4 e5 2.Nf3 twice, and 1.e4 c5 once.
        tree.insert("e2e4,e7e5,g1f3");
        tree.insert("e2e4,e7e5,g1f3");
        tree.insert("e2e4,c7c5");

        // Temperature 0.0 => deterministic, always the most-played move.
        // From the root, e4 is the only first move.
        assert_eq!(tree.lookup(&[], 0.0), Some("e2e4".to_string()));

        // After 1.e4, e5 (played twice) beats c5 (played once).
        assert_eq!(tree.lookup(&["e2e4"], 0.0), Some("e7e5".to_string()));

        // After 1.e4 e5, the booked reply is Nf3.
        assert_eq!(tree.lookup(&["e2e4", "e7e5"], 0.0), Some("g1f3".to_string()));
    }

    #[test]
    fn out_of_book_history_returns_none() {
        let mut tree = OpeningNode::new();
        tree.insert("e2e4,e7e5");

        // d4 was never played from the root.
        assert_eq!(tree.lookup(&["d2d4"], 0.0), None);
        // Diverging mid-line leaves the book.
        assert_eq!(tree.lookup(&["e2e4", "c7c5"], 0.0), None);
    }

    #[test]
    fn leaf_node_has_no_continuation() {
        let mut tree = OpeningNode::new();
        tree.insert("e2e4,e7e5");

        // We have reached the end of the only recorded line.
        assert_eq!(tree.lookup(&["e2e4", "e7e5"], 0.0), None);
    }

    #[test]
    fn insert_ignores_blank_segments() {
        let mut tree = OpeningNode::new();
        tree.insert(" e2e4 , , e7e5 ,");

        assert_eq!(tree.lookup(&[], 0.0), Some("e2e4".to_string()));
        assert_eq!(tree.lookup(&["e2e4"], 0.0), Some("e7e5".to_string()));
    }

    #[test]
    fn temperature_sampling_stays_within_book() {
        let mut tree = OpeningNode::new();
        tree.insert("e2e4,e7e5");
        tree.insert("e2e4,c7c5");
        tree.insert("d2d4,d7d5");

        // With a non-zero temperature, every sampled move must still be a real
        // recorded continuation from the root.
        for _ in 0..200 {
            let mv = tree.lookup(&[], 1.0).unwrap();
            assert!(mv == "e2e4" || mv == "d2d4", "unexpected move {mv}");
        }
    }

    #[test]
    fn temperature_sampling_can_pick_rarer_moves() {
        let mut tree = OpeningNode::new();
        // e4 played 3x, d4 played once.
        tree.insert("e2e4");
        tree.insert("e2e4");
        tree.insert("e2e4");
        tree.insert("d2d4");

        // A high temperature flattens the distribution, so over many samples
        // the rarer d4 should eventually appear.
        let saw_rare = (0..500).any(|_| tree.lookup(&[], 5.0) == Some("d2d4".to_string()));
        assert!(saw_rare, "high temperature never sampled the rarer move");
    }
}

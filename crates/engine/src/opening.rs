//! Opening book: a prefix tree over UCI move sequences.
//!
//! Each node records, for every move played from that position, the child node
//! it leads to and how many times we have played it. Looking up a history walks
//! the tree move-by-move and returns the recorded continuations together with
//! how often each has been seen.

use shakmaty::uci::UciMove;
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct OpeningNode {
    /// Move -> (child node, times played from here).
    children: HashMap<UciMove, (OpeningNode, u32)>,
}

impl OpeningNode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one game into the tree as a sequence of moves, e.g.
    /// `[e2e4, e7e5, g1f3]`.
    pub fn insert(&mut self, moves: &[UciMove]) {
        let mut node = self;
        for m in moves {
            let entry = node
                .children
                .entry(*m)
                .or_insert_with(|| (OpeningNode::default(), 0));
            entry.1 += 1;
            node = &mut entry.0;
        }
    }

    /// Walks the tree along `history` and returns every recorded continuation
    /// from the reached node together with how often it has been seen.
    ///
    /// Returns `None` if the history leaves the book or the reached node has no
    /// recorded continuations.
    pub fn lookup(&self, history: &[UciMove]) -> Option<HashMap<UciMove, u32>> {
        let mut node = self;

        for m in history {
            match node.children.get(m) {
                Some((child, _)) => node = child,
                None => return None,
            }
        }

        if node.children.is_empty() {
            return None;
        }

        Some(
            node.children
                .iter()
                .map(|(m, (_, freq))| (*m, *freq))
                .collect(),
        )
    }

    /// Total number of times any continuation has been played from the node
    /// reached by walking `history`.
    pub fn count(&self, history: &[UciMove]) -> u32 {
        let mut node = self;

        for m in history {
            match node.children.get(m) {
                Some((child, _)) => node = child,
                None => return 0,
            }
        }

        node.children.values().map(|(_, freq)| *freq).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a UCI move string in tests, panicking on malformed input.
    fn uci(s: &str) -> UciMove {
        s.parse().expect("valid UCI move")
    }

    /// Parse a comma-separated UCI line into a move list.
    fn line(s: &str) -> Vec<UciMove> {
        s.split(',').map(|t| uci(t.trim())).collect()
    }

    #[test]
    fn empty_tree_has_no_book_move() {
        let tree = OpeningNode::new();
        assert_eq!(tree.lookup(&[]), None);
        assert_eq!(tree.lookup(&[uci("e2e4")]), None);
    }

    #[test]
    fn walks_tree_to_recorded_continuations() {
        let mut tree = OpeningNode::new();
        // Play 1.e4 e5 2.Nf3 twice, and 1.e4 c5 once.
        tree.insert(&line("e2e4,e7e5,g1f3"));
        tree.insert(&line("e2e4,e7e5,g1f3"));
        tree.insert(&line("e2e4,c7c5"));

        // From the root, e4 is the only first move (played 3 times).
        assert_eq!(tree.lookup(&[]), Some(HashMap::from([(uci("e2e4"), 3)])));

        // After 1.e4: e5 (played twice) and c5 (played once).
        assert_eq!(
            tree.lookup(&[uci("e2e4")]),
            Some(HashMap::from([(uci("e7e5"), 2), (uci("c7c5"), 1)]))
        );

        // After 1.e4 e5, the booked reply is Nf3 (twice).
        assert_eq!(
            tree.lookup(&[uci("e2e4"), uci("e7e5")]),
            Some(HashMap::from([(uci("g1f3"), 2)]))
        );
    }

    #[test]
    fn out_of_book_history_returns_none() {
        let mut tree = OpeningNode::new();
        tree.insert(&line("e2e4,e7e5"));

        // d4 was never played from the root.
        assert_eq!(tree.lookup(&[uci("d2d4")]), None);
        // Diverging mid-line leaves the book.
        assert_eq!(tree.lookup(&[uci("e2e4"), uci("c7c5")]), None);
    }

    #[test]
    fn leaf_node_has_no_continuation() {
        let mut tree = OpeningNode::new();
        tree.insert(&line("e2e4,e7e5"));

        // We have reached the end of the only recorded line.
        assert_eq!(tree.lookup(&[uci("e2e4"), uci("e7e5")]), None);
    }

    #[test]
    fn count_sums_continuations_from_node() {
        let mut tree = OpeningNode::new();
        tree.insert(&line("e2e4,e7e5,g1f3"));
        tree.insert(&line("e2e4,e7e5,g1f3"));
        tree.insert(&line("e2e4,c7c5"));

        // Root: e4 played 3 times.
        assert_eq!(tree.count(&[]), 3);
        // After 1.e4: e5 (2) + c5 (1) = 3 continuations.
        assert_eq!(tree.count(&[uci("e2e4")]), 3);
        // After 1.e4 e5: Nf3 played twice.
        assert_eq!(tree.count(&[uci("e2e4"), uci("e7e5")]), 2);
        // Out of book.
        assert_eq!(tree.count(&[uci("d2d4")]), 0);
    }
}

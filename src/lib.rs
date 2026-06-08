mod heuristics;
mod opening;
mod utils;

#[cfg(not(target_arch = "wasm32"))]
pub mod pgn;

use std::collections::HashSet;

use shakmaty::uci::UciMove;
use shakmaty::zobrist::Zobrist64;
use shakmaty::{CastlingMode, Chess, EnPassantMode, Move, Position};
use wasm_bindgen::prelude::*;

use heuristics::{score_move, PersonalityWeights};
use opening::OpeningNode;

/// Replay a comma-separated UCI move history into a position.
///
/// Illegal/unparseable moves stop the replay early and return the position
/// reached so far, so a bad history never panics the bot at runtime.
fn replay_moves(move_history: &str) -> Chess {
    let mut pos = Chess::default();
    for raw in move_history.split(',') {
        let uci = raw.trim();
        if uci.is_empty() {
            continue;
        }
        let parsed = match uci.parse::<UciMove>() {
            Ok(u) => u,
            Err(_) => break,
        };
        let m = match parsed.to_move(&pos) {
            Ok(m) => m,
            Err(_) => break,
        };
        // `to_move` already validated the move against `pos`.
        pos.play_unchecked(m);
    }
    pos
}

/// Sample one move proportionally to its weight using browser-compatible RNG.
fn weighted_sample(moves: &[Move], weights: &[f32]) -> Move {
    let total: f32 = weights.iter().sum();

    let mut rng_bytes = [0u8; 4];
    getrandom::getrandom(&mut rng_bytes).expect("rng failed");
    let rand_val = u32::from_le_bytes(rng_bytes) as f32 / u32::MAX as f32;
    let threshold = rand_val * total;

    let mut cumulative = 0.0;
    for (m, &w) in moves.iter().zip(weights.iter()) {
        cumulative += w;
        if cumulative >= threshold {
            return *m;
        }
    }
    *moves.last().expect("called weighted_sample on no moves")
}

#[wasm_bindgen]
pub struct ChessBot {
    opening_tree: OpeningNode,
    seen_positions: HashSet<u64>,
    weights: PersonalityWeights,
}

impl Default for ChessBot {
    fn default() -> Self {
        Self {
            opening_tree: OpeningNode::new(),
            seen_positions: HashSet::new(),
            weights: PersonalityWeights::default(),
        }
    }
}

#[wasm_bindgen]
impl ChessBot {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ChessBot {
        utils::set_panic_hook();

        let mut bot = ChessBot::default();

        bot.load_games(include_str!("games.txt"));

        bot
    }

    /// Load game histories: one game per line, each a comma-separated UCI move
    /// string (e.g. `"e2e4,e7e5,g1f3"`). Feeds both the opening book and the
    /// seen-position set.
    fn load_games(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            self.opening_tree.insert(line);
            self.record_seen_positions(line);
        }
    }

    /// Adjust personality weights at runtime (wire these up to JS sliders).
    #[allow(clippy::too_many_arguments)]
    pub fn set_weights(
        &mut self,
        ladder_mate: f32,
        knight_bishop_trade: f32,
        knight_eyeing_bishop: f32,
        knight_fork: f32,
        knight_approaching_f6: f32,
        seen_position: f32,
        opener_temperature: f32,
    ) {
        self.weights = PersonalityWeights {
            ladder_mate,
            knight_bishop_trade,
            knight_eyeing_bishop,
            knight_fork,
            knight_approaching_f6,
            seen_position,
            opener_temperature,
        };
    }

    pub fn how_many_times_game_seen(&self, moves_played: &str) -> u32 {
        let history: Vec<&str> = if moves_played.is_empty() {
            Vec::new()
        } else {
            moves_played.split(',').map(str::trim).collect()
        };

        self.opening_tree.count(&history)
    }

    /// Main entry point. `moves_played` is the comma-separated UCI history.
    /// Returns a UCI move string, or an empty string if the game is over.
    pub fn get_move(&self, moves_played: &str) -> String {
        let history: Vec<&str> = if moves_played.is_empty() {
            Vec::new()
        } else {
            moves_played.split(',').map(str::trim).collect()
        };

        // 1. Opening book.
        if let Some(book) = self
            .opening_tree
            .lookup(&history, self.weights.opener_temperature)
        {
            return book;
        }

        // 2. Weighted-random selection with personality heuristics.
        let pos = replay_moves(moves_played);
        let legals = pos.legal_moves();
        if legals.is_empty() {
            return String::new();
        }

        let weights: Vec<f32> = legals
            .iter()
            .map(|m| {
                let after = pos.clone().play(*m).expect("legal move failed to play");
                1.0 + score_move(&pos, m, &after, &self.weights, &self.seen_positions)
            })
            .collect();

        let chosen = weighted_sample(&legals, &weights);
        UciMove::from_move(chosen, CastlingMode::Standard).to_string()
    }
}

impl ChessBot {
    /// Record the Zobrist hash of every position along a UCI move line.
    fn record_seen_positions(&mut self, line: &str) {
        let mut pos = Chess::default();
        self.seen_positions
            .insert(pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0);

        for raw in line.split(',') {
            let uci = raw.trim();
            if uci.is_empty() {
                continue;
            }
            let m = match uci.parse::<UciMove>().ok().and_then(|u| u.to_move(&pos).ok()) {
                Some(m) => m,
                None => break,
            };
            pos.play_unchecked(m);
            self.seen_positions
                .insert(pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_move_is_returned_from_history() {
        let mut bot = ChessBot::default();
        bot.load_games("e2e4,e7e5,g1f3\ne2e4,e7e5,g1f3\ne2e4,c7c5");

        // The most-played first move is e4.
        assert_eq!(bot.get_move(""), "e2e4");
        // After 1.e4, the booked reply is e5 (played twice vs c5 once).
        assert_eq!(bot.get_move("e2e4"), "e7e5");
        // After 1.e4 e5, the booked reply is Nf3.
        assert_eq!(bot.get_move("e2e4,e7e5"), "g1f3");
    }

    #[test]
    fn out_of_book_returns_a_legal_move() {
        let bot = ChessBot::default(); // empty book
        let mv = bot.get_move("");
        // No book, so it falls through to weighted sampling: must be a legal
        // opening move parseable as UCI.
        let parsed: UciMove = mv.parse().expect("returned a valid UCI move");
        assert!(parsed.to_move(&Chess::default()).is_ok());
    }

    #[test]
    fn terminal_position_returns_empty_string() {
        // Fool's mate: 1. f3 e5 2. g4 Qh4# — checkmate, no legal moves.
        let bot = ChessBot::default();
        assert_eq!(bot.get_move("f2f3,e7e5,g2g4,d8h4"), "");
    }
}

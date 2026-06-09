mod heuristics;
mod opening;

pub mod move_possibility;
#[cfg(not(target_arch = "wasm32"))]
pub mod pgn;

use heuristics::{
    ConsiderationScore, PersonalityConfig, PositionScore, consideration_score_for_move,
    position_score_for_move,
};
use move_possibility::{EvalReason, PossibleMove};
use opening::OpeningNode;
use shakmaty::uci::UciMove;
use shakmaty::zobrist::Zobrist64;
use shakmaty::{Chess, Color, EnPassantMode, Move, Position, Role};
use std::collections::HashSet;

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

/// Parse a comma-separated UCI history (e.g. `"e2e4,e7e5,g1f3"`) into moves.
///
/// UCI parsing is position-independent, so this needs no board context.
/// Unparseable tokens stop the parse early, mirroring `replay_moves`.
fn parse_uci_history(moves_played: &str) -> Vec<UciMove> {
    let mut history = Vec::new();
    for raw in moves_played.split(',') {
        let uci = raw.trim();
        if uci.is_empty() {
            continue;
        }
        match uci.parse::<UciMove>() {
            Ok(m) => history.push(m),
            Err(_) => break,
        }
    }
    history
}

/// Sample an index proportionally to its weight using browser-compatible RNG.
fn weighted_sample_index(weights: &[f32]) -> usize {
    let total: f32 = weights.iter().sum();

    let mut rng_bytes = [0u8; 4];
    getrandom::getrandom(&mut rng_bytes).expect("rng failed");
    let rand_val = u32::from_le_bytes(rng_bytes) as f32 / u32::MAX as f32;
    let threshold = rand_val * total;

    let mut cumulative = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        cumulative += w;
        if cumulative >= threshold {
            return i;
        }
    }
    weights.len() - 1
}

pub struct ChessBot {
    opening_tree: OpeningNode,
    seen_positions: HashSet<u64>,
    personality_config: PersonalityConfig,
}

impl Default for ChessBot {
    fn default() -> Self {
        Self {
            opening_tree: OpeningNode::new(),
            seen_positions: HashSet::new(),
            personality_config: PersonalityConfig::default(),
        }
    }
}

impl ChessBot {
    pub fn new() -> ChessBot {
        ChessBot::default()
    }

    /// Load game histories: one game per line, each a comma-separated UCI move
    /// string (e.g. `"e2e4,e7e5,g1f3"`). Feeds both the opening book and the
    /// seen-position set.
    pub fn load_games(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            self.opening_tree.insert(&parse_uci_history(line));
            self.record_seen_positions(line);
        }
    }

    /// Adjust personality weights at runtime (wire these up to JS sliders).
    #[allow(clippy::too_many_arguments)]
    pub fn set_config(
        &mut self,
        ladder_mate_weight: f32,
        knight_bishop_trade_weight: f32,
        knight_eyeing_bishop_weight: f32,
        knight_fork_weight: f32,
        knight_approaching_f6_weight: f32,
        material_weight: f32,
        castling_weight: f32,
        developed_major_pieces_weight: f32,
        min_depth: u32,
        max_depth: u32,
        top_level_moves_to_consider: u32,
        max_moves_to_consider_in_tree: u32,
        min_moves_to_consider_in_tree: u32,
        play_outside_of_book: bool,
        temperature: f32,
    ) {
        self.personality_config = PersonalityConfig {
            temperature,
            ladder_mate_weight,
            knight_bishop_trade_weight,
            knight_eyeing_bishop_weight,
            knight_fork_weight,
            knight_approaching_f6_weight,
            castling_weight,
            developed_major_pieces_weight,
            min_depth,
            max_depth,
            material_weight,
            top_level_moves_to_consider,
            max_moves_to_consider_in_tree,
            min_moves_to_consider_in_tree,
            play_outside_of_book,
        };
    }

    pub fn how_many_times_game_seen(&self, moves_played: &str) -> u32 {
        self.opening_tree.count(&parse_uci_history(moves_played))
    }

    /// Main entry point. `moves_played` is the comma-separated UCI history.
    /// Returns a UCI move string, or an empty string if the game is over.
    pub fn get_move_possibilities(&self, moves_played: &str) -> PossibleMoveList {
        let history = parse_uci_history(moves_played);
        let pos = replay_moves(moves_played);
        let mut final_moves = Vec::new();

        // 1. Opening book: return every booked continuation that is legal here.
        if let Some(book) = self.opening_tree.lookup(&history) {
            let book_moves: Vec<PossibleMove> = book
                .iter()
                .filter_map(|(uci, count)| Some((uci.to_move(&pos).ok()?, count)))
                .map(|(m, count)| PossibleMove {
                    m,
                    eval_reason: EvalReason::OpenerBook { prevalence: *count },
                })
                .collect();

            if !book_moves.is_empty() {
                if self.personality_config.play_outside_of_book {
                    final_moves.extend(book_moves)
                } else {
                    return PossibleMoveList(book_moves);
                }
            }
        }

        // 2. Weighted-random selection with personality heuristics.
        let legals = pos.legal_moves();
        if legals.is_empty() {
            return PossibleMoveList(Vec::new());
        }

        let mut possible_move_weights: Vec<(Move, ConsiderationScore)> = legals
            .into_iter()
            .map(|m| {
                let after = pos.clone().play(m).expect("legal move failed to play");
                (
                    m,
                    consideration_score_for_move(&pos, &m, &after, &self.personality_config),
                )
            })
            .collect();

        possible_move_weights.sort_by(|(_, a_weight), (_, b_weight)| {
            let a_score = a_weight.score();
            let b_score = b_weight.score();

            if !a_score.is_finite() || !b_score.is_finite() {
                // Prevents issues with total_cmp vs .cmp
                panic!("Move weights should be finite");
            }

            a_score.total_cmp(&b_score)
        });

        let number_of_moves_to_consider = usize::min(
            self.personality_config
                .top_level_moves_to_consider
                .try_into()
                .expect("u32 into usize valid"),
            possible_move_weights.len(),
        );

        let (moves_to_consider, moves_to_prune) = possible_move_weights.split_at(number_of_moves_to_consider);

        for (m, consideration_score) in moves_to_consider {
            let after = pos.clone().play(*m).expect("legal move failed to play");
            let (tree_score, depth_searched) = self.tree_score_for_move(&pos, &after);

            final_moves.push(PossibleMove {
                m: *m,
                eval_reason: EvalReason::Considered {
                    consideration_score: *consideration_score,
                    tree_score,
                    depth_searched,
                },
            });
        }

        for (m, consideration_score) in moves_to_prune {
            final_moves.push(PossibleMove {
                m: *m,
                eval_reason: EvalReason::Pruned {
                    consideration_score: *consideration_score,
                },
            });
        }

        PossibleMoveList(final_moves)
    }

    pub fn get_temperature(&self) -> f32 {
        self.personality_config.temperature
    }
}

/// Does this move capture a major piece (knight, bishop, rook, or queen)?
///
/// Capturing one of these extends the tree search beyond `min_depth`.
fn is_major_capture(m: &Move) -> bool {
    matches!(
        m.capture(),
        Some(Role::Knight | Role::Bishop | Role::Rook | Role::Queen)
    )
}

impl ChessBot {
    /// Minmax tree score for a top-level considered move.
    ///
    /// `before` is the position before the move and `after` the position after
    /// it (opponent to move). The maximizing side is whoever moved at the top
    /// level.
    fn tree_score_for_move(&self, before: &Chess, after: &Chess) -> (PositionScore, u32) {
        self.minmax(after, before.turn(), 1)
    }

    /// Recursive minmax search from `pos`, scoring from `maximizing_side`'s view.
    ///
    /// Returns `(position_score, deepest_depth_reached)`, where the score is the
    /// leaf [`PositionScore`] at the end of the principal variation, always from
    /// `maximizing_side`'s perspective. Every branch recurses at least
    /// `min_depth` deep. Past that it keeps going only while a major-piece
    /// capture is available (a quiescence-style extension), stopping once no
    /// such capture exists or `max_depth` is hit.
    fn minmax(&self, pos: &Chess, maximizing_side: Color, depth: u32) -> (PositionScore, u32) {
        let cfg = &self.personality_config;

        if pos.is_checkmate() {
            // The side to move has been mated; good for us only if it isn't us.
            let delivered = pos.turn() != maximizing_side;
            return (PositionScore::checkmate(delivered), depth);
        }

        let legals = pos.legal_moves();
        if legals.is_empty() {
            // Stalemate or other drawn terminal position.
            return (PositionScore::default(), depth);
        }

        let has_major_capture = legals.iter().any(is_major_capture);
        let reached_min = depth >= cfg.min_depth;
        let reached_max = depth >= cfg.max_depth;
        if reached_max || (reached_min && !has_major_capture) {
            return (self.leaf_eval(pos, maximizing_side), depth);
        }

        let candidates = self.candidate_moves(pos, &legals);
        let maximizing = pos.turn() == maximizing_side;
        let mut best: Option<PositionScore> = None;
        let mut deepest = depth;

        for cm in candidates {
            let child = pos.clone().play(cm).expect("legal move failed to play");
            let (score, child_depth) = self.minmax(&child, maximizing_side, depth + 1);
            deepest = deepest.max(child_depth);
            let better = match best {
                None => true,
                Some(b) if maximizing => score.score() > b.score(),
                Some(b) => score.score() < b.score(),
            };
            if better {
                best = Some(score);
            }
        }

        // `candidates` is non-empty whenever `legals` is, but fall back to a
        // static eval if a degenerate config produced no candidates.
        let best = best.unwrap_or_else(|| self.leaf_eval(pos, maximizing_side));
        (best, deepest)
    }

    /// Static evaluation of a leaf position from `maximizing_side`'s view.
    ///
    /// Reuses only the position-based heuristics (`position_score_for_move`), so
    /// the tree search preserves personality at depth without crediting the
    /// transient move-based bonuses of whatever move happened to reach the leaf.
    /// The score is computed from the mover's (`!pos.turn()`) perspective, so we
    /// negate it when the opponent made the move.
    fn leaf_eval(&self, pos: &Chess, maximizing_side: Color) -> PositionScore {
        let mover = !pos.turn();
        let score = position_score_for_move(pos, mover, &self.personality_config);
        if mover == maximizing_side {
            score
        } else {
            score.negated()
        }
    }

    /// Pick the moves to search at one tree node.
    ///
    /// Major-piece captures are taken first, up to `max_moves_to_consider_in_tree`.
    /// If that leaves fewer than `min_moves_to_consider_in_tree` moves, the list
    /// is topped up with the highest `consideration_score_for_move` moves.
    fn candidate_moves(&self, pos: &Chess, legals: &[Move]) -> Vec<Move> {
        let cfg = &self.personality_config;
        let max_captures = cfg.max_moves_to_consider_in_tree as usize;
        let min_moves = cfg.min_moves_to_consider_in_tree as usize;

        let mut chosen: Vec<Move> = Vec::new();
        for m in legals.iter().filter(|m| is_major_capture(m)) {
            if chosen.len() >= max_captures {
                break;
            }
            chosen.push(*m);
        }

        if chosen.len() < min_moves {
            let mut scored: Vec<(Move, f32)> = legals
                .iter()
                .filter(|m| !chosen.contains(m))
                .map(|m| {
                    let after = pos.clone().play(*m).expect("legal move failed to play");
                    let score = consideration_score_for_move(pos, m, &after, cfg).score();
                    (*m, score)
                })
                .collect();
            scored.sort_by(|(_, a), (_, b)| b.total_cmp(a));
            for (m, _) in scored.into_iter().take(min_moves - chosen.len()) {
                chosen.push(m);
            }
        }

        chosen
    }
}

#[derive(serde::Serialize)]
#[serde(transparent)]
pub struct PossibleMoveList(pub Vec<PossibleMove>);

impl PossibleMoveList {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Score every move and sample one according to `temperature`.
    ///
    /// `temperature` is clamped to `[0.0, 1.0]`:
    /// - `0.0` always returns the highest-scoring move.
    /// - `1.0` picks a move completely at random (uniform).
    /// - values in between sharpen/flatten the score-weighted distribution,
    ///   with `0.5` sampling proportionally to (shifted) scores.
    pub fn chose_move(&self, temperature: f32) -> PossibleMove {
        let moves = &self.0;
        assert!(!moves.is_empty(), "chose_move called on empty move list");

        let temperature = temperature.clamp(0.0, 1.0);

        let scores: Vec<f32> = moves.iter().filter(|m| match m.eval_reason {
             EvalReason::Pruned { consideration_score : _ } => false,
             _ => true
        }).map(PossibleMove::score).collect();

        // Index of the best-scoring move (used directly at temperature 0.0).
        let best_idx = scores
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(i, _)| i)
            .expect("non-empty move list has a max");

        if temperature == 0.0 {
            return moves[best_idx].clone();
        }

        // Shift scores so the lowest becomes a small positive base, keeping
        // ordering intact while letting us exponentiate by a sharpness factor.
        let min_score = scores.iter().cloned().fold(f32::INFINITY, f32::min);
        const EPSILON: f32 = 1e-6;
        let bases: Vec<f32> = scores.iter().map(|s| s - min_score + EPSILON).collect();

        // sharpness: temperature 1.0 -> 0 (uniform), -> infinity as temperature -> 0 (argmax).
        let sharpness = 1.0 / temperature - 1.0;
        let weights: Vec<f32> = bases.iter().map(|b| b.powf(sharpness)).collect();

        let chosen = weighted_sample_index(&weights);
        moves[chosen].clone()
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
            let m = match uci
                .parse::<UciMove>()
                .ok()
                .and_then(|u| u.to_move(&pos).ok())
            {
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
    use shakmaty::CastlingMode;

    /// Collect the returned moves as UCI strings for order-independent checks.
    fn uci_set(moves: &[PossibleMove]) -> HashSet<String> {
        moves
            .iter()
            .map(|pm| UciMove::from_move(pm.m, CastlingMode::Standard).to_string())
            .collect()
    }

    #[test]
    fn book_moves_are_returned_from_history() {
        let mut bot = ChessBot::default();
        bot.load_games("e2e4,e7e5,g1f3\ne2e4,e7e5,g1f3\ne2e4,c7c5");

        // From the start, e4 is the only booked first move.
        let root = bot.get_move_possibilities("");
        assert_eq!(uci_set(&root.0), HashSet::from(["e2e4".to_string()]));
        assert!(
            root.0
                .iter()
                .all(|pm| matches!(pm.eval_reason, EvalReason::OpenerBook { prevalence: _ }))
        );

        // After 1.e4, both booked replies (e5 and c5) are returned.
        assert_eq!(
            uci_set(&bot.get_move_possibilities("e2e4").0),
            HashSet::from(["e7e5".to_string(), "c7c5".to_string()])
        );

        // After 1.e4 e5, the only booked reply is Nf3.
        assert_eq!(
            uci_set(&bot.get_move_possibilities("e2e4,e7e5").0),
            HashSet::from(["g1f3".to_string()])
        );
    }

    #[test]
    fn out_of_book_returns_legal_moves() {
        let bot = ChessBot::default(); // empty book
        let moves = bot.get_move_possibilities("");
        // No book, so it falls through to heuristics and returns the legal moves.
        assert!(!moves.is_empty());
        for pm in &moves.0 {
            let uci = UciMove::from_move(pm.m, CastlingMode::Standard);
            assert!(uci.to_move(&Chess::default()).is_ok());
        }
    }

    #[test]
    fn terminal_position_returns_no_moves() {
        // Fool's mate: 1. f3 e5 2. g4 Qh4# — checkmate, no legal moves.
        let bot = ChessBot::default();
        assert!(bot.get_move_possibilities("f2f3,e7e5,g2g4,d8h4").is_empty());
    }
}

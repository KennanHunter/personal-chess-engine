//! Personality heuristics for the bot.
//!
//! All scoring functions are pure: they look only at the position before the
//! move, the move itself, and the resulting position. The tunable weights live
//! in [`PersonalityWeights`], a plain config that the public WASM layer owns and
//! passes into [`score_move`] on every single move evaluation.

use serde::Serialize;
use shakmaty::zobrist::Zobrist64;
use shakmaty::{attacks, Chess, Color, EnPassantMode, Move, Position, Rank, Role, Square};
use std::collections::HashSet;

/// Tunable weights for each heuristic. Owned by the WASM layer and passed into
/// every move evaluation, so personality can be adjusted live from JS.
#[derive(Clone, Copy, Debug)]
pub struct PersonalityConfig {
    /// Reward rooks cutting off the enemy king (ladder-mate pattern).
    pub ladder_mate_weight: f32,
    /// Reward capturing an enemy bishop with one of our knights.
    pub knight_bishop_trade_weight: f32,
    /// Reward a knight attacking an enemy bishop.
    pub knight_eyeing_bishop_weight: f32,
    /// Reward a knight move that forks high-value pieces.
    pub knight_fork_weight: f32,
    /// Reward knights that are one hop away from f6/f3.
    pub knight_approaching_f6_weight: f32,

    pub castling_weight: f32,
    pub material_weight: f32,


    pub play_outside_of_book: bool,

    /// 0.0 means always choose the top move, 1.0 means completely random
    pub temperature: f32,

    pub min_depth: u32,

    pub max_depth: u32,

    /// Total number of moves to consider at the top level
    pub top_level_moves_to_consider: u32,

    /// Minimum moves to consider in the game tree when evaluating a specific move
    /// this is minim moves because we always consider major piece captures, up to a maximum
    pub min_moves_to_consider_in_tree: u32,

    /// Minimum moves to consider in the game tree when evaluating a specific move
    /// this intentionally makes the bot susceptible to attacks that overload the amount of
    /// major piece captures the bot considers
    pub max_moves_to_consider_in_tree: u32,

}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            ladder_mate_weight: 2.0,
            knight_bishop_trade_weight: 3.0,
            knight_eyeing_bishop_weight: 1.0,
            knight_fork_weight: 4.0,
            knight_approaching_f6_weight: 0.8,
            material_weight: 5.0,
            temperature: 0.0,
            castling_weight: 1.0,
            min_depth: 2,
            max_depth: 5,
            play_outside_of_book: false,
            top_level_moves_to_consider: 8,
            max_moves_to_consider_in_tree: 5,
            min_moves_to_consider_in_tree: 3,
        }
    }
}

#[derive(Serialize, Clone, Copy)]
pub struct ConsiderationScore {
    checkmate_score: f32,
    material_score: f32,
    ladder_mate_score: f32,
    knight_bishop_trade_score: f32,
    knight_eyeing_bishop_score: f32,
    knight_fork_score: f32,
    knight_approaching_f6_score: f32,
    castling_score: f32,
}

impl ConsiderationScore {
    pub fn score(&self) -> f32 {
        // if we ever forget to update this we get an error
        let ConsiderationScore {
            checkmate_score,
            material_score,
            ladder_mate_score,
            knight_bishop_trade_score,
            knight_eyeing_bishop_score,
            knight_fork_score,
            knight_approaching_f6_score,
            castling_score,
        } = self;

        checkmate_score
            + material_score
            + ladder_mate_score
            + knight_bishop_trade_score
            + knight_eyeing_bishop_score
            + knight_fork_score
            + knight_approaching_f6_score
            + castling_score
    }
}

/// Combine every heuristic into a single bonus score for a candidate move.
///
/// `before` is the position to move in, `m` the candidate move, and `after` the
/// position that results from playing it. `seen` is the set of Zobrist hashes
/// of positions from game history.
pub fn consideration_score_for_move(
    before: &Chess,
    m: &Move,
    after: &Chess,
    weights: &PersonalityConfig,
) -> ConsiderationScore {
    let side = before.turn();
    ConsiderationScore {
        checkmate_score: if after.is_checkmate() { 100.0 } else { 0.0 },
        material_score: score_material(after, side) * weights.material_weight,
        ladder_mate_score: score_ladder_mate(after, side) * weights.ladder_mate_weight,
        knight_bishop_trade_score: score_knight_bishop_trade(before, after)
            * weights.knight_bishop_trade_weight,
        knight_eyeing_bishop_score: score_knight_eyeing_bishop(after)
            * weights.knight_eyeing_bishop_weight,
        knight_fork_score: score_knight_fork(before, m, after) * weights.knight_fork_weight,
        knight_approaching_f6_score: score_knight_approaching_f6(after)
            * weights.knight_approaching_f6_weight,
        castling_score: score_did_castle(m) * weights.castling_weight,
    }
}

fn score_material(after: &Chess, side: Color) -> f32 {
    let material_count: u8 = after.board().material_side(side).iter().sum();

    material_count.into()
}

/// Reward rooks on the 7th/8th rank or cutting off the enemy king.
pub fn score_ladder_mate(after: &Chess, side: Color) -> f32 {
    let board = after.board();
    let our_rooks = board.rooks() & board.by_color(side);
    let occupied = board.occupied();
    let mut score = 0.0;

    let enemy_king = match board.king_of(!side) {
        Some(sq) => sq,
        None => return 0.0,
    };

    for sq in our_rooks {
        if sq.rank() == Rank::Seventh || sq.rank() == Rank::Eighth {
            score += 1.0;
        }
        if attacks::rook_attacks(sq, occupied).contains(enemy_king) {
            score += 1.5;
        }
    }
    score
}

/// Reward giving up a knight to win a bishop on the same move.
pub fn score_knight_bishop_trade(before: &Chess, after: &Chess) -> f32 {
    let side = before.turn();
    let opp = !side;
    let b = before.board();
    let a = after.board();

    let our_knights_lost =
        (b.knights() & b.by_color(side)).count() - (a.knights() & a.by_color(side)).count();
    let their_bishops_lost =
        (b.bishops() & b.by_color(opp)).count() - (a.bishops() & a.by_color(opp)).count();

    if our_knights_lost > 0 && their_bishops_lost > 0 {
        1.0
    } else {
        0.0
    }
}

/// Reward one of our knights attacking an enemy bishop in the resulting position.
pub fn score_knight_eyeing_bishop(after: &Chess) -> f32 {
    // `after` is the opponent's turn, so the side that just moved is `!turn`.
    let side = !after.turn();
    let opp = after.turn();
    let board = after.board();

    let our_knights = board.knights() & board.by_color(side);
    let their_bishops = board.bishops() & board.by_color(opp);
    let mut score = 0.0;

    for sq in our_knights {
        if !(attacks::knight_attacks(sq) & their_bishops).is_empty() {
            score += 1.0;
        }
    }
    score
}

/// Reward a knight move that lands on a square forking high-value pieces.
pub fn score_knight_fork(before: &Chess, m: &Move, after: &Chess) -> f32 {
    if m.role() != Role::Knight {
        return 0.0;
    }

    let side = before.turn();
    let opp = !side;
    let board = after.board();
    let dest = m.to();

    let high_value = board.queens() | board.rooks() | board.kings();
    let attacked = attacks::knight_attacks(dest) & board.by_color(opp) & high_value;

    match attacked.count() {
        2 => 1.0,           // fork
        n if n >= 3 => 1.5, // royal-family fork
        _ => 0.0,
    }
}

/// Reward knights that are a single hop from f6 (white) or f3 (black).
pub fn score_knight_approaching_f6(after: &Chess) -> f32 {
    let side = !after.turn();
    let board = after.board();
    let our_knights = board.knights() & board.by_color(side);

    let targets = [Square::F6, Square::F3];
    let mut score = 0.0;

    for sq in our_knights {
        let one_hop = attacks::knight_attacks(sq);
        if targets.iter().any(|&t| one_hop.contains(t)) {
            score += 1.0;
        }
    }
    score
}

/// Reward reaching a position seen in game history.
pub fn score_seen(after: &Chess, seen: &HashSet<u64>) -> f32 {
    let hash: Zobrist64 = after.zobrist_hash(EnPassantMode::Legal);
    if seen.contains(&hash.0) {
        1.0
    } else {
        0.0
    }
}

/// reward a move that castles
pub fn score_did_castle(m: &Move) -> f32 {
    if m.is_castle() {
        1.0
    } else {
        0.0
    }
}

//! Personality heuristics for the bot.
//!
//! All scoring functions are pure: they look only at the position before the
//! move, the move itself, and the resulting position. The tunable weights live
//! in [`PersonalityWeights`], a plain config that the public WASM layer owns and
//! passes into [`score_move`] on every single move evaluation.

use std::collections::HashSet;

use shakmaty::zobrist::Zobrist64;
use shakmaty::{CastlingSide, Chess, Color, EnPassantMode, Move, Position, Rank, Role, Square, attacks};

/// Tunable weights for each heuristic. Owned by the WASM layer and passed into
/// every move evaluation, so personality can be adjusted live from JS.
#[derive(Clone, Copy, Debug)]
pub struct PersonalityWeights {
    /// Reward rooks cutting off the enemy king (ladder-mate pattern).
    pub ladder_mate: f32,
    /// Reward capturing an enemy bishop with one of our knights.
    pub knight_bishop_trade: f32,
    /// Reward a knight attacking an enemy bishop.
    pub knight_eyeing_bishop: f32,
    /// Reward a knight move that forks high-value pieces.
    pub knight_fork: f32,
    /// Reward knights that are one hop away from f6/f3.
    pub knight_approaching_f6: f32,
    /// Reward reaching a position we have been in before (from game history).
    pub seen_position: f32,
    /// Spread of opener randomness
    pub opener_temperature: f32,
    /// Castling
    pub castling: f32,
    /// Depth
    pub depth: u32,
}

impl Default for PersonalityWeights {
    fn default() -> Self {
        Self {
            ladder_mate: 2.0,
            knight_bishop_trade: 3.0,
            knight_eyeing_bishop: 1.0,
            knight_fork: 4.0,
            knight_approaching_f6: 0.8,
            seen_position: 2.5,
            opener_temperature: 0.0,
            castling: 1.0,
            depth: 2,
        }
    }
}

/// Combine every heuristic into a single bonus score for a candidate move.
///
/// `before` is the position to move in, `m` the candidate move, and `after` the
/// position that results from playing it. `seen` is the set of Zobrist hashes
/// of positions from game history.
pub fn score_move(
    before: &Chess,
    m: &Move,
    after: &Chess,
    weights: &PersonalityWeights,
    seen: &HashSet<u64>,
) -> f32 {
    let side = before.turn();
    let mut score = 0.0;
    score += score_ladder_mate(after, side) * weights.ladder_mate;
    score += score_knight_bishop_trade(before, after) * weights.knight_bishop_trade;
    score += score_knight_eyeing_bishop(after) * weights.knight_eyeing_bishop;
    score += score_knight_fork(before, m, after) * weights.knight_fork;
    score += score_knight_approaching_f6(after) * weights.knight_approaching_f6;
    score += score_seen(after, seen) * weights.seen_position;
    score += score_did_castle(m) * weights.castling;
    score
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
        2 => 1.0,        // fork
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
pub fn score_did_castle( m: &Move) -> f32 {
    if m.is_castle() {
        1.0
    } else {
        0.0
    }
}

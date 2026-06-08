use crate::heuristics::{ConsiderationScore, PositionScore};
use serde::Serialize;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Move};

#[derive(Serialize, Clone)]
pub struct PossibleMove {
    /// Serialized to JavaScript as a UCI move string (e.g. `"e2e4"`).
    #[serde(rename = "move", serialize_with = "serialize_move_as_uci")]
    pub m: Move,
    pub eval_reason: EvalReason,
}

impl PossibleMove {
    pub fn to_uci(&self) -> UciMove {
        self.m.to_uci(CastlingMode::Standard)
    }

    /// Scalar score used to rank/sample this move.
    ///
    /// Higher is better. Book moves use their prevalence, considered moves add
    /// their tree search result on top of the heuristic consideration score,
    /// and pruned moves fall back to the consideration score alone.
    pub fn score(&self) -> f32 {
        match &self.eval_reason {
            EvalReason::OpenerBook { prevalence } => *prevalence as f32,
            EvalReason::Considered {
                consideration_score,
                tree_score,
                depth_searched: _,
            } => consideration_score.score() + tree_score.score(),
            EvalReason::Pruned {
                consideration_score,
            } => consideration_score.score(),
        }
    }
}

/// `Move` itself isn't `Serialize`, so emit its position-independent UCI form.
fn serialize_move_as_uci<S>(m: &Move, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    UciMove::from_move(*m, CastlingMode::Standard).serialize(serializer)
}

#[derive(Serialize, Clone)]
pub enum EvalReason {
    OpenerBook {
        prevalence: u32,
    },
    Considered {
        consideration_score: ConsiderationScore,
        tree_score: PositionScore,
        depth_searched: u32,
    },
    Pruned {
        consideration_score: ConsiderationScore,
    },
}

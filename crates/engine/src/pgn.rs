//! PGN reading: turn a PGN file (SAN move text) into the labeled UCI move
//! histories the bot consumes.
//!
//! This is native-only tooling (it depends on `pgn-reader`); it is compiled out
//! of the WASM build. Use it to convert a downloaded PGN into the `\n`-separated
//! format that [`crate::ChessBot::load_games`] expects. Each output line is
//! `white_name|black_name|uci1,uci2,...`, and missing tags become empty fields.

use pgn_reader::{RawTag, Reader, SanPlus, Visitor};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Position};
use std::io;
use std::ops::ControlFlow;

/// Visitor that replays a game's mainline and collects each move as UCI.
struct UciCollector;

struct TagState {
    white: String,
    black: String,
}

/// State carried through a single game: the running position, the UCI moves,
/// and the player-name tags copied in from `TagState`.
struct GameState {
    pos: Chess,
    moves: Vec<String>,
    white: String,
    black: String,
}

impl Visitor for UciCollector {
    type Tags = TagState;
    type Movetext = GameState;
    type Output = Option<GameOutput>;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(TagState {
            white: String::new(),
            black: String::new(),
        })
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let slot = match name {
            b"White" => Some(&mut tags.white),
            b"Black" => Some(&mut tags.black),
            _ => None,
        };
        if let Some(slot) = slot {
            *slot = value.decode_utf8_lossy().into_owned();
        }
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(GameState {
            pos: Chess::default(),
            moves: Vec::new(),
            white: tags.white,
            black: tags.black,
        })
    }

    fn san(&mut self, state: &mut Self::Movetext, san_plus: SanPlus) -> ControlFlow<Self::Output> {
        match san_plus.san.to_move(&state.pos) {
            Ok(m) => {
                state
                    .moves
                    .push(UciMove::from_move(m, CastlingMode::Standard).to_string());
                state.pos.play_unchecked(m);
                ControlFlow::Continue(())
            }
            // Stop on the first illegal/ambiguous move; keep what we parsed.
            Err(_) => ControlFlow::Break(Some(GameOutput {
                white: std::mem::take(&mut state.white),
                black: std::mem::take(&mut state.black),
                moves: std::mem::take(&mut state.moves),
            })),
        }
    }

    fn end_game(&mut self, state: Self::Movetext) -> Self::Output {
        Some(GameOutput {
            white: state.white,
            black: state.black,
            moves: state.moves,
        })
    }
}

pub struct GameOutput {
    pub white: String,
    pub black: String,
    pub moves: Vec<String>,
}

impl GameOutput {
    /// Serialize as `white|black|uci,uci,...`. Any `|` in a name is stripped so
    /// the format stays parseable by a simple `split('|')`.
    fn to_line(&self) -> String {
        let sanitize = |s: &str| s.replace('|', "");
        format!(
            "{}|{}|{}",
            sanitize(&self.white),
            sanitize(&self.black),
            self.moves.join(",")
        )
    }
}

/// Convert PGN text into one labeled UCI line per game.
///
/// Games with no parsed moves are skipped. Variations are ignored; only the
/// mainline of each game is followed. Each line is `white|black|uci,uci,...`.
pub fn pgn_to_uci_lines(pgn: &str) -> io::Result<Vec<String>> {
    let mut reader = Reader::new(io::Cursor::new(pgn.as_bytes()));
    let mut lines = Vec::new();

    while let Some(game) = reader.read_game(&mut UciCollector)? {
        if let Some(game) = game {
            if !game.moves.is_empty() {
                lines.push(game.to_line());
            }
        }
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_simple_game_to_uci() {
        let pgn = "[White \"Alice\"]\n[Black \"Bob\"]\n\n1. e4 e5 2. Nf3 Nc6 *";
        let lines = pgn_to_uci_lines(pgn).unwrap();
        assert_eq!(lines, vec!["Alice|Bob|e2e4,e7e5,g1f3,b8c6".to_string()]);
    }

    #[test]
    fn handles_multiple_games_and_castling() {
        let pgn = "1. e4 e5 *\n\n1. e4 e5 2. Bc4 Bc5 3. Nf3 Nf6 4. O-O O-O *";
        let lines = pgn_to_uci_lines(pgn).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "||e2e4,e7e5");
        assert!(lines[1].starts_with("||"));
        assert!(lines[1].contains("e1g1")); // white short castle in UCI
        assert!(lines[1].contains("e8g8")); // black short castle in UCI
    }

    #[test]
    fn skips_games_with_no_moves() {
        let pgn = "[Event \"empty\"]\n\n*";
        let lines = pgn_to_uci_lines(pgn).unwrap();
        assert!(lines.is_empty());
    }
}

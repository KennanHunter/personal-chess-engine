//! PGN reading: turn a PGN file (SAN move text) into the comma-separated UCI
//! move histories the bot consumes.
//!
//! This is native-only tooling (it depends on `pgn-reader`); it is compiled out
//! of the WASM build. Use it to convert a downloaded PGN into the `\n`-separated,
//! comma-separated UCI format that [`crate::ChessBot::load_games`] expects.

use std::io;
use std::ops::ControlFlow;

use pgn_reader::{Reader, SanPlus, Visitor};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Position};

/// Visitor that replays a game's mainline and collects each move as UCI.
struct UciCollector;

/// State carried through a single game: the running position and the UCI moves.
struct GameState {
    pos: Chess,
    moves: Vec<String>,
}

impl Visitor for UciCollector {
    type Tags = ();
    type Movetext = GameState;
    type Output = Vec<String>;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, _tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(GameState {
            pos: Chess::default(),
            moves: Vec::new(),
        })
    }

    fn san(
        &mut self,
        state: &mut Self::Movetext,
        san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        match san_plus.san.to_move(&state.pos) {
            Ok(m) => {
                state
                    .moves
                    .push(UciMove::from_move(m, CastlingMode::Standard).to_string());
                state.pos.play_unchecked(m);
                ControlFlow::Continue(())
            }
            // Stop on the first illegal/ambiguous move; keep what we parsed.
            Err(_) => ControlFlow::Break(std::mem::take(&mut state.moves)),
        }
    }

    fn end_game(&mut self, state: Self::Movetext) -> Self::Output {
        state.moves
    }
}

/// Convert PGN text into one comma-separated UCI line per game.
///
/// Games with no parsed moves are skipped. Variations are ignored; only the
/// mainline of each game is followed.
pub fn pgn_to_uci_lines(pgn: &str) -> io::Result<Vec<String>> {
    let mut reader = Reader::new(io::Cursor::new(pgn.as_bytes()));
    let mut lines = Vec::new();

    while let Some(moves) = reader.read_game(&mut UciCollector)? {
        if !moves.is_empty() {
            lines.push(moves.join(","));
        }
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_simple_game_to_uci() {
        let pgn = "1. e4 e5 2. Nf3 Nc6 *";
        let lines = pgn_to_uci_lines(pgn).unwrap();
        assert_eq!(lines, vec!["e2e4,e7e5,g1f3,b8c6".to_string()]);
    }

    #[test]
    fn handles_multiple_games_and_castling() {
        let pgn = "1. e4 e5 *\n\n1. e4 e5 2. Bc4 Bc5 3. Nf3 Nf6 4. O-O O-O *";
        let lines = pgn_to_uci_lines(pgn).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "e2e4,e7e5");
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

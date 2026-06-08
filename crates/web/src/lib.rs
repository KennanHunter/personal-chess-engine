//! WASM bindings for the chess engine.
//!
//! This crate is a thin `wasm-bindgen` wrapper around [`engine::ChessBot`]; all
//! chess logic lives in the `engine` crate. Build it with `wasm-pack`:
//!
//! ```bash
//! wasm-pack build crates/web --target web --release --scope kennanhunter
//! ```

mod utils;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct ChessBot {
    inner: engine::ChessBot,
}

#[wasm_bindgen]
impl ChessBot {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ChessBot {
        utils::set_panic_hook();

        let mut inner = engine::ChessBot::new();

        inner.load_games(include_str!("games.txt"));

        ChessBot {
            inner,
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
        castling: f32,
        depth: u32,
    ) {
        self.inner.set_weights(
            ladder_mate,
            knight_bishop_trade,
            knight_eyeing_bishop,
            knight_fork,
            knight_approaching_f6,
            seen_position,
            opener_temperature,
            castling,
            depth,
        );
    }

    pub fn how_many_times_game_seen(&self, moves_played: &str) -> u32 {
        self.inner.how_many_times_game_seen(moves_played)
    }

    /// Main entry point. `moves_played` is the comma-separated UCI history.
    /// Returns a UCI move string, or an empty string if the game is over.
    pub fn get_move(&self, moves_played: &str) -> String {
        self.inner.get_move(moves_played)
    }
}



impl Default for ChessBot {
    fn default() -> Self {
        Self::new()
    }
}

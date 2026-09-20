//! WASM bindings for the chess engine.
//!
//! This crate is a thin `wasm-bindgen` wrapper around [`engine::ChessBot`]; all
//! chess logic lives in the `engine` crate. Build it with `wasm-pack`:
//!
//! ```bash
//! wasm-pack build crates/web --target web --release --scope kennanhunter
//! ```

mod utils;

use engine::PossibleMoveList;
use wasm_bindgen::prelude::*;

const GAMES_TXT: &str = include_str!("games.txt");

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

        inner.load_games(GAMES_TXT);

        ChessBot { inner }
    }

    /// Reload the built-in games with a player-color filter.
    ///
    /// `player` is matched against the `[White]`/`[Black]` tags baked into each
    /// line. A game is kept when it satisfies either enabled side:
    ///   - `include_white` && line's white player == `player`
    ///   - `include_black` && line's black player == `player`
    ///
    /// If both flags are false, no games are loaded. If `player` is empty, the
    /// full games set is reloaded regardless of the flags.
    pub fn set_player_filter(&mut self, player: &str, include_white: bool, include_black: bool) {
        self.inner.clear_games();

        if player.is_empty() {
            self.inner.load_games(GAMES_TXT);
            return;
        }

        self.inner.load_games_filtered(GAMES_TXT, |white, black| {
            (include_white && white == player) || (include_black && black == player)
        });
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
        material_weight: f32,
        castling: f32,
        developed_major_pieces_weight: f32,
        min_depth: u32,
        max_depth: u32,
        top_level_moves_to_consider: u32,
        max_moves_to_consider_in_tree: u32,
        min_moves_to_consider_in_tree: u32,
        play_outside_of_tree: bool,
        temperature: f32,
    ) {
        self.inner.set_config(
            ladder_mate,
            knight_bishop_trade,
            knight_eyeing_bishop,
            knight_fork,
            knight_approaching_f6,
            material_weight,
            castling,
            developed_major_pieces_weight,
            min_depth,
            max_depth,
            top_level_moves_to_consider,
            max_moves_to_consider_in_tree,
            min_moves_to_consider_in_tree,
            play_outside_of_tree,
            temperature,
        );
    }

    pub fn how_many_times_game_seen(&self, moves_played: &str) -> u32 {
        self.inner.how_many_times_game_seen(moves_played)
    }

    /// Main entry point. `moves_played` is the comma-separated UCI history.
    /// Returns the candidate moves as a JS array of `PossibleMove` objects
    /// (empty when the game is over).
    pub fn get_move(&self, moves_played: &str) -> JsValue {
        #[derive(serde::Serialize)]
        struct PublicMoveResponse {
            possible: PossibleMoveList,
            chosen: String,
        }

        let possible = self.inner.get_move_possibilities(moves_played);

        let chosen = possible.chose_move(self.inner.get_temperature());

        serde_wasm_bindgen::to_value(&PublicMoveResponse {
            possible,
            chosen: chosen.to_uci().to_string(),
        })
        .expect("PossibleMove serialization failed")
    }
}

impl Default for ChessBot {
    fn default() -> Self {
        Self::new()
    }
}

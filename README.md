# chess-engine

A small chaotic chess bot compiled to WebAssembly. It mixes an opening book
built from your own games with weighted-random move selection guided by a few
"personality" heuristics — aiming for fun, human-ish play rather than strength.

## How it works

1. **Opening book** — replays moves from your game history; while in book, it
   plays the most-frequent continuation.
2. **Out of book** — scores every legal move with the heuristics below, then
   samples one proportionally to its weight (base weight `1.0`, so any legal
   move can still happen).

Heuristics (see [`src/heuristics.rs`](src/heuristics.rs)): ladder mate,
knight-for-bishop trades, knights eyeing bishops, knight forks, knights
approaching f6/f3, and revisiting positions seen in your games. Each has a
tunable weight in `PersonalityWeights`, owned by the WASM layer and passed in on
every move evaluation.

## Build (WASM)

```bash
cargo install wasm-pack
wasm-pack build --target web --release
```

```ts
import init, { ChessBot } from './pkg/chess_engine.js';
await init();

const bot = new ChessBot();
bot.load_games(gamesText);          // newline-separated, comma-separated UCI
const move = bot.get_move('e2e4,e7e5'); // -> e.g. "g1f3", or "" if game over
```

## Converting PGN to the game format

`load_games` expects one game per line, each a comma-separated UCI string. Use
the bundled converter to turn a PGN export into that format:

```bash
cargo run --bin convert_pgn -- kennan.pgn > src/games.txt
```

## Test

```bash
cargo test
```

## Layout

| Path | Purpose |
|------|---------|
| [`src/lib.rs`](src/lib.rs) | `ChessBot` WASM struct: `load_games`, `set_weights`, `get_move` |
| [`src/heuristics.rs`](src/heuristics.rs) | `PersonalityWeights` config + move scoring |
| [`src/opening.rs`](src/opening.rs) | Opening-book prefix tree |
| [`src/pgn.rs`](src/pgn.rs) | PGN → UCI conversion (native-only) |
| [`src/bin/convert_pgn.rs`](src/bin/convert_pgn.rs) | CLI for the conversion above |

## License

MIT or Apache-2.0.

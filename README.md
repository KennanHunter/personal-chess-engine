# chess-engine

A small chaotic chess bot compiled to WebAssembly based on my games using weighted-random move selection guided by a few heuristics based on my own rules.

## How it works

1. **Opening book** — replays moves from my game history; while in book, it
   plays the most-frequent continuation sampled with a temperature.
2. **Out of book** — scores every legal move heuristics below, then
   samples one proportionally to its weight

## Personality weights

`PersonalityWeights` controls how strongly each heuristic biases move selection.
Higher values make the corresponding behavior more likely. They can be adjusted
live from JS via `bot.set_weights(...)`.

## Build (WASM)

```bash
cargo install wasm-pack
wasm-pack build crates/web --target web --release --scope kennanhunter
```

```ts
import init, { ChessBot } from './crates/web/pkg/personal_chess_engine.js';
await init();

const bot = new ChessBot();
const move = bot.get_move('e2e4,e7e5'); // -> e.g. "g1f3", or "" if game over
```

## Converting PGN to the game format

`load_games` expects one game per line, each a comma-separated UCI string. Use
the bundled converter to turn a PGN export into that format:

```bash
cargo run --bin convert_pgn -- kennan.pgn > crates/web/src/games.txt
```

## Test

```bash
cargo test
```

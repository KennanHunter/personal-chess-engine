# chess-engine

A small chaotic chess bot compiled to WebAssembly based on my games using weighted-random move selection guided by a few heuristics based on my own rules.

## How it works

1. **Opening book** — replays moves from my game history; while in book, it
   plays the most-frequent continuation sampled with a temperature.
2. **Out of book** — scores every legal move with the heuristics below, then
   samples one proportionally to its weight (base weight `1.0`, so any legal
   move can still happen).

Heuristics (see [`src/heuristics.rs`](src/heuristics.rs)): ladder mate,
knight-for-bishop trades, knights eyeing bishops, knight forks, knights
approaching f6/f3, and revisiting positions seen in your games. Each has a
tunable weight in `PersonalityWeights`, owned by the WASM layer and passed in on
every move evaluation.

## Personality weights

`PersonalityWeights` controls how strongly each heuristic biases move selection.
Higher values make the corresponding behavior more likely. They can be adjusted
live from JS via `bot.set_weights(...)`.

| Weight | Default | Effect |
|--------|---------|--------|
| `ladder_mate` | `2.0` | Reward rooks on the 7th/8th rank or cutting off the enemy king (ladder-mate pattern). |
| `knight_bishop_trade` | `3.0` | Reward giving up a knight to win an enemy bishop on the same move. |
| `knight_eyeing_bishop` | `1.0` | Reward a knight attacking an enemy bishop. |
| `knight_fork` | `4.0` | Reward a knight move that forks high-value pieces (queen/rook/king). |
| `knight_approaching_f6` | `0.8` | Reward knights one hop away from f6 (white) or f3 (black). |
| `seen_position` | `2.5` | Reward reaching a position seen before in your game history. |
| `opener_temperature` | `0.0` | Spread of randomness while still in the opening book. |

## Build (WASM)

```bash
cargo install wasm-pack
wasm-pack build --target web --release --scope kennanhunter
```

```ts
import init, { ChessBot } from './pkg/chess_engine.js';
await init();

const bot = new ChessBot();
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

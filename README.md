FZChess
=======

FZChess chess engine rewritten in C++

Install on Android:
The above file is only the chess engine, you need a gui to connect to. DroidFish Android application contains the Stockfish chess engine and its own gui can be used with 3rd party engines:

1. Download Android version of FZChess engine: http://ongroa.com/fzchess
2. Download DroidFish Chess from Android Market: https://play.google.com/store/apps/details?id=org.petero.droidfish&hl=en
3. Copy downloaded fzchess file to DroidFish/uci directory in your Android phone.
4. From the program menu, choose Manage Chess Engines and Select Chess Engine and select the fzchess engine to use.


Opening book (Rust engine)
==========================

The Rust engine (`rust/`) supports Polyglot opening books (`.bin`). The book is
read at runtime and is **not** bundled with the source — supply your own and
point the engine at it. When no book is present or the book is disabled, the
engine simply searches as usual, so book support is fully opt-in.

UCI options:

- `OwnBook` (check, default `true`) — enable book moves. While a position is in
  the book and `OwnBook` is on, the engine returns a book move instantly instead
  of searching. On by default; disable with `setoption name OwnBook value false`.
- `BookFile` (string, default `performance.bin`) — path to the Polyglot `.bin`
  book, resolved relative to the engine's working directory. Setting it attempts
  to load the book immediately; load failures are reported via an `info string`
  line and leave the book disabled.
- `BookBestMove` (check, default `false`) — when `true`, always play the
  highest-weight book move (deterministic); when `false`, pick a move at random
  weighted by each entry's weight (varied play, the default).

Example UCI session:

```
uci
setoption name BookFile value performance.bin
setoption name OwnBook value true
position startpos
go
```

Polyglot `.bin` format: a key-sorted array of 16-byte big-endian records
(`key: u64`, `move: u16`, `weight: u16`, `learn: u32`). Place `performance.bin`
in the `rust/` directory to use the default path. The book file is gitignored.

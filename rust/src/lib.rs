//! FZChess — Rust port (work in progress).
//!
//! Being ported bottom-up from the C++ engine, verified against the shared
//! `tools/golden.sh` perft + bestmove baseline. Leaf modules land first
//! (types, util); move generation + perft are next; evaluation, search, and
//! the UCI loop follow.
//!
//! This is the library crate root. The `fzchess` binary (`main.rs`) is a thin
//! CLI wrapper over this API, which keeps the engine logic testable and
//! reusable independently of the command-line front end.

// Items are implemented ahead of the code that consumes them during the port.
#![allow(dead_code)]

pub mod board;
pub mod eval;
pub mod movegen;
pub mod perft;
pub mod search;
pub mod tt;
pub mod types;
pub mod util;

pub use board::Board;

//! FZChess — Rust port (work in progress).
//!
//! Being ported bottom-up from the C++ engine, verified against the shared
//! `tools/golden.sh` perft + bestmove baseline. Leaf modules land first
//! (types, util); move generation, evaluation, search, and the UCI loop follow.

// Leaf modules are implemented ahead of the code that consumes them, so items
// are legitimately unused until later port slices wire them in. Scoped to the
// crate and intended to be removed once the engine is fully assembled.
#![allow(dead_code)]

mod types;
mod util;

fn main() {
    // Placeholder. The UCI command loop will be ported in a later slice; until
    // then this binary exists so the crate builds and the leaf modules
    // (types, util) are exercised by `cargo test`.
    eprintln!("fzchess (rust port) — not yet a functional engine");
}

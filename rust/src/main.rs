//! `fzchess` CLI — a thin front end over the `fzchess` library crate.
//!
//! Engine logic lives in the library (see `lib.rs`); this binary only parses
//! arguments and formats output.

use fzchess::perft::{PerftCase, SUITE};

/// Print one perft case in the C++ engine's format ("depth: N nodes: M"), one
/// line per depth, so the output diffs cleanly against the golden baseline.
fn print_case(case: &PerftCase) {
    for (depth, nodes) in case.run() {
        println!("depth: {} nodes: {}", depth, nodes);
    }
}

/// Run the standard suite. The C++ suite prints the startpos block twice; we
/// replicate that so the line-by-line golden comparison matches exactly.
fn run_perft_suite() {
    // First suite entry is startpos; the C++ engine emits it twice.
    print_case(&SUITE[0]);
    for case in SUITE {
        print_case(case);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("perft") => run_perft_suite(),
        _ => eprintln!("usage: fzchess perft   # run the perft suite"),
    }
}

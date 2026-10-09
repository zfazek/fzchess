//! `fzchess` CLI — a thin front end over the `fzchess` library crate.
//!
//! Engine logic lives in the library (see `lib.rs`); this binary only parses
//! arguments, formats output, and (for the bench subcommand) does timing.

use fzchess::board::Board;
use fzchess::perft::{PerftCase, SUITE};
use fzchess::search::Search;
use std::time::Instant;

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

/// Time a single perft(position, depth) and print nodes, elapsed, and NPS.
/// Output format is intended for the comparison script, not the golden diff.
///
/// `fen` is a FEN body, or the literal "startpos".
fn run_perft_bench(fen: &str, depth: i32) {
    let case = if fen == "startpos" {
        PerftCase::startpos(depth)
    } else {
        // Leak is fine: the process exits right after; this keeps the API's
        // &'static str contract without threading lifetimes through the CLI.
        PerftCase::from_fen(Box::leak(fen.to_string().into_boxed_str()), depth)
    };
    let start = Instant::now();
    let nodes = case.count(depth);
    let secs = start.elapsed().as_secs_f64();
    let nps = if secs > 0.0 { (nodes as f64 / secs) as u64 } else { 0 };
    println!(
        "engine=rust depth={} nodes={} time_ms={:.1} nps={}",
        depth,
        nodes,
        secs * 1000.0,
        nps
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("perft") => run_perft_suite(),
        Some("perft-bench") => {
            // Usage: fzchess perft-bench <depth> [fen...]
            // No FEN (or "startpos") benches the starting position.
            let depth: i32 = args
                .get(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(5);
            let fen = if args.len() > 3 {
                args[3..].join(" ")
            } else {
                "startpos".to_string()
            };
            run_perft_bench(&fen, depth);
        }
        Some("go") => {
            // Usage: fzchess go <depth> [fen...]
            // Fixed-depth search; prints the C++-style info/bestmove lines so the
            // bestmove can be diffed against the golden baseline.
            let depth: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
            let mut board = Board::new();
            if args.len() > 3 {
                let fen = args[3..].join(" ");
                if fen != "startpos" {
                    board.setboard(&fen);
                }
            }
            let mut search = Search::new(&mut board);
            // default_seldepth = 0, break_if_mate_found = true: matches the C++
            // UCI `go depth` path that produced the golden baseline.
            search.make_move(depth, 0, true);
        }
        _ => eprintln!(
            "usage:\n  fzchess perft                     # run the perft suite\n  fzchess perft-bench <depth> [fen] # time a single perft\n  fzchess go <depth> [fen]          # fixed-depth search, prints bestmove"
        ),
    }
}


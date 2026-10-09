//! UCI (Universal Chess Interface) command loop.
//!
//! This is a from-scratch, spec-conformant implementation rather than a port of
//! the C++ engine's ad-hoc `strstr`-based parser. Priorities (per the UCI spec):
//!   * tokenize on whitespace; arbitrary spacing is allowed;
//!   * unknown leading tokens are skipped and the rest of the line is parsed
//!     ("fail silently"), e.g. "joho go depth 2" still runs the go;
//!   * commands that don't apply are ignored (e.g. `stop` when not searching);
//!   * every `go` eventually produces a `bestmove`.
//!
//! The search runs on a background thread so `stop` and `quit` stay responsive
//! while thinking; a shared `AtomicBool` is the stop signal.

use crate::board::Board;
use crate::search::Search;
use crate::util::str2move;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

/// Parsed `go` parameters relevant to this engine.
#[derive(Default, Clone, Copy)]
struct GoParams {
    depth: Option<i32>,
    movetime: Option<u64>,
    infinite: bool,
    wtime: Option<u64>,
    btime: Option<u64>,
    winc: Option<u64>,
    binc: Option<u64>,
    movestogo: Option<u64>,
}

/// Handle to a running search thread.
struct Searching {
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

/// Run the UCI loop reading commands from stdin until `quit` (or EOF).
pub fn run() {
    let stdin = io::stdin();
    let mut board = Board::new();
    let mut searching: Option<Searching> = None;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // EOF / read error
        };
        let mut tokens = line.split_whitespace().peekable();

        // Skip leading unknown tokens until we hit a recognised command
        // (spec: ignore unknown tokens, parse the rest of the line).
        let cmd = loop {
            match tokens.peek() {
                None => break None,
                Some(&t) if is_command(t) => break Some(t.to_string()),
                Some(_) => {
                    tokens.next();
                }
            }
        };
        let cmd = match cmd {
            Some(c) => c,
            None => continue, // blank or all-unknown line
        };
        tokens.next(); // consume the command token

        match cmd.as_str() {
            "uci" => {
                println!("id name FZChess-rs");
                println!("id author Zoltan FAZEKAS");
                // No tunable options are exposed yet.
                println!("uciok");
                flush();
            }
            "isready" => {
                // May arrive while searching; answer immediately without
                // stopping the search (spec).
                println!("readyok");
                flush();
            }
            "ucinewgame" => {
                stop_search(&mut searching);
                board = Board::new();
            }
            "position" => {
                // position [startpos | fen <6 fields>] [moves <m1> <m2> ...]
                stop_search(&mut searching);
                if let Some(b) = parse_position(tokens) {
                    board = b;
                }
            }
            "go" => {
                stop_search(&mut searching);
                let params = parse_go(tokens);
                searching = Some(start_search(&board, params));
            }
            "stop" => {
                stop_search(&mut searching);
            }
            "quit" => {
                stop_search(&mut searching);
                break;
            }
            _ => {} // recognised but unhandled: ignore
        }
    }
}

fn is_command(t: &str) -> bool {
    matches!(
        t,
        "uci" | "isready" | "ucinewgame" | "position" | "go" | "stop" | "quit"
    )
}

fn flush() {
    let _ = io::stdout().flush();
}

/// Signal any running search to stop and join it (blocks until it emits its
/// bestmove and exits). No-op if nothing is running.
fn stop_search(searching: &mut Option<Searching>) {
    if let Some(s) = searching.take() {
        s.stop.store(true, Ordering::Relaxed);
        let _ = s.handle.join();
    }
}

/// Parse a `position` command (tokens AFTER the `position` keyword) into a fresh
/// `Board`. Returns `None` if the command is malformed enough to ignore.
fn parse_position<'a, I>(mut tokens: std::iter::Peekable<I>) -> Option<Board>
where
    I: Iterator<Item = &'a str>,
{
    let mut board = Board::new();
    match tokens.next() {
        Some("startpos") => {
            board.reset_startpos();
        }
        Some("fen") => {
            // Collect the FEN fields up to (but not including) "moves".
            let mut fen = String::new();
            while let Some(&t) = tokens.peek() {
                if t == "moves" {
                    break;
                }
                if !fen.is_empty() {
                    fen.push(' ');
                }
                fen.push_str(t);
                tokens.next();
            }
            if fen.is_empty() {
                return None;
            }
            board.setboard(&fen);
        }
        _ => return None, // neither startpos nor fen: ignore
    }

    // Optional "moves <m1> <m2> ...": apply each, stopping at the first that is
    // not legal in the current position (spec is silent; Stockfish-style:
    // apply up to the invalid move, no error).
    if tokens.peek() == Some(&"moves") {
        tokens.next();
        for mv_str in tokens {
            if !apply_uci_move(&mut board, mv_str) {
                break;
            }
        }
    }
    Some(board)
}

/// Apply a single UCI move string (e.g. "e2e4", "e7e8q") if it is legal in the
/// current position. Returns false (and leaves the board unchanged) if the move
/// is unparseable or not legal.
fn apply_uci_move(board: &mut Board, mv_str: &str) -> bool {
    let len = mv_str.len();
    if len != 4 && len != 5 {
        return false;
    }
    if !mv_str.is_ascii() {
        return false;
    }
    let encoded = str2move(mv_str);

    // Validate against the legal move list for the side to move.
    board.list_legal_moves();
    let n = (board.legal_pointer + 1) as usize;
    let legal = &board.legal_moves[..n];
    if !legal.contains(&encoded) {
        return false;
    }
    board.update_table(encoded, false);
    board.invert_player_to_move();
    true
}

/// Parse `go` parameters (tokens AFTER the `go` keyword).
fn parse_go<'a, I>(mut tokens: std::iter::Peekable<I>) -> GoParams
where
    I: Iterator<Item = &'a str>,
{
    let mut p = GoParams::default();
    while let Some(tok) = tokens.next() {
        match tok {
            "depth" => p.depth = tokens.next().and_then(|v| v.parse().ok()),
            "movetime" => p.movetime = tokens.next().and_then(|v| v.parse().ok()),
            "infinite" => p.infinite = true,
            "wtime" => p.wtime = tokens.next().and_then(|v| v.parse().ok()),
            "btime" => p.btime = tokens.next().and_then(|v| v.parse().ok()),
            "winc" => p.winc = tokens.next().and_then(|v| v.parse().ok()),
            "binc" => p.binc = tokens.next().and_then(|v| v.parse().ok()),
            "movestogo" => p.movestogo = tokens.next().and_then(|v| v.parse().ok()),
            _ => {} // ignore unknown go sub-tokens
        }
    }
    p
}

/// Compute the soft time budget in ms from `go` params and the side to move.
/// Returns 0 for "no time limit" (depth-limited or infinite).
fn time_budget_ms(p: &GoParams, white_to_move: bool) -> u64 {
    if p.infinite || p.depth.is_some() {
        return 0;
    }
    if let Some(mt) = p.movetime {
        return mt;
    }
    // Basic clock management: time_left / movestogo (+ increment), default 40.
    let (time_left, inc) = if white_to_move {
        (p.wtime, p.winc.unwrap_or(0))
    } else {
        (p.btime, p.binc.unwrap_or(0))
    };
    if let Some(t) = time_left {
        let mtg = p.movestogo.unwrap_or(40).max(1);
        return t / mtg + inc;
    }
    0 // no clock info and no depth/movetime: treat as unlimited (depth cap below)
}

/// Spawn a background thread that searches `board` and prints `bestmove`.
fn start_search(board: &Board, params: GoParams) -> Searching {
    let stop = Arc::new(AtomicBool::new(false));
    let white_to_move = board.player_to_move == crate::types::WHITE;
    let budget = time_budget_ms(&params, white_to_move);
    // Depth cap: explicit depth, else a high ceiling for time/infinite searches.
    let gui_depth = params.depth.unwrap_or(64);

    // The worker needs its own board; clone the current position.
    let mut board_copy = board.clone();
    let stop_for_thread = Arc::clone(&stop);

    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let mut search = Search::with_stop(&mut board_copy, stop_for_thread);
        if budget != 0 {
            search.set_time_limit(budget);
        }
        let _ = ready_tx.send(()); // signal that the search has started
        search.make_move(gui_depth, 0, true);
    });
    // Ensure the thread has started before returning (keeps ordering sane).
    let _ = ready_rx.recv();

    Searching { stop, handle }
}

/// Build a `Board` from a full `position ...` command line (including the
/// leading `position` token). Exposed for testing the parser without the I/O
/// loop. Returns `None` if the command is malformed.
pub fn board_from_position_line(line: &str) -> Option<Board> {
    let mut tokens = line.split_whitespace().peekable();
    if tokens.peek() != Some(&"position") {
        return None;
    }
    tokens.next();
    parse_position(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_parses() {
        let b = board_from_position_line("position startpos").unwrap();
        assert_eq!(b.player_to_move, crate::types::WHITE);
        // Starting position FEN.
        assert!(b.get_fen().starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w"));
    }

    #[test]
    fn startpos_with_moves_applies_them() {
        let b = board_from_position_line("position startpos moves e2e4 e7e5 g1f3").unwrap();
        // After 1.e4 e5 2.Nf3 it's black to move; knight on f3.
        let fen = b.get_fen();
        assert!(fen.contains(" b "), "expected black to move: {fen}");
        // f3 square index = 1 + 5 + (2+2)*10 = 46; white knight = 0x02.
        assert_eq!(b.board[46], 0x02);
    }

    #[test]
    fn fen_parses_and_moves_apply() {
        let b = board_from_position_line(
            "position fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 moves e2e4",
        )
        .unwrap();
        assert!(b.get_fen().contains(" b "), "white moved, black to move");
    }

    #[test]
    fn illegal_move_in_list_stops_application() {
        // e2e4 legal, then "e2e4" again is illegal (pawn already moved); the
        // second is rejected and application stops — board is after 1.e4 only.
        let b = board_from_position_line("position startpos moves e2e4 e2e4").unwrap();
        let fen = b.get_fen();
        // Only one half-move applied => black to move.
        assert!(fen.contains(" b "), "expected one move applied: {fen}");
    }

    #[test]
    fn malformed_position_is_rejected() {
        assert!(board_from_position_line("position bogus").is_none());
        assert!(board_from_position_line("position fen").is_none());
        assert!(board_from_position_line("notposition startpos").is_none());
    }

    #[test]
    fn go_parsing_reads_params() {
        let toks = "depth 7 movetime 1234 wtime 60000 btime 50000 movestogo 20"
            .split_whitespace()
            .peekable();
        let p = parse_go(toks);
        assert_eq!(p.depth, Some(7));
        assert_eq!(p.movetime, Some(1234));
        assert_eq!(p.wtime, Some(60000));
        assert_eq!(p.movestogo, Some(20));
    }

    #[test]
    fn go_infinite_and_depth_mean_no_time_limit() {
        let p = parse_go("infinite".split_whitespace().peekable());
        assert_eq!(time_budget_ms(&p, true), 0);
        let p = parse_go("depth 5".split_whitespace().peekable());
        assert_eq!(time_budget_ms(&p, true), 0);
    }

    #[test]
    fn go_clock_budget_uses_movestogo() {
        let p = parse_go("wtime 40000 movestogo 20".split_whitespace().peekable());
        // 40000 / 20 = 2000 ms for white.
        assert_eq!(time_budget_ms(&p, true), 2000);
    }
}

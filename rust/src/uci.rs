//! UCI (Universal Chess Interface) command loop.
//!
//! This is a from-scratch, spec-conformant implementation rather than a port of
//! the C++ engine's ad-hoc `strstr`-based parser. Priorities (per the UCI spec):
//!   * tokenize on whitespace; arbitrary spacing is allowed;
//!   * unknown leading tokens are skipped and the rest of the line is parsed
//!     ("fail silently"), e.g. "joho go depth 2" still runs the go;
//!   * commands that don't apply are ignored (e.g. `stop` when not searching);
//!   * every `go` eventually produces a `bestmove`;
//!   * `d` is a non-standard debug command printing the position and option values.
//!
//! The search runs on a background thread so `stop` and `quit` stay responsive;
//! a shared `AtomicBool` is the stop signal. With `setoption name Threads value N`
//! (N > 1) that thread is the main searcher and N-1 silent Lazy SMP helper threads
//! search the same position, communicating only through the shared lock-free TT.

use crate::board::Board;
use crate::book::{self, PolyglotBook};
use crate::util::str2move;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
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

/// Default Polyglot book path, relative to the crate/run directory. Users may
/// override it via `setoption name BookFile value <path>`. The engine searches
/// normally when the file is absent, so this is purely a convenience default.
const DEFAULT_BOOK_FILE: &str = "performance.bin";

/// Run the UCI loop reading commands from stdin until `quit` (or EOF).
pub fn run() {
    let stdin = io::stdin();
    let mut board = Board::new();
    let mut searching: Option<Searching> = None;
    let mut threads: usize = 1; // search threads (Lazy SMP); setoption Threads
    let mut use_tt = true; // transposition table on by default; toggled via setoption UseTT

    // --- Opening book state ---
    // `own_book` gates book use; `book_best` selects highest-weight vs weighted
    // random; `book_file` is the path to load from; `book` holds the loaded
    // book (None until successfully loaded).
    //
    // The book is ON by default so the engine plays book moves out of the box
    // without any `setoption`. It is still fully opt-out via
    // `setoption name OwnBook value false`, and if the book file cannot be
    // found the engine simply searches (load failure is reported as an
    // `info string`). The actual load is deferred to the `uci` handshake so we
    // never print output before the GUI sends `uci` (UCI spec).
    let mut own_book = true;
    let mut book_best = false;
    let mut book_file = DEFAULT_BOOK_FILE.to_string();
    let mut book: Option<PolyglotBook> = None;

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
                // Custom option: toggle the transposition table (default on).
                println!("option name UseTT type check default true");
                println!("option name Threads type spin default 1 min 1 max 64");
                // Opening book options.
                println!("option name OwnBook type check default true");
                println!("option name BookFile type string default {DEFAULT_BOOK_FILE}");
                println!("option name BookBestMove type check default false");
                println!("uciok");
                flush();
                // Eagerly load the default book now (after the handshake, so
                // no output precedes `uciok`). Only when the book is enabled
                // and not already loaded.
                if own_book && book.is_none() {
                    book = try_load_book(&book_file);
                }
            }
            "setoption" => {
                // setoption name <id> [value <x>]. Recognised ids: UseTT, Threads,
                // OwnBook, BookFile, BookBestMove. Unknown ids are ignored.
                if let Some((name, value)) = parse_setoption(tokens) {
                    match name.as_str() {
                        n if n.eq_ignore_ascii_case("UseTT") => {
                            use_tt = value.eq_ignore_ascii_case("true");
                        }
                        n if n.eq_ignore_ascii_case("Threads") => {
                            if let Ok(v) = value.trim().parse::<usize>() {
                                threads = v.clamp(1, 64);
                            }
                        }
                        n if n.eq_ignore_ascii_case("OwnBook") => {
                            own_book = value.eq_ignore_ascii_case("true");
                            // Lazily load the current book file when enabling
                            // the book and nothing is loaded yet.
                            if own_book && book.is_none() {
                                book = try_load_book(&book_file);
                            }
                        }
                        n if n.eq_ignore_ascii_case("BookBestMove") => {
                            book_best = value.eq_ignore_ascii_case("true");
                        }
                        n if n.eq_ignore_ascii_case("BookFile") => {
                            book_file = value;
                            // Attempt to (re)load immediately so errors surface
                            // now; failure leaves the book unset.
                            book = try_load_book(&book_file);
                        }
                        _ => {} // unknown option id: ignore
                    }
                }
            }
            "isready" => {
                // May arrive while searching; answer immediately without
                // stopping the search (spec).
                println!("readyok");
                flush();
            }
            "d" => {
                // Non-standard debug command: show the current position and
                // the current option values (UCI itself has no way to read them).
                println!("fen: {}", board.get_fen());
                println!("searching: {}", searching.is_some());
                println!("options:");
                println!("  UseTT = {use_tt}");
                println!("  Threads = {threads}");
                println!("  OwnBook = {own_book}");
                println!("  BookFile = {book_file}");
                println!("  BookBestMove = {book_best}");
                match &book {
                    Some(b) => println!("book: loaded ({} entries)", b.len()),
                    None => println!("book: not loaded"),
                }
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
                // Try the opening book first: if enabled, loaded, and the
                // current position yields a legal book move, play it instantly
                // and skip the search entirely. Load lazily here too, in case a
                // front end issues `go` without the `uci` handshake.
                if own_book && book.is_none() {
                    book = try_load_book(&book_file);
                }
                let book_move = if own_book {
                    book.as_ref()
                        .and_then(|b| pick_book_move(&board, b, book_best))
                } else {
                    None
                };
                if let Some(mv) = book_move {
                    println!("bestmove {mv}");
                    flush();
                } else {
                    let params = parse_go(tokens);
                    searching = Some(start_search(&board, params, use_tt, threads));
                }
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
        "uci" | "setoption" | "isready" | "ucinewgame" | "position" | "go" | "stop" | "quit" | "d"
    )
}

/// Parse a `setoption name <id> [value <x>]` command (tokens AFTER the
/// `setoption` keyword). Returns `(name, value)` where `name` is the option id
/// (which may contain spaces) and `value` is the remaining tokens joined by
/// spaces (empty string if no `value` section). Returns `None` if the command
/// does not start with `name`.
fn parse_setoption<'a, I>(mut tokens: std::iter::Peekable<I>) -> Option<(String, String)>
where
    I: Iterator<Item = &'a str>,
{
    if tokens.next() != Some("name") {
        return None;
    }
    // The id may contain spaces; collect name tokens until "value".
    let mut name = String::new();
    for t in tokens.by_ref() {
        if t == "value" {
            break;
        }
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(t);
    }
    if name.is_empty() {
        return None;
    }
    // Remaining tokens form the value (may be empty for check/button options,
    // though GUIs normally send "value true"/"value false").
    let value: Vec<&str> = tokens.collect();
    Some((name, value.join(" ")))
}

/// Attempt to load a Polyglot book from `path`. On failure, log a note to
/// stderr (so it never pollutes the UCI stdout protocol) and return `None`.
/// This makes a missing/invalid book a no-op: the engine just searches.
/// Attempt to load a Polyglot book for the configured `path`.
///
/// Path resolution tries, in order:
///   1. `path` as given (absolute, or relative to the current working dir);
///   2. `path` resolved next to the running executable.
///
/// GUIs launch engines from an arbitrary working directory, so a bare relative
/// default like `performance.bin` must also be looked up beside the binary —
/// otherwise the book silently fails to load. Diagnostics are emitted as UCI
/// `info string` lines on stdout (the protocol channel GUIs read), and the
/// resolved path is reported on success. Returns `None` on failure (the engine
/// then simply searches), never panicking.
fn try_load_book(path: &str) -> Option<PolyglotBook> {
    for candidate in book_path_candidates(path) {
        match PolyglotBook::load(&candidate) {
            Ok(b) => {
                println!(
                    "info string loaded book '{}' ({} entries)",
                    candidate.display(),
                    b.len()
                );
                flush();
                return Some(b);
            }
            Err(_) => continue, // try the next candidate
        }
    }
    println!("info string could not load book '{path}' (searched working dir and executable dir)");
    flush();
    None
}

/// Candidate filesystem paths to try when loading `path`, in priority order:
/// the path as given, then the same file name resolved beside the executable.
/// Absolute paths yield a single candidate.
fn book_path_candidates(path: &str) -> Vec<std::path::PathBuf> {
    let p = std::path::PathBuf::from(path);
    let mut out = vec![p.clone()];
    // For a relative path, also probe the same file name beside the running
    // executable, so a GUI launching from an arbitrary cwd still finds a book
    // placed next to the binary.
    let exe_dir = if p.is_relative() {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    } else {
        None
    };
    if let Some(dir) = exe_dir {
        out.push(dir.join(&p));
    }
    out
}

/// Look up a book move for the current position.
///
/// Computes the Polyglot key from the board's FEN, probes the book (highest
/// weight when `best_only`, else weighted-random), decodes the move to UCI
/// coordinate notation, and returns it only if it is legal in the current
/// position. Returns `None` when the position is out of book or the decoded
/// move is somehow not legal (defensive: a corrupt/foreign book never makes
/// the engine play an illegal move).
pub fn pick_book_move(board: &Board, book: &PolyglotBook, best_only: bool) -> Option<String> {
    let key = book::polyglot_key(&board.get_fen());
    let mut rng = book::Rng::from_time();
    let raw = book.probe(key, best_only, &mut rng)?;
    let uci = book::decode_move(raw);
    if is_legal_uci_move(board, &uci) {
        Some(uci)
    } else {
        None
    }
}

/// Return true if `mv_str` is a legal move in `board`'s current position,
/// without mutating the board. Shares the validation logic with
/// `apply_uci_move`.
fn is_legal_uci_move(board: &Board, mv_str: &str) -> bool {
    let len = mv_str.len();
    if (len != 4 && len != 5) || !mv_str.is_ascii() {
        return false;
    }
    let encoded = str2move(mv_str);
    let mut probe = board.clone();
    probe.list_legal_moves();
    let n = (probe.legal_pointer + 1) as usize;
    probe.legal_moves[..n].contains(&encoded)
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
    if !is_legal_uci_move(board, mv_str) {
        return false;
    }
    let encoded = str2move(mv_str);
    // Populate the legal list on the real board (is_legal_uci_move used a
    // clone), then apply the validated move.
    board.list_legal_moves();
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
fn start_search(board: &Board, params: GoParams, use_tt: bool, threads: usize) -> Searching {
    let stop = Arc::new(AtomicBool::new(false));
    let white_to_move = board.player_to_move == crate::types::WHITE;
    let budget = time_budget_ms(&params, white_to_move);
    // Depth cap: explicit depth, else a high ceiling for time/infinite searches.
    let gui_depth = params.depth.unwrap_or(64);

    // The worker needs its own board; clone the current position.
    let board_copy = board.clone();
    let stop_for_thread = Arc::clone(&stop);

    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let _ = ready_tx.send(()); // signal that the search has started
        // Lazy SMP: this thread is the main searcher; `threads - 1` helpers
        // share the TT with it.
        crate::search::run_smp(
            &board_copy,
            threads,
            stop_for_thread,
            use_tt,
            budget,
            gui_depth,
        );
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
        assert!(
            b.get_fen()
                .starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w")
        );
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

    // --- setoption parser (generalized name + value) ---

    fn setopt(line: &str) -> Option<(String, String)> {
        // line is the text AFTER "setoption".
        parse_setoption(line.split_whitespace().peekable())
    }

    #[test]
    fn setoption_extracts_name_and_value() {
        assert_eq!(
            setopt("name UseTT value false"),
            Some(("UseTT".to_string(), "false".to_string()))
        );
        assert_eq!(
            setopt("name OwnBook value true"),
            Some(("OwnBook".to_string(), "true".to_string()))
        );
        assert_eq!(
            setopt("name BookBestMove value true"),
            Some(("BookBestMove".to_string(), "true".to_string()))
        );
    }

    #[test]
    fn setoption_bookfile_value_keeps_path() {
        // BookFile values can contain path separators and spaces.
        assert_eq!(
            setopt("name BookFile value /tmp/my book.bin"),
            Some(("BookFile".to_string(), "/tmp/my book.bin".to_string()))
        );
    }

    #[test]
    fn setoption_missing_name_is_none() {
        assert_eq!(setopt("bogus"), None);
        assert_eq!(setopt(""), None);
    }

    // --- pick_book_move end-to-end logic ---

    use crate::book::{Entry, PolyglotBook};

    /// Encode a 4-char UCI move (no promotion, no castling) into a Polyglot
    /// move word so a synthetic book can be hand-built from a UCI string.
    fn encode_polyglot(uci: &str) -> u16 {
        let b = uci.as_bytes();
        let ff = (b[0] - b'a') as u16;
        let fr = (b[1] - b'1') as u16;
        let tf = (b[2] - b'a') as u16;
        let tr = (b[3] - b'1') as u16;
        let from = fr * 8 + ff;
        let to = tr * 8 + tf;
        (from << 6) | to
    }

    #[test]
    fn pick_book_move_returns_legal_startpos_move() {
        let board = board_from_position_line("position startpos").unwrap();
        let key = crate::book::polyglot_key(&board.get_fen());
        // Single-entry book: startpos -> e2e4.
        let book = PolyglotBook {
            entries: vec![Entry {
                key,
                move_raw: encode_polyglot("e2e4"),
                weight: 100,
                learn: 0,
            }],
        };
        assert_eq!(pick_book_move(&board, &book, true), Some("e2e4".to_string()));
    }

    #[test]
    fn pick_book_move_out_of_book_returns_none() {
        let board = board_from_position_line("position startpos").unwrap();
        // Book keyed to a different (bogus) position: startpos not present.
        let book = PolyglotBook {
            entries: vec![Entry {
                key: 0xDEAD_BEEF_DEAD_BEEF,
                move_raw: encode_polyglot("e2e4"),
                weight: 100,
                learn: 0,
            }],
        };
        assert_eq!(pick_book_move(&board, &book, true), None);
    }

    #[test]
    fn pick_book_move_rejects_illegal_decoded_move() {
        let board = board_from_position_line("position startpos").unwrap();
        let key = crate::book::polyglot_key(&board.get_fen());
        // e2e5 is NOT a legal first move; the book move must be rejected and
        // the engine falls back to searching (None).
        let book = PolyglotBook {
            entries: vec![Entry {
                key,
                move_raw: encode_polyglot("e2e5"),
                weight: 100,
                learn: 0,
            }],
        };
        assert_eq!(pick_book_move(&board, &book, true), None);
    }

    #[test]
    fn try_load_book_missing_is_none() {
        assert!(try_load_book("/nonexistent/no-book-here.bin").is_none());
    }

    #[test]
    fn book_path_candidates_relative_includes_exe_dir() {
        // A relative path yields the path itself plus an executable-dir variant,
        // so a GUI launching from an arbitrary cwd can still find a book placed
        // beside the binary.
        let cands = book_path_candidates("performance.bin");
        assert_eq!(cands[0], std::path::PathBuf::from("performance.bin"));
        assert!(
            cands.len() >= 2,
            "relative path should also probe the executable dir"
        );
        assert!(
            cands[1].ends_with("performance.bin"),
            "exe-dir candidate must target the same file name"
        );
    }

    #[test]
    fn book_path_candidates_absolute_is_single() {
        let cands = book_path_candidates("/abs/path/book.bin");
        assert_eq!(cands, vec![std::path::PathBuf::from("/abs/path/book.bin")]);
    }
}

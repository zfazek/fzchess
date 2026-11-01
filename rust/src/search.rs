//! Alpha-beta search core, ported from C++ `Chess::alfabeta` and its helpers.
//!
//! The search owns the transposition table and all per-search scratch state
//! (node counter, killer moves, principal-variation lines, root move list),
//! borrowing the `Board` mutably for the duration. This matches the ownership
//! split we adopted during the port: the board holds position state, the search
//! holds search state.
//!
//! Scoring is negamax from the side-to-move's perspective. This implements the
//! recursion, MVV-LVA + killer-move ordering, a mate-aware quiescence search at
//! the horizon (resolves pending captures/checks to kill the horizon effect,
//! while still detecting checkmate/stalemate), draw detection, and PV
//! collection, plus the iterative-deepening driver and UCI output.

use crate::board::Board;
use crate::eval::{DRAW, LOST, NOT_END, WON};
use crate::tt::{TranspositionTable, TtFlag};
use crate::types::{MAX_LEGAL_MOVES, Move};
use crate::util::get_ms;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_PLY: usize = 128;
/// Initial `value` floor in alfabeta (matches C++ -22767).
const NEG_INF: i32 = -22767;

/// Safety cap on quiescence recursion depth (plies past the horizon). Capture
/// chains self-terminate (captures are finite) and check chains are rare, so
/// this is only a guardrail against pathological trees; on reaching it,
/// quiescence returns the stand-pat score.
const QS_MAX_PLY: i32 = 12;

/// A principal-variation line: `moves[1..=length]` is the line, `value` its score.
#[derive(Clone)]
struct Line {
    moves: [i32; MAX_LEGAL_MOVES],
    length: usize,
    value: i32,
}

impl Default for Line {
    fn default() -> Self {
        Line {
            moves: [0; MAX_LEGAL_MOVES],
            length: 0,
            value: 0,
        }
    }
}

pub struct Search<'a> {
    board: &'a mut Board,
    tt: TranspositionTable,
    pub tt_nodes: u64,
    pub nodes: u64,
    search_age: u8,

    /// Target depth of the current iterative-deepening iteration.
    depth: i32,
    /// Selective (quiescence) depth limit for the current iteration.
    seldepth: i32,
    last_ply: bool,

    killer_moves: [[i32; 2]; MAX_PLY],
    best_line: Vec<Line>, // indexed by ply
    curr_line: [i32; MAX_LEGAL_MOVES],
    /// Principal variation from the previous iterative-deepening iteration,
    /// indexed by ply (prev_pv[dpt] is the PV move at ply dpt). Tried first at
    /// each node still on the PV path, so the previous best line is re-searched
    /// first — the strongest form of "use last iteration's best move first".
    prev_pv: [i32; MAX_PLY],

    /// Root move list and ordering (for iterative deepening).
    pub root_moves: [Move; MAX_LEGAL_MOVES],
    pub nof_legal_root_moves: usize,

    pub best_move: i32,
    pub mate_score: i32,
    /// Number of root moves searched in the latest completed/active root evaluation (dpt == 1).
    pub root_moves_searched: usize,
    /// Toggle MVV-LVA ordering (C++ `sort_alfarray`, default on).
    pub sort_alfarray: bool,

    /// Summed material of the side to move at the root, used by eval as the
    /// end-game threshold (`sm`). Computed once per `make_move`. (The C++ engine
    /// intended this but left its `sm` field unassigned at 0 — a bug; the Rust
    /// engine computes it properly so the end-game eval terms engage correctly.)
    sm: i32,

    /// Shared stop flag, set by the UCI `stop`/`quit` commands from another
    /// thread. Polled periodically via `checkup`.
    stop: Arc<AtomicBool>,
    /// Set once the search decides to abort (time up or `stop`); causes the
    /// recursion to unwind cleanly (each level breaks before making a new move,
    /// so make/unmake stay balanced — no board corruption, unlike the C++
    /// exception approach).
    stop_search: bool,
    /// Wall-clock start of the current search and optional soft time limit in
    /// milliseconds (0 = no time limit, i.e. depth-limited or infinite).
    start_ms: u64,
    max_time_ms: u64,
}

impl<'a> Search<'a> {
    pub fn new(board: &'a mut Board) -> Self {
        Self::with_stop(board, Arc::new(AtomicBool::new(false)))
    }

    /// Construct a search sharing an external stop flag (used by the UCI loop so
    /// `stop`/`quit` on the main thread can abort a search running on a worker).
    pub fn with_stop(board: &'a mut Board, stop: Arc<AtomicBool>) -> Self {
        Search {
            board,
            tt: TranspositionTable::with_capacity(1 << 20),
            tt_nodes: 0,
            nodes: 0,
            search_age: 0,
            depth: 1,
            seldepth: 1,
            last_ply: false,
            killer_moves: [[0; 2]; MAX_PLY],
            best_line: vec![Line::default(); MAX_PLY],
            curr_line: [0; MAX_LEGAL_MOVES],
            prev_pv: [0; MAX_PLY],
            root_moves: [Move::default(); MAX_LEGAL_MOVES],
            nof_legal_root_moves: 0,
            best_move: 0,
            mate_score: 0,
            root_moves_searched: 0,
            sort_alfarray: true,
            sm: 0,
            stop,
            stop_search: false,
            start_ms: 0,
            max_time_ms: 0,
        }
    }

    /// Enable or disable the transposition table for this search (default on).
    /// Disabling makes the search behave as if no TT existed — useful for
    /// measuring the TT's effect or for strictly reproducible/pure search.
    pub fn set_tt_enabled(&mut self, enabled: bool) {
        self.tt.set_enabled(enabled);
    }

    /// MVV-LVA static move ordering. Port of C++ `Chess::sort_legal_moves`:
    /// captures (big victim, small attacker) and promotions rank ahead of quiet
    /// moves; sort is descending by score.
    fn sort_legal_moves(&mut self, nbr_legal: usize) {
        use crate::types::PIECE_VALUE;
        let mut scored: [Move; MAX_LEGAL_MOVES] = [Move::default(); MAX_LEGAL_MOVES];
        // Index-based: `i` addresses both the source move and the mailbox math.
        #[allow(clippy::needless_range_loop)]
        for i in 0..nbr_legal {
            let mv = self.board.legal_moves[i];
            let x_from = (mv & 0xe000) >> 13;
            let y_from = (mv & 0x1c00) >> 10;
            let x_to = (mv & 0x00e0) >> 5;
            let y_to = (mv & 0x001c) >> 2;
            let sq_from = (1 + x_from + (y_from + 2) * 10) as usize;
            let sq_to = (1 + x_to + (y_to + 2) * 10) as usize;
            let victim = (self.board.board[sq_to] & 127) as usize;
            let attacker = (self.board.board[sq_from] & 127) as usize;

            let mut score = 0;
            if victim != 0 {
                score = 100000 + PIECE_VALUE[victim] * 16 - PIECE_VALUE[attacker];
            }
            if (mv & 0x0303) != 0 {
                let promo_val = if (mv & 0x0100) == 0x0100 {
                    PIECE_VALUE[4] // rook
                } else if (mv & 0x0002) == 0x0002 {
                    PIECE_VALUE[3] // bishop
                } else if (mv & 0x0001) == 0x0001 {
                    PIECE_VALUE[2] // knight
                } else {
                    PIECE_VALUE[5] // queen
                };
                score += 100000 + promo_val;
            }
            scored[i] = Move { mv, value: score };
        }
        // Descending by score (C++ move_t::operator< sorts best-first). Stable
        // sort keeps equal-scored moves in generation order.
        scored[..nbr_legal].sort_by_key(|m| std::cmp::Reverse(m.value));
        for (dst, m) in self.board.legal_moves[..nbr_legal]
            .iter_mut()
            .zip(&scored[..nbr_legal])
        {
            *dst = m.mv;
        }
    }

    /// Reorder the root move list by previous-iteration value (descending).
    /// Port of C++ `Chess::calculate_evarray`.
    fn calculate_evarray(&mut self) {
        if self.depth == 1 {
            for i in 0..self.nof_legal_root_moves {
                self.root_moves[i] = Move {
                    mv: self.board.legal_moves[i],
                    value: 0,
                };
            }
        } else {
            // Selection-sort descending by value (matches the C++ bubble).
            for i in 0..self.nof_legal_root_moves {
                for j in (i + 1)..self.nof_legal_root_moves {
                    if self.root_moves[j].value > self.root_moves[i].value {
                        self.root_moves.swap(i, j);
                    }
                }
            }
        }
    }

    /// Negamax alpha-beta. Port of C++ `Chess::alfabeta`. Returns the score of
    /// the position from the side-to-move's perspective. `dpt` is the current
    /// ply (root == 1).
    pub fn alfabeta(&mut self, dpt: i32, mut alfa: i32, beta: i32, on_pv: bool) -> i32 {
        let mut value = NEG_INF;
        let mut alfarray: [i32; MAX_LEGAL_MOVES] = [0; MAX_LEGAL_MOVES];
        // Zobrist key of this position, for TT hash-move probing/storing.
        let key = self.board.movelist[self.board.move_number].zobrist_key;
        // Best move found at this node, stored into the TT for future ordering.
        let mut node_best_move = 0i32;
        let alfa_orig = alfa; // original alpha, to classify the node (Exact vs upper-bound)

        self.board.list_legal_moves();
        if self.board.legal_pointer == -1 {
            // No legal move for the side to move: stalemate (draw) or mated (lost).
            let king = if self.board.player_to_move == crate::types::WHITE {
                self.board.movelist[self.board.move_number].pos_white_king
            } else {
                self.board.movelist[self.board.move_number].pos_black_king
            };
            if !self.board.is_attacked(king, self.board.player_to_move) {
                return DRAW;
            } else {
                return LOST;
            }
        }
        let nbr_legal = (self.board.legal_pointer + 1) as usize;

        // Move ordering.
        if dpt == 1 {
            self.root_moves_searched = 0;
            self.nof_legal_root_moves = nbr_legal;
            self.calculate_evarray();
            #[allow(clippy::needless_range_loop)] // parallel index copy
            for i in 0..self.nof_legal_root_moves {
                alfarray[i] = self.root_moves[i].mv;
            }
        } else {
            if self.sort_alfarray {
                self.sort_legal_moves(nbr_legal);
            }
            alfarray[..nbr_legal].copy_from_slice(&self.board.legal_moves[..nbr_legal]);
            // Killer-move ordering: pull the two killers for this ply forward.
            if (dpt as usize) < MAX_PLY {
                let mut front = 0;
                for kslot in 0..2 {
                    let km = self.killer_moves[dpt as usize][kslot];
                    if km == 0 {
                        continue;
                    }
                    for i in front..nbr_legal {
                        if alfarray[i] == km {
                            if i != front {
                                alfarray.swap(front, i);
                            }
                            front += 1;
                            break;
                        }
                    }
                }
            }
            // Hash-move ordering (highest priority): if the TT has a best move
            // for this position from a previous (iteration or transposition)
            // visit, try it first. This is the main fix for ordering decay at
            // depth — MVV-LVA/killers can't rank quiet moves, but the hash move
            // carries forward what the search already found best here.
            let hash_move = self.tt.probe_move(key);
            if hash_move != 0 {
                for i in 0..nbr_legal {
                    if alfarray[i] == hash_move {
                        if i != 0 {
                            alfarray[..=i].rotate_right(1);
                        }
                        break;
                    }
                }
            }
        }

        // PV-move ordering (highest priority): while still on the previous
        // iteration's principal variation, try that PV move first at this ply.
        // This re-searches last iteration's best line before anything else —
        // the strongest ordering signal, applied at both root and interior.
        if on_pv {
            let pv_move = self.prev_pv[dpt as usize];
            if pv_move != 0 {
                for i in 0..nbr_legal {
                    if alfarray[i] == pv_move {
                        if i != 0 {
                            alfarray[..=i].rotate_right(1);
                        }
                        break;
                    }
                }
            }
        }

        // `i` is used for alfarray[i], root_moves[i], and currmovenumber i+1.
        #[allow(clippy::needless_range_loop)]
        for i in 0..nbr_legal {
            if dpt == 1 {
                self.root_moves_searched += 1;
            }
            // Poll the stop flag / time limit periodically. Checked BEFORE
            // making the move so that if we abort, no move is left pending at
            // this level and make/unmake stay balanced as the recursion unwinds.
            self.nodes += 1;
            if self.nodes & 1023 == 0 {
                self.checkup();
            }
            if self.stop_search {
                break;
            }
            // At the root, report the move currently being searched (UCI).
            // Disabled temporary
            // if dpt == 1 {
            //     println!(
            //         "info currmove {} currmovenumber {}",
            //         crate::util::move2str(alfarray[i]),
            //         i + 1
            //     );
            // }
            let u;
            self.board.update_table(alfarray[i], false);
            self.curr_line[dpt as usize] = alfarray[i];

            // Horizon: at the nominal depth, resolve pending tactics with a
            // quiescence search instead of scoring the raw position. This is
            // what eliminates the horizon effect — a position with a hanging
            // capture or an in-progress exchange must not be scored statically.
            //
            // The quiescence search is MATE-AWARE (this is the fix for the bug
            // where a forced mate sitting exactly on the horizon was missed):
            //   * it first tests for checkmate/stalemate (no legal move), and
            //   * when the side to move is in check it searches ALL evasions,
            //     not just captures,
            // so a mate delivered by a quiet move at the horizon is seen and
            // scored as a real mate (mate-distance-adjusted), then propagates
            // up the negamax tree. Draw conditions are checked first.
            if dpt >= self.depth {
                self.last_ply = true;
                if self.board.third_occurance()
                    || self.board.is_not_enough_material()
                    || self.board.movelist[self.board.move_number].not_pawn_move >= 100
                {
                    u = DRAW;
                } else {
                    // Negamax into quiescence from the opponent's side to move.
                    self.board.invert_player_to_move();
                    u = -self.quiescence(-beta, -alfa, dpt + 1, 0);
                    self.board.invert_player_to_move();
                }
                self.board.unmake_table(false);
            } else {
                // Draw detection before recursing.
                if self.board.third_occurance()
                    || self.board.is_not_enough_material()
                    || self.board.movelist[self.board.move_number].not_pawn_move >= 100
                {
                    u = DRAW;
                    self.board.unmake_table(false);
                } else {
                    let eg = self.board.evaluation_only_end_game(dpt);
                    if eg == NOT_END {
                        self.board.invert_player_to_move();
                        let child_on_pv = on_pv && alfarray[i] == self.prev_pv[dpt as usize];
                        u = -self.alfabeta(dpt + 1, -beta, -alfa, child_on_pv);
                        self.last_ply = false;
                        self.board.invert_player_to_move();
                    } else {
                        u = eg;
                    }
                    self.board.unmake_table(false);
                }
            }

            // If the search was aborted WHILE evaluating this move, `u` is the
            // result of an interrupted (incomplete) sub-search and is garbage —
            // in particular it can be the `NEG_INF` sentinel that an aborted
            // child returns, which would otherwise surface as a bogus near-mate
            // score. Discard it: break out WITHOUT updating value / best_line /
            // the root report. The caller (iterative deepening) will drop this
            // whole iteration and keep the last completed depth.
            if self.stop_search {
                break;
            }

            if dpt == 1 {
                self.root_moves[i].mv = alfarray[i];
                self.root_moves[i].value = u;
            }

            if u > value {
                value = u;
                node_best_move = alfarray[i];
                // Copy current line into best_line[dpt].
                let sd = self.seldepth as usize;
                for b in 1..=sd {
                    self.best_line[dpt as usize].moves[b] = self.curr_line[b];
                }
                self.best_line[dpt as usize].value = u;
                if self.last_ply {
                    self.best_line[dpt as usize].length = dpt as usize;
                } else {
                    let child_len = self.best_line[dpt as usize + 1].length;
                    let child_val = self.best_line[dpt as usize + 1].value;
                    self.best_line[dpt as usize].length = child_len;
                    self.best_line[dpt as usize].value = child_val;
                    for b in 1..=child_len {
                        self.best_line[dpt as usize].moves[b] =
                            self.best_line[dpt as usize + 1].moves[b];
                    }
                }
                if dpt == 1 {
                    self.best_move = alfarray[i];
                    // Only a mate the engine is DELIVERING (positive score)
                    // counts as `mate_score` and may short-circuit the search.
                    // A negative mate score means the engine is being mated —
                    // it must keep searching other root moves and deeper plies
                    // to find the move that escapes or best delays mate, not
                    // lock in the first losing line. (Bug fix: the old code used
                    // `u.abs()`, so being mated set a positive mate_score and
                    // broke iterative deepening instantly, returning a losing
                    // move without any real search.)
                    if u > 20000 {
                        self.mate_score = u;
                    } else {
                        self.mate_score = 0;
                    }
                    // Emit the multipv/score/pv line (C++ format). mate score
                    // conversion matches C++: uu = (22001-u)/2 for u>0 else
                    // -(u+22001)/2, clamped to +/-1 if 0.
                    let line_len = self.best_line[dpt as usize].length;
                    let mut pv = String::new();
                    for b in 1..=line_len {
                        pv.push_str(&crate::util::move2str(
                            self.best_line[dpt as usize].moves[b],
                        ));
                        pv.push(' ');
                    }
                    if u.abs() > 20000 {
                        let mut uu = if u > 0 {
                            (22001 - u) / 2
                        } else {
                            -(u + 22001) / 2
                        };
                        if uu == 0 {
                            uu = if u > 0 { 1 } else { -1 };
                        }
                        println!(
                            "info multipv 1 depth {} seldepth {} score mate {} nodes {} pv {}",
                            self.depth.max(dpt),
                            self.seldepth,
                            uu,
                            self.nodes,
                            pv
                        );
                        // Only stop examining other root moves when we have
                        // found a mate WE deliver (u > 0). If `u` is a losing
                        // mate, we must keep trying the remaining root moves —
                        // another move may avoid or delay the mate.
                        if u > 20000 {
                            break;
                        }
                    } else {
                        println!(
                            "info multipv 1 depth {} seldepth {} score cp {} nodes {} pv {}",
                            self.depth.max(dpt),
                            self.seldepth,
                            u,
                            self.nodes,
                            pv
                        );
                    }
                }
            }

            // Alpha-beta cutoff.
            if value >= beta {
                // Record a killer move if it was quiet (non-capturing). The move
                // was unmade, so the board holds the pre-move position; a capture
                // has an enemy piece on the destination.
                let km = alfarray[i];
                let x_to = (km & 0x00e0) >> 5;
                let y_to = (km & 0x001c) >> 2;
                let sq_to = (1 + x_to + (y_to + 2) * 10) as usize;
                let is_capture = self.board.board[sq_to] != crate::types::EMPTY
                    && self.board.board[sq_to] != crate::types::OFFBOARD;
                if !is_capture
                    && (dpt as usize) < MAX_PLY
                    && self.killer_moves[dpt as usize][0] != km
                {
                    self.killer_moves[dpt as usize][1] = self.killer_moves[dpt as usize][0];
                    self.killer_moves[dpt as usize][0] = km;
                }
                // Store a fail-high (lower-bound) entry with the cutoff move as
                // the hash move, so future visits try it first.
                if !self.stop_search {
                    self.tt.store(
                        key,
                        dpt as i16,
                        value as i16,
                        km,
                        TtFlag::Beta,
                        self.search_age,
                    );
                }
                return value;
            }
            if value > alfa {
                alfa = value;
            }
        }
        // Node fully searched: store the best move (hash move) for ordering on
        // future visits. Flag is Exact if we raised alpha, else Alpha (upper
        // bound). Skip if the search was aborted mid-node (value unreliable).
        if !self.stop_search && node_best_move != 0 {
            let flag = if value > alfa_orig {
                TtFlag::Exact
            } else {
                TtFlag::Alpha
            };
            self.tt.store(
                key,
                dpt as i16,
                value as i16,
                node_best_move,
                flag,
                self.search_age,
            );
        }
        value
    }

    /// Mate-aware quiescence search from the side-to-move's perspective.
    ///
    /// Purpose: eliminate the horizon effect. Scoring a position statically at
    /// the search horizon is unsound while captures/exchanges are still pending
    /// (the static eval would, e.g., count a "won" piece that is actually about
    /// to be recaptured). Quiescence extends the search along forcing lines
    /// until the position is quiet, then scores it.
    ///
    /// Correctness (the part the earlier capture-only version got wrong):
    ///   1. TERMINAL FIRST. Before standing pat we check for no-legal-move:
    ///      that is checkmate (score `-(WON - dpt)`, mate-distance adjusted so a
    ///      faster mate scores higher) or stalemate (`DRAW`). A capture-only
    ///      quiescence that skips this never sees a mate delivered by a quiet
    ///      move and mis-scores it as material — the original bug.
    ///   2. IN CHECK -> SEARCH ALL EVASIONS. When the side to move is in check
    ///      there is no "stand pat" option (you cannot decline to answer check),
    ///      so we search every legal move, not just captures. Out of check we
    ///      stand pat and search captures only, as usual.
    ///
    /// `dpt` is the current ply (for mate-distance scoring); `qply` counts plies
    /// past the horizon for the safety cap.
    fn quiescence(&mut self, mut alfa: i32, beta: i32, dpt: i32, qply: i32) -> i32 {
        self.nodes += 1;
        // Record the deepest ply actually reached (for the UCI `seldepth`).
        if dpt > self.seldepth {
            self.seldepth = dpt;
        }
        if self.nodes & 1023 == 0 {
            self.checkup();
        }
        if self.stop_search {
            return 0;
        }

        // Is the side to move in check? (No stand-pat option if so.)
        let king = if self.board.player_to_move == crate::types::WHITE {
            self.board.movelist[self.board.move_number].pos_white_king
        } else {
            self.board.movelist[self.board.move_number].pos_black_king
        };
        let in_check = self.board.is_attacked(king, self.board.player_to_move);

        // Terminal detection: generate legal moves up front so we can detect
        // checkmate / stalemate before anything else.
        self.board.list_legal_moves();
        let nbr = (self.board.legal_pointer + 1) as usize;
        if nbr == 0 {
            // No legal move: mate if in check (mate-distance adjusted so the
            // search prefers the quickest mate), else stalemate.
            return if in_check { -(WON - dpt) } else { DRAW };
        }

        // Stand-pat (only when NOT in check): the static eval is a lower bound
        // on this node's value because the side to move can decline to capture.
        if !in_check {
            let stand_pat = self.board.evaluation_material(dpt, self.sm);
            // Safety cap: stop extending and return the stand-pat score.
            if qply >= QS_MAX_PLY {
                return stand_pat;
            }
            if stand_pat >= beta {
                return beta;
            }
            if stand_pat > alfa {
                alfa = stand_pat;
            }
        } else if qply >= QS_MAX_PLY {
            // In check but past the safety cap: fall back to a static material
            // score rather than extending further.
            return self.board.evaluation_material(dpt, self.sm);
        }

        // Select the moves to search. Out of check: captures only. In check:
        // every legal move (all check evasions), so mates are detected and the
        // mated side is forced to try every escape.
        let en_passant = self.board.movelist[self.board.move_number].en_passant;
        let mut moves: [i32; MAX_LEGAL_MOVES] = [0; MAX_LEGAL_MOVES];
        let mut nmoves = 0usize;
        for i in 0..nbr {
            let mv = self.board.legal_moves[i];
            if in_check || self.is_capture(mv, en_passant) {
                moves[nmoves] = mv;
                nmoves += 1;
            }
        }
        // Order by MVV-LVA (quiet evasions sort last, which is fine).
        self.order_captures(&mut moves[..nmoves]);

        let mut best = if in_check { NEG_INF } else { alfa };
        for &mv in moves.iter().take(nmoves) {
            if self.stop_search {
                break;
            }
            self.board.update_table(mv, false);
            self.board.invert_player_to_move();
            let score = -self.quiescence(-beta, -alfa, dpt + 1, qply + 1);
            self.board.invert_player_to_move();
            self.board.unmake_table(false);
            if score > best {
                best = score;
            }
            if score >= beta {
                return score;
            }
            if score > alfa {
                alfa = score;
            }
        }
        // Out of check we may have had no captures (best == alfa == stand_pat
        // raised); in check `best` holds the best evasion score.
        best
    }

    /// Is `mv` a capture in the current position? A normal capture lands on a
    /// square holding an opponent piece; an en-passant capture lands on the
    /// (empty) en-passant target diagonally with a pawn.
    fn is_capture(&self, mv: i32, en_passant: i32) -> bool {
        let x_to = (mv & 0x00e0) >> 5;
        let y_to = (mv & 0x001c) >> 2;
        let sq_to = (1 + x_to + (y_to + 2) * 10) as usize;
        let dest = self.board.board[sq_to];
        if dest != crate::types::EMPTY && dest != crate::types::OFFBOARD {
            return true; // normal capture (legality already guarantees it's an enemy)
        }
        // En-passant: a pawn moving onto the en_passant target square.
        if en_passant > 1 && sq_to as i32 == en_passant {
            let x_from = (mv & 0xe000) >> 13;
            let y_from = (mv & 0x1c00) >> 10;
            let sq_from = (1 + x_from + (y_from + 2) * 10) as usize;
            let piece = self.board.board[sq_from] & 127;
            return piece == crate::types::PAWN;
        }
        false
    }

    /// Order a slice of moves by MVV-LVA (most valuable victim, least valuable
    /// attacker), descending. Quiet moves (no victim) score lowest and sort to
    /// the end — correct for the in-check evasion case.
    fn order_captures(&self, caps: &mut [i32]) {
        use crate::types::PIECE_VALUE;
        let board = &self.board;
        let score = |mv: i32| -> i32 {
            let x_from = (mv & 0xe000) >> 13;
            let y_from = (mv & 0x1c00) >> 10;
            let x_to = (mv & 0x00e0) >> 5;
            let y_to = (mv & 0x001c) >> 2;
            let sq_from = (1 + x_from + (y_from + 2) * 10) as usize;
            let sq_to = (1 + x_to + (y_to + 2) * 10) as usize;
            let victim = (board.board[sq_to] & 127) as usize;
            let attacker = (board.board[sq_from] & 127) as usize;
            PIECE_VALUE[victim] * 16 - PIECE_VALUE[attacker]
        };
        caps.sort_by_key(|&mv| std::cmp::Reverse(score(mv)));
    }

    /// Periodic abort check (called every ~1024 nodes). Sets `stop_search` if
    /// the external stop flag is set or the soft time limit has elapsed.
    fn checkup(&mut self) {
        if self.stop.load(Ordering::Relaxed) {
            self.stop_search = true;
            return;
        }
        if self.max_time_ms != 0 && get_ms().saturating_sub(self.start_ms) >= self.max_time_ms {
            self.stop_search = true;
        }
    }

    /// Iterative-deepening driver with UCI `info`/`bestmove` output.
    ///
    /// `gui_depth` caps the search depth (use a large value for effectively
    /// unlimited / time-controlled search). `break_if_mate_found` stops once a
    /// mate is found. Honors the shared stop flag and `max_time_ms` set via
    /// `set_time_limit`. Returns the best move from the last COMPLETED iteration.
    pub fn make_move(&mut self, gui_depth: i32, break_if_mate_found: bool) -> i32 {
        const MAX: i32 = 22767;
        self.search_age = self.search_age.wrapping_add(1);
        for p in 0..MAX_PLY {
            self.killer_moves[p] = [0, 0];
        }
        self.nodes = 0;
        self.tt_nodes = 0;
        self.stop_search = false;
        self.prev_pv = [0; MAX_PLY]; // no carried PV for a fresh search
        if self.start_ms == 0 {
            self.start_ms = get_ms();
        }

        // Ensure best_move is a legal move before searching.
        self.board.list_legal_moves();
        if self.board.legal_pointer >= 0 {
            self.best_move = self.board.legal_moves[0];
        }
        // End-game threshold: the side-to-move's summed material at the root,
        // used by eval to detect the end game. Computed once per search (the
        // C++ engine intended this but left it at 0 due to an uninitialised
        // field; the Rust engine computes it correctly).
        self.sm = self.board.sum_material(self.board.player_to_move);
        println!("info string position {}", self.board.get_fen());

        let mut depth = 1;
        loop {
            self.depth = depth;
            // seldepth is MEASURED: the deepest ply actually reached this
            // iteration (including quiescence). Start at the nominal depth and
            // let alfabeta/quiescence raise it. (The old fixed depth+N was
            // stale once quiescence replaced the seldepth-capped extension.)
            self.seldepth = depth;
            // Root is on the PV. Iteration 1 has no previous PV (prev_pv all 0),
            // so the on_pv ordering is a no-op until a PV has been collected.
            self.alfabeta(1, -MAX, MAX, true);

            if self.stop_search {
                // This iteration was interrupted; its best_move is unreliable.
                // Keep the result from the last fully completed depth.
                break;
            }

            // Carry this iteration's principal variation forward: copy the
            // completed best line into prev_pv[ply] so the next, deeper
            // iteration tries it first along the PV path.
            let len = self.best_line[1].length.min(MAX_PLY - 1);
            self.prev_pv = [0; MAX_PLY];
            for ply in 1..=len {
                self.prev_pv[ply] = self.best_line[1].moves[ply];
            }

            let elapsed = get_ms().saturating_sub(self.start_ms);
            let nps = (self.nodes * 1000).checked_div(elapsed).unwrap_or(0);
            println!(
                "info depth {} seldepth {} time {} nodes {} nps {}",
                depth, self.seldepth, elapsed, self.nodes, nps
            );

            if depth == 1 && self.nof_legal_root_moves == 1 {
                break;
            }
            if self.mate_score > 20000 && break_if_mate_found {
                break;
            }
            if depth >= gui_depth {
                break;
            }
            depth += 1;
        }

        println!("bestmove {}", crate::util::move2str(self.best_move));
        self.best_move
    }

    /// Set a soft time limit (ms) for the next `make_move`. 0 = no time limit.
    pub fn set_time_limit(&mut self, max_time_ms: u64) {
        self.start_ms = get_ms();
        self.max_time_ms = max_time_ms;
    }

    /// Convenience entry point for tests: run `make_move` at a fixed depth
    /// (with mate-break) and return the best move.
    pub fn search_fixed_depth(&mut self, target_depth: i32) -> i32 {
        self.make_move(target_depth, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::move2str;

    #[test]
    fn finds_a_legal_move_from_startpos() {
        let mut b = Board::new();
        let mut s = Search::new(&mut b);
        let mv = s.search_fixed_depth(3);
        assert_ne!(mv, 0, "search returned no move");
        // The returned move must be among the legal moves of the start position.
        b.list_legal_moves();
        let n = (b.legal_pointer + 1) as usize;
        let legal: Vec<i32> = b.legal_moves[..n].to_vec();
        assert!(legal.contains(&mv), "bestmove {} not legal", move2str(mv));
    }

    #[test]
    fn finds_mate_in_one() {
        // Simple back-rank mate in 1: white Rook a1 -> a8#. Black king h8, pawns
        // g7/h7 box it in; white king far away.
        // FEN: 6k1/5ppp/8/8/8/8/8/R6K w - - 0 1  -> Ra8 is mate.
        let mut b = Board::new();
        b.setboard("6k1/5ppp/8/8/8/8/8/R6K w - - 0 1");
        let mut s = Search::new(&mut b);
        let mv = s.search_fixed_depth(3);
        assert_eq!(
            move2str(mv),
            "a1a8",
            "expected mate-in-1 Ra8, got {}",
            move2str(mv)
        );
        assert!(
            s.mate_score > 20000,
            "mate not scored as mate: {}",
            s.mate_score
        );
    }

    #[test]
    fn tt_disabled_still_finds_mate() {
        // Correctness must not depend on the transposition table: with the TT
        // off the search still finds the same mate.
        let mut b = Board::new();
        b.setboard("6k1/5ppp/8/8/8/8/8/R6K w - - 0 1");
        let mut s = Search::new(&mut b);
        s.set_tt_enabled(false);
        let mv = s.search_fixed_depth(3);
        assert_eq!(
            move2str(mv),
            "a1a8",
            "TT-off: expected Ra8, got {}",
            move2str(mv)
        );
        assert_eq!(s.tt_nodes, 0, "TT-off should record zero TT hits");
    }

    #[test]
    fn finds_mate_in_three_on_the_horizon() {
        // Regression: a forced mate in 3 (5 plies) sitting exactly on the
        // depth-5 horizon. A capture-only quiescence that skips terminal
        // detection scored the (quiet) mating move as material and missed the
        // mate; the mate-aware quiescence finds it. Expected line:
        //   c4a5 c6b5 e6c4 b5a5 c4a4#
        let mut b = Board::new();
        b.setboard("rn5r/ppp3pp/2kbQn2/8/2N1p3/1P2B3/P1P2PPP/qN2K2R w K - 0 13");
        let mut s = Search::new(&mut b);
        let mv = s.search_fixed_depth(6);
        assert_eq!(
            move2str(mv),
            "c4a5",
            "expected mating move Nca5, got {}",
            move2str(mv)
        );
        assert!(
            s.mate_score > 20000,
            "mate not scored as mate: {}",
            s.mate_score
        );
        assert_eq!(
            s.depth, 5,
            "expected mate to be found by depth 5, but depth was {}",
            s.depth
        );
        // Verify that once mate was found at the root, the search returned immediately
        // (searching only 1 root move instead of all legal root moves).
        assert_eq!(
            s.root_moves_searched, 1,
            "expected search to return immediately after root move 1 found mate, but searched {} root moves",
            s.root_moves_searched
        );
    }

    #[test]
    fn losing_position_does_not_bail_early() {
        // Regression (lichess game UGFiq0JJ, move 26): in a LOST position
        // (Black has a forced mate), the engine used to set `mate_score` from
        // `u.abs()` — so being mated looked like delivering mate — and bail out
        // of iterative deepening after a shallow depth, returning a losing move
        // instantly (it played 26.Kf3?? into Qg4#). The search must instead
        // keep deepening and pick the most resistant move.
        //
        // This is the exact fatal position with White to move. We search to a
        // fixed depth and assert the engine actually reached that depth (did
        // not short-circuit) and did NOT report a positive mate_score (it is
        // being mated, not delivering mate).
        let mut b = Board::new();
        b.setboard("r7/6qk/1Q2B3/p1pP1p2/2P1n2P/4P3/P4PK1/R4R2 w - - 2 26");
        let mut s = Search::new(&mut b);
        let mv = s.search_fixed_depth(5);
        assert_eq!(
            s.depth, 5,
            "search bailed early at depth {} instead of reaching the requested depth 5 \
             (losing-mate scores must not trigger break_if_mate_found)",
            s.depth
        );
        assert_eq!(
            s.mate_score, 0,
            "engine is being mated here, so mate_score (a mate WE deliver) must be 0, got {}",
            s.mate_score
        );
        // And it must examine more than one root move (not lock in the first
        // losing line the way the inner root-loop break used to).
        assert!(
            s.root_moves_searched > 1,
            "expected the engine to weigh multiple root moves in a lost position, searched {}",
            s.root_moves_searched
        );
        // The returned move must be legal.
        b.list_legal_moves();
        let n = (b.legal_pointer + 1) as usize;
        assert!(
            b.legal_moves[..n].contains(&mv),
            "bestmove {} not legal",
            move2str(mv)
        );
    }

    #[test]
    fn losing_position_does_not_bail_early_black_to_move() {
        // Same regression as `losing_position_does_not_bail_early`, but with the
        // position vertically mirrored and colors swapped so BLACK is the side
        // being mated. The mate handling is negamax (scores are from the
        // side-to-move's perspective), so the fix must be color-symmetric:
        // Black being mated must likewise keep deepening rather than bail.
        let mut b = Board::new();
        b.setboard("r4r2/p4pk1/4p3/2p1N2p/P1Pp1P2/1q2b3/6QK/R7 b - - 2 26");
        let mut s = Search::new(&mut b);
        let mv = s.search_fixed_depth(5);
        assert_eq!(
            s.depth, 5,
            "black-to-move search bailed early at depth {} instead of depth 5",
            s.depth
        );
        assert_eq!(
            s.mate_score, 0,
            "black is being mated, so mate_score (a mate WE deliver) must be 0, got {}",
            s.mate_score
        );
        assert!(
            s.root_moves_searched > 1,
            "expected multiple root moves weighed in a lost position, searched {}",
            s.root_moves_searched
        );
        b.list_legal_moves();
        let n = (b.legal_pointer + 1) as usize;
        assert!(
            b.legal_moves[..n].contains(&mv),
            "bestmove {} not legal",
            move2str(mv)
        );
    }
}

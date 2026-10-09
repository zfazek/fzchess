//! Alpha-beta search core, ported from C++ `Chess::alfabeta` and its helpers.
//!
//! The search owns the transposition table and all per-search scratch state
//! (node counter, killer moves, principal-variation lines, root move list),
//! borrowing the `Board` mutably for the duration. This matches the ownership
//! split we adopted during the port: the board holds position state, the search
//! holds search state.
//!
//! Scoring is negamax from the side-to-move's perspective. This slice (task 4)
//! implements the recursion, MVV-LVA + killer-move ordering, quiescence via the
//! `further` field, draw detection, and PV collection. The iterative-deepening
//! driver and UCI `info`/`bestmove` output are layered on in task 5; here the
//! results are collected into the `Search` struct so they can be unit-tested.

use crate::board::Board;
use crate::eval::{DRAW, LOST, NOT_END};
use crate::tt::TranspositionTable;
use crate::types::{Move, MAX_LEGAL_MOVES};
use crate::util::get_ms;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MAX_PLY: usize = 128;
/// Initial `value` floor in alfabeta (matches C++ -22767).
const NEG_INF: i32 = -22767;

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

    /// Root move list and ordering (for iterative deepening).
    pub root_moves: [Move; MAX_LEGAL_MOVES],
    pub nof_legal_root_moves: usize,

    pub best_move: i32,
    pub mate_score: i32,
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
            root_moves: [Move::default(); MAX_LEGAL_MOVES],
            nof_legal_root_moves: 0,
            best_move: 0,
            mate_score: 0,
            sort_alfarray: true,
            sm: 0,
            stop,
            stop_search: false,
            start_ms: 0,
            max_time_ms: 0,
        }
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
    pub fn alfabeta(&mut self, dpt: i32, mut alfa: i32, beta: i32) -> i32 {
        let mut value = NEG_INF;
        let mut alfarray: [i32; MAX_LEGAL_MOVES] = [0; MAX_LEGAL_MOVES];

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
        }

        // `i` is used for alfarray[i], root_moves[i], and currmovenumber i+1.
        #[allow(clippy::needless_range_loop)]
        for i in 0..nbr_legal {
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
            if dpt == 1 {
                println!(
                    "info currmove {} currmovenumber {}",
                    crate::util::move2str(alfarray[i]),
                    i + 1
                );
            }
            let u;
            self.board.update_table(alfarray[i], false);
            self.curr_line[dpt as usize] = alfarray[i];

            // Leaf test: at target depth with no quiescence extension, or at the
            // selective-depth cap -> static evaluation.
            let further = self.board.movelist[self.board.move_number].further;
            if (dpt >= self.depth && further == 0) || dpt >= self.seldepth {
                self.last_ply = true;
                // e_legal_pointer is the mover's move count (legal_pointer was
                // overwritten by update_table's internal legality probing, so we
                // re-list before calling eval, as the C++ does inside evaluation).
                let elp = self.board.legal_pointer;
                let mut tt_nodes = self.tt_nodes;
                u = self.board.evaluation(
                    &mut self.tt,
                    &mut tt_nodes,
                    self.search_age,
                    elp,
                    dpt,
                    self.sm,
                );
                self.tt_nodes = tt_nodes;
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
                        u = -self.alfabeta(dpt + 1, -beta, -alfa);
                        self.last_ply = false;
                        self.board.invert_player_to_move();
                    } else {
                        u = eg;
                    }
                    self.board.unmake_table(false);
                }
            }

            if dpt == 1 {
                self.root_moves[i].mv = alfarray[i];
                self.root_moves[i].value = u;
            }

            if u > value {
                value = u;
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
                    if u.abs() > 20000 {
                        self.mate_score = u.abs();
                    } else {
                        self.mate_score = 0;
                    }
                    // Emit the multipv/score/pv line (C++ format). mate score
                    // conversion matches C++: uu = (22001-u)/2 for u>0 else
                    // -(u+22001)/2, clamped to +/-1 if 0.
                    let line_len = self.best_line[dpt as usize].length;
                    let mut pv = String::new();
                    for b in 1..=line_len {
                        pv.push_str(&crate::util::move2str(self.best_line[dpt as usize].moves[b]));
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
                return value;
            }
            if value > alfa {
                alfa = value;
            }
        }
        value
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
    /// unlimited / time-controlled search). `default_seldepth` is the quiescence
    /// offset (0 matches the C++ UCI path). `break_if_mate_found` stops once a
    /// mate is found. Honors the shared stop flag and `max_time_ms` set via
    /// `set_time_limit`. Returns the best move from the last COMPLETED iteration.
    pub fn make_move(
        &mut self,
        gui_depth: i32,
        default_seldepth: i32,
        break_if_mate_found: bool,
    ) -> i32 {
        const MAX: i32 = 22767;
        self.search_age = self.search_age.wrapping_add(1);
        for p in 0..MAX_PLY {
            self.killer_moves[p] = [0, 0];
        }
        self.nodes = 0;
        self.tt_nodes = 0;
        self.stop_search = false;
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

        let mut best_completed = self.best_move;
        let mut depth = 1;
        loop {
            self.depth = depth;
            self.seldepth = depth + default_seldepth;
            self.alfabeta(1, -MAX, MAX);

            if self.stop_search {
                // This iteration was interrupted; its best_move is unreliable.
                // Keep the result from the last fully completed depth.
                break;
            }
            // Iteration completed: commit its best move.
            best_completed = self.best_move;

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

        self.best_move = best_completed;
        println!("bestmove {}", crate::util::move2str(best_completed));
        best_completed
    }

    /// Set a soft time limit (ms) for the next `make_move`. 0 = no time limit.
    pub fn set_time_limit(&mut self, max_time_ms: u64) {
        self.start_ms = get_ms();
        self.max_time_ms = max_time_ms;
    }

    /// Convenience entry point for tests: run `make_move` at a fixed depth with
    /// the golden settings (`default_seldepth = 0`, `break_if_mate_found = true`)
    /// and return the best move.
    pub fn search_fixed_depth(&mut self, target_depth: i32) -> i32 {
        self.make_move(target_depth, 0, true)
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
        assert_eq!(move2str(mv), "a1a8", "expected mate-in-1 Ra8, got {}", move2str(mv));
        assert!(s.mate_score > 20000, "mate not scored as mate: {}", s.mate_score);
    }
}

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

const MAX_PLY: usize = 128;
/// Initial `value` floor in alfabeta (matches C++ -22767).
const NEG_INF: i32 = -22767;
/// End-game threshold material passed to eval. The C++ `sm` field is never
/// assigned and is 0 at runtime, so eval always runs with sm == 0 (this makes
/// the end-game king-distance and pawn-advance terms always active). Passing 0
/// here is required to reproduce the golden bestmoves — see the eval notes.
const SM: i32 = 0;

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
}

impl<'a> Search<'a> {
    pub fn new(board: &'a mut Board) -> Self {
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
        }
    }

    /// MVV-LVA static move ordering. Port of C++ `Chess::sort_legal_moves`:
    /// captures (big victim, small attacker) and promotions rank ahead of quiet
    /// moves; sort is descending by score.
    fn sort_legal_moves(&mut self, nbr_legal: usize) {
        use crate::types::PIECE_VALUE;
        let mut scored: [Move; MAX_LEGAL_MOVES] = [Move::default(); MAX_LEGAL_MOVES];
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
        // Descending by score (C++ move_t::operator< sorts best-first).
        scored[..nbr_legal].sort_by(|a, b| b.value.cmp(&a.value));
        for i in 0..nbr_legal {
            self.board.legal_moves[i] = scored[i].mv;
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
            for i in 0..self.nof_legal_root_moves {
                alfarray[i] = self.root_moves[i].mv;
            }
        } else {
            if self.sort_alfarray {
                self.sort_legal_moves(nbr_legal);
            }
            for i in 0..nbr_legal {
                alfarray[i] = self.board.legal_moves[i];
            }
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

        for i in 0..nbr_legal {
            self.nodes += 1;
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
                    SM,
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

    /// Run an iterative-deepening search up to `target_depth` and return the
    /// best move. Iterating from depth 1 is required so the root move list
    /// (`root_moves`) is populated and ordered for `calculate_evarray` at
    /// deeper iterations. The full driver + UCI output is task 5; this is the
    /// task-4 entry point used by tests.
    pub fn search_fixed_depth(&mut self, target_depth: i32) -> i32 {
        self.search_age = self.search_age.wrapping_add(1);
        for p in 0..MAX_PLY {
            self.killer_moves[p] = [0, 0];
        }
        self.best_move = 0;
        let max = 22767;
        for d in 1..=target_depth {
            self.depth = d;
            self.seldepth = d + 4;
            self.nodes = 0;
            self.tt_nodes = 0;
            self.alfabeta(1, -max, max);
        }
        self.best_move
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

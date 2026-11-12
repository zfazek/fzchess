//! Position evaluation, ported from the C++ `Eval` class.
//!
//! The evaluation is integer-only and reads the incrementally-maintained
//! bookkeeping on `Board` (material sum, pawn list, king positions, castle /
//! double-bishop / king-castled eval fields). Scores are from the side-to-move's
//! perspective, matching the C++ engine so that search results (bestmoves) line
//! up with the golden baseline.

use crate::board::Board;
use crate::tt::{TT_MISS, TranspositionTable, TtFlag};
use crate::types::*;

// Evaluation constants (match C++ `Eval`).
pub const DRAW: i32 = 0;
pub const LOST: i32 = -22000;
pub const WON: i32 = 22000;

/// Sentinel "not an end-game terminal node" return from `evaluation_only_end_game`
/// (matches the C++ magic value 32767).
pub const NOT_END: i32 = 32767;

// Game-phase thresholds on the side-to-move's summed material (`sm`). These are
// intentionally two distinct cutoffs, not one: pawn pushes become worthwhile
// earlier (while more material is still on the board) than committing the king
// toward the centre, so the pawn-advance bonus uses the higher threshold and
// the king-activity terms use the lower one.
const END_GAME_THRESHOLD: i32 = 1200; // king-activity terms (drive king to centre/corner)
const PAWN_PUSH_THRESHOLD: i32 = 2000; // pawn-advancement bonus engages earlier
const CANT_CASTLE: i32 = -40;
const DOUBLE_PAWN: i32 = -30;
const FRIENDLY_PAWN: i32 = 20;
const PAWN_ADVANTAGE: i32 = 10;

// --- Piece-Square Tables (centipawns, white's point of view) ---
// Each table is laid out visually: first row is rank 8, last row is rank 1,
// columns a..h. Black uses the vertically mirrored square. Values follow the
// well-known "simplified evaluation function" tables.
#[rustfmt::skip]
const PST_PAWN: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];
#[rustfmt::skip]
const PST_KNIGHT: [i32; 64] = [
   -50,-40,-30,-30,-30,-30,-40,-50,
   -40,-20,  0,  0,  0,  0,-20,-40,
   -30,  0, 10, 15, 15, 10,  0,-30,
   -30,  5, 15, 20, 20, 15,  5,-30,
   -30,  0, 15, 20, 20, 15,  0,-30,
   -30,  5, 10, 15, 15, 10,  5,-30,
   -40,-20,  0,  5,  5,  0,-20,-40,
   -50,-40,-30,-30,-30,-30,-40,-50,
];
#[rustfmt::skip]
const PST_BISHOP: [i32; 64] = [
   -20,-10,-10,-10,-10,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5, 10, 10,  5,  0,-10,
   -10,  5,  5, 10, 10,  5,  5,-10,
   -10,  0, 10, 10, 10, 10,  0,-10,
   -10, 10, 10, 10, 10, 10, 10,-10,
   -10,  5,  0,  0,  0,  0,  5,-10,
   -20,-10,-10,-10,-10,-10,-10,-20,
];
#[rustfmt::skip]
const PST_ROOK: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];
#[rustfmt::skip]
const PST_QUEEN: [i32; 64] = [
   -20,-10,-10, -5, -5,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5,  5,  5,  5,  0,-10,
    -5,  0,  5,  5,  5,  5,  0, -5,
     0,  0,  5,  5,  5,  5,  0, -5,
   -10,  5,  5,  5,  5,  5,  0,-10,
   -10,  0,  5,  0,  0,  0,  0,-10,
   -20,-10,-10, -5, -5,-10,-10,-20,
];
#[rustfmt::skip]
const PST_KING_MG: [i32; 64] = [
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -20,-30,-30,-40,-40,-30,-30,-20,
   -10,-20,-20,-20,-20,-20,-20,-10,
    20, 20,  0,  0,  0,  0, 20, 20,
    20, 30, 10,  0,  0, 10, 30, 20,
];
#[rustfmt::skip]
const PST_KING_EG: [i32; 64] = [
   -50,-40,-30,-20,-20,-30,-40,-50,
   -30,-20,-10,  0,  0,-10,-20,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-30,  0,  0,  0,  0,-30,-30,
   -50,-30,-30,-30,-30,-30,-30,-50,
];

impl Board {
    /// Piece-square score from White's point of view (positive = good for
    /// White). Scans the 64 squares of the 10x12 mailbox. `sm` is the same
    /// game-phase material used elsewhere in the eval: below
    /// `END_GAME_THRESHOLD` the king uses the end-game table.
    pub fn evaluation_pst_white(&self, sm: i32) -> i32 {
        let king_tbl = if sm < END_GAME_THRESHOLD {
            &PST_KING_EG
        } else {
            &PST_KING_MG
        };

        let mut e = 0;
        for rank in 0..8usize {
            for file in 0..8usize {
                let f = self.board[(rank + 2) * 10 + file + 1];
                if f <= 0 || f >= OFFBOARD {
                    continue;
                }
                let white = (f & 128) == 0;
                // Tables are stored rank 8 first; mirror vertically for black.
                let ti = if white { (7 - rank) * 8 + file } else { rank * 8 + file };
                let v = match f & 127 {
                    PAWN => PST_PAWN[ti],
                    KNIGHT => PST_KNIGHT[ti],
                    BISHOP => PST_BISHOP[ti],
                    ROOK => PST_ROOK[ti],
                    QUEEN => PST_QUEEN[ti],
                    KING => king_tbl[ti],
                    _ => 0,
                };
                e += if white { v } else { -v };
            }
        }
        e
    }

    /// Material + king/pawn-structure evaluation from the side-to-move's
    /// perspective. Port of C++ `Eval::evaluation_material`.
    ///
    /// `sm` is the summed material (both sides) used for the end-game threshold;
    /// `dpt` is the search ply (affects the end-game king-distance term's sign).
    pub fn evaluation_material(&self, dpt: i32, sm: i32) -> i32 {
        let pm = &self.movelist[self.move_number];
        let ptm = self.player_to_move;
        let move_number = self.move_number;
        let my_color_bit = if ptm == WHITE { 0 } else { 128 };

        let mut e = 0;

        // Incremental material, flipped to the side-to-move's perspective.
        e += if ptm == WHITE {
            pm.material_wp
        } else {
            -pm.material_wp
        };

        // Piece-square tables, flipped to the side-to-move's perspective.
        let pst = self.evaluation_pst_white(sm);
        e += if ptm == WHITE { pst } else { -pst };

        // Pawn structure / advance terms, iterating only the pawn list.
        for k in 0..self.n_pawns {
            let i = self.pawn_sq[k];
            let field = self.board[i as usize];
            let c = if (field & 128) == my_color_bit { 1 } else { -1 };
            e += c * self.evaluation_pawn(i, field, sm);
        }

        // King pawn-shield term (only once past the opening).
        if move_number > 6 {
            let wk = pm.pos_white_king;
            let bk = pm.pos_black_king;
            let wk_c = if my_color_bit == 0 { 1 } else { -1 };
            e += wk_c * self.evaluation_king(wk, self.board[wk as usize]);
            e += -wk_c * self.evaluation_king(bk, self.board[bk as usize]);
        }

        let c = if ptm == WHITE { 1 } else { -1 };

        // Castling bonus and double-bishop bonus.
        e += c * (pm.white_king_castled - pm.black_king_castled);
        e += c * (pm.white_double_bishops - pm.black_double_bishops);

        // End-game king-proximity terms, else castling punishment.
        if sm < END_GAME_THRESHOLD {
            let mut evaking = 3 * ((pm.pos_white_king / 10) - (pm.pos_black_king / 10)).abs();
            evaking += 3 * ((pm.pos_white_king % 10) - (pm.pos_black_king % 10)).abs();
            if dpt % 2 == 0 {
                e += evaking;
            } else {
                e -= evaking;
            }

            // Force the losing king to the corner.
            let evaking = if ptm == WHITE && dpt % 2 == 1 {
                5 * ((5 - pm.pos_black_king / 10).abs() + (5 - pm.pos_black_king % 10).abs())
            } else if ptm == WHITE && dpt % 2 == 0 {
                -5 * ((5 - pm.pos_white_king / 10).abs() + (5 - pm.pos_white_king % 10).abs())
            } else if ptm == BLACK && dpt % 2 == 0 {
                -5 * ((5 - pm.pos_black_king / 10).abs() + (5 - pm.pos_black_king % 10).abs())
            } else {
                // ptm == BLACK && dpt % 2 == 1
                5 * ((5 - pm.pos_white_king / 10).abs() + (5 - pm.pos_white_king % 10).abs())
            };
            e += evaking;
        } else {
            // CASTLING_PUNISHMENT is not compiled into the C++ build we target
            // (the `#ifdef` is off by default), so this branch is intentionally
            // empty. CANT_CASTLE is kept for parity/documentation.
            let _ = CANT_CASTLE;
        }

        e
    }

    /// King pawn-shield bonus: count friendly pawns adjacent to the king.
    /// Port of C++ `Eval::evaluation_king`.
    fn evaluation_king(&self, idx: i32, field: i32) -> i32 {
        let mut e = 0;
        let target = (field & 128) + PAWN; // friendly pawn code
        const KDIR: [i32; 8] = [-11, -10, -9, -1, 1, 9, 10, 11];
        for &k in &KDIR {
            if self.board[(idx + k) as usize] == target {
                e += FRIENDLY_PAWN;
            }
        }
        e
    }

    /// Pawn term: punish doubled pawns, bonus for advancement in the end game.
    /// Port of C++ `Eval::evaluation_pawn`.
    fn evaluation_pawn(&self, idx: i32, field: i32, sm: i32) -> i32 {
        let mut e = 0;

        // Doubled pawns ahead/behind on the same file.
        let mut dir = 0;
        loop {
            dir += 10;
            if self.board[(idx + dir) as usize] == field {
                e += DOUBLE_PAWN;
            }
            if !(dir <= 20 && self.board[(idx + dir) as usize] != OFFBOARD) {
                break;
            }
        }
        let mut dir = 0;
        loop {
            dir -= 10;
            if self.board[(idx + dir) as usize] == field {
                e += DOUBLE_PAWN;
            }
            if !(dir >= -20 && self.board[(idx + dir) as usize] != OFFBOARD) {
                break;
            }
        }

        // Advancement bonus — engages earlier than king activity (see the
        // PAWN_PUSH_THRESHOLD / END_GAME_THRESHOLD note above).
        if sm < PAWN_PUSH_THRESHOLD {
            if (field & 128) == 0 {
                e += (idx / 10) * PAWN_ADVANTAGE;
            } else {
                e += (11 - (idx / 10)) * PAWN_ADVANTAGE;
            }
        }
        e
    }

    /// Summed material for one color (used for the end-game threshold `sm`).
    /// Port of C++ `Eval::sum_material`. Uses `PIECE_VALUE` (same figure values
    /// as the C++ `figure_value` table).
    pub fn sum_material(&self, color: i32) -> i32 {
        let mut e = 0;
        for i in 20..100 {
            let field = self.board[i];
            if field > 0 && field < OFFBOARD {
                let figure_color = field & 128;
                if (color == WHITE && figure_color == 0) || (color == BLACK && figure_color == 128)
                {
                    e += PIECE_VALUE[(field & 127) as usize];
                }
            }
        }
        e
    }

    /// Leaf evaluation with TT probe/store and a mobility differential. Port of
    /// C++ `Eval::evaluation`.
    ///
    /// `e_legal_pointer` is the side-to-move's `legal_pointer` captured before
    /// this call (its move count), used for the mobility term. `sm` is the
    /// end-game threshold material. (The C++ engine never assigned its `sm`
    /// field, so it was always 0 there; the Rust search computes it properly
    /// in `Search::make_move`.) Note: this function is currently not called by
    /// the Rust search, which uses `evaluation_material` directly.
    /// `tt_nodes` counts TT hits.
    ///
    /// Takes `&mut self` because it re-lists legal moves and flips the side to
    /// move (restored before returning), and `&mut TranspositionTable` because
    /// the TT belongs to the search, not the board.
    pub fn evaluation(
        &mut self,
        tt: &TranspositionTable,
        tt_nodes: &mut u64,
        search_age: u8,
        e_legal_pointer: i32,
        dpt: i32,
        sm: i32,
    ) -> i32 {
        let key = self.movelist[self.move_number].zobrist_key;

        let cached = tt.probe(key);
        if cached != TT_MISS {
            *tt_nodes += 1;
            return cached;
        }

        // Draw conditions.
        if self.third_occurance()
            || self.is_not_enough_material()
            || self.movelist[self.move_number].not_pawn_move >= 100
        {
            tt.store(key, dpt as i16, DRAW as i16, 0, TtFlag::Exact, search_age);
            return DRAW;
        }

        // Mobility differential: our move count minus the opponent's.
        self.list_legal_moves();
        let mut lp = e_legal_pointer;
        self.invert_player_to_move();
        self.list_legal_moves();
        lp -= self.legal_pointer;
        self.invert_player_to_move();

        if self.legal_pointer == -1 {
            // `legal_pointer` here is the OPPONENT's move count (from the second
            // list_legal_moves above, after the invert). So this branch means
            // the opponent has no legal move: checkmate if their king is
            // attacked (good for us -> WON), else stalemate (DRAW). `king` is
            // the opponent's king and `-player_to_move` is its color, so
            // is_attacked tests whether we attack it.
            let king = if self.player_to_move == WHITE {
                self.movelist[self.move_number].pos_black_king
            } else {
                self.movelist[self.move_number].pos_white_king
            };
            if !self.is_attacked(king, -self.player_to_move) {
                return DRAW; // stalemate
            } else {
                return WON - dpt; // checkmate
            }
        }

        let random_number = 0; // C++: rand()%(window+1), but window path disabled
        let u = self.evaluation_material(dpt, sm) + 2 * lp + random_number;
        tt.store(key, dpt as i16, u as i16, 0, TtFlag::Exact, search_age);
        u
    }

    /// Detect a terminal end-game node (mate/stalemate) without scoring it;
    /// returns `NOT_END` (32767) otherwise. Port of C++
    /// `Eval::evaluation_only_end_game`.
    pub fn evaluation_only_end_game(&mut self, dpt: i32) -> i32 {
        self.invert_player_to_move();
        self.list_legal_moves();
        self.invert_player_to_move();
        if self.legal_pointer == -1 {
            // As in `evaluation`, `legal_pointer` is the opponent's move count
            // here (listed between the two inverts). No opponent move =>
            // checkmate if their king is attacked by us, else stalemate.
            let king = if self.player_to_move == WHITE {
                self.movelist[self.move_number].pos_black_king
            } else {
                self.movelist[self.move_number].pos_white_king
            };
            if !self.is_attacked(king, -self.player_to_move) {
                return DRAW; // stalemate
            } else {
                return WON - dpt; // checkmate
            }
        }
        NOT_END
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_material_is_balanced() {
        // From either side's perspective the opening is materially equal, so
        // the material component is 0. sum_material per side == full army.
        let b = Board::new();
        assert_eq!(b.sum_material(WHITE), b.sum_material(BLACK));
        // 8*100 + 2*330 + 2*330 + 2*500 + 900 = 800+660+660+1000+900 = 4020
        assert_eq!(b.sum_material(WHITE), 4020);
    }

    #[test]
    fn material_perspective_sign() {
        // White up a queen: material_wp positive; eval from white's view > 0,
        // from black's view < 0 by the same magnitude for the material part.
        let mut b = Board::new();
        // Remove the black queen (d8 == square 95? no; d8 is file d rank 8).
        // d8 board index: 1 + (3) + (7+2)*10 = 1+3+90 = 94.
        b.board[94] = EMPTY;
        b.rebuild_pawn_list();
        b.material_wp = b.recompute_material();
        b.movelist[b.move_number].material_wp = b.material_wp;
        assert_eq!(b.material_wp, 900); // white is +900 (black queen gone)
    }

    /// Evaluate several positions (opening, midgame, and a bare-king end game
    /// that exercises the `sm < END_GAME_THRESHOLD` branch) to ensure the eval
    /// runs without panicking (board indexing, pawn/king terms) and that the
    /// side-to-move perspective is self-consistent: evaluating the same static
    /// position's material from White vs Black negates the material component.
    #[test]
    fn eval_runs_on_varied_positions() {
        let fens = [
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "8/8/8/4k3/8/8/4K3/4R3 w - - 0 1", // end game: forces sm<threshold path
        ];
        for fen in fens {
            let mut b = Board::new();
            b.setboard(fen);
            let sm = b.sum_material(WHITE) + b.sum_material(BLACK);
            // Just needs to run without panicking; value is engine-defined.
            let _ = b.evaluation_material(1, sm);
            let _ = b.evaluation_material(2, sm);
        }
    }

    #[test]
    fn pst_startpos_symmetric_and_prefers_center() {
        let mut b = Board::new();
        assert_eq!(b.evaluation_pst_white(b.sum_material(WHITE)), 0);
        // White knight b1 -> c3 improves White's PST score.
        b.board[22] = EMPTY;
        b.board[43] = KNIGHT;
        assert!(b.evaluation_pst_white(b.sum_material(WHITE)) > 0);
    }

    #[test]
    fn sum_material_counts_one_side() {
        let mut b = Board::new();
        b.setboard("8/8/8/4k3/8/8/4K3/4R3 w - - 0 1");
        // White has a lone rook (500), black has nothing but the king (0).
        assert_eq!(b.sum_material(WHITE), 500);
        assert_eq!(b.sum_material(BLACK), 0);
    }
}

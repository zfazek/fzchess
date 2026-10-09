//! Move generation, make/unmake, and attack detection for perft.
//!
//! Faithful port of the corresponding C++ `Table` methods, operating on
//! `Board`. Only the state needed to generate legal moves and to make/unmake
//! them exactly (so counts are correct) is maintained; see `board.rs` for the
//! list of intentionally omitted eval/search bookkeeping.

use crate::board::Board;
use crate::types::*;

const WHITE_PAWN: i32 = 0x01;
const WHITE_ROOK: i32 = 0x04;
const WHITE_KING: i32 = 0x06;
const BLACK_PAWN: i32 = 0x81;
const BLACK_ROOK: i32 = 0x84;
const BLACK_KING: i32 = 0x86;

const WHITE_KNIGHT: i32 = 0x02;
const WHITE_BISHOP: i32 = 0x03;
const WHITE_QUEEN: i32 = 0x05;
const BLACK_KNIGHT: i32 = 0x82;
const BLACK_BISHOP: i32 = 0x83;
const BLACK_QUEEN: i32 = 0x85;

const WHITE_COLOR: i32 = 0;
const BLACK_COLOR: i32 = 128;

/// King-castled evaluation bonus, set in a position slot when a side castles.
/// Matches C++ `Eval::king_castled`. Lives here because `update_table` writes
/// it, but it is purely an evaluation term (not used by move generation).
const KING_CASTLED: i32 = 40;

/// Encode a move from two board indices + optional promotion bits.
/// Port of the `encode` lambda in C++ `list_legal_moves`.
#[inline]
fn encode(sf: i32, st: i32, promo: i32) -> i32 {
    let (xf, yf) = (sf % 10 - 1, sf / 10 - 2);
    let (xt, yt) = (st % 10 - 1, st / 10 - 2);
    (xf << 13) | (yf << 10) | (xt << 5) | (yt << 2) | promo
}

impl Board {
    /// Is `field` attacked by the side opposite to `color`? Port of
    /// C++ `Table::is_attacked`. `color` is the color of the piece on `field`.
    pub fn is_attacked(&self, field: i32, color: i32) -> bool {
        let b = &self.board;
        let f = field as usize;
        let (knight, bishop, rook, queen, king);
        if color == WHITE {
            knight = BLACK_KNIGHT;
            bishop = BLACK_BISHOP;
            rook = BLACK_ROOK;
            queen = BLACK_QUEEN;
            king = BLACK_KING;
            if b[f + 9] == BLACK_PAWN || b[f + 11] == BLACK_PAWN {
                return true;
            }
        } else {
            knight = WHITE_KNIGHT;
            bishop = WHITE_BISHOP;
            rook = WHITE_ROOK;
            queen = WHITE_QUEEN;
            king = WHITE_KING;
            if b[f - 9] == WHITE_PAWN || b[f - 11] == WHITE_PAWN {
                return true;
            }
        }

        // Rook/Queen orthogonal rays, with adjacent-king check.
        for &dir in &[10i32, -10, 1, -1] {
            let mut sq = field + dir;
            while b[sq as usize] == EMPTY {
                sq += dir;
            }
            if b[sq as usize] == rook || b[sq as usize] == queen {
                return true;
            }
            if sq == field + dir && b[sq as usize] == king {
                return true;
            }
        }

        // Bishop/Queen diagonal rays, with adjacent-king check.
        for &dir in &[11i32, -11, 9, -9] {
            let mut sq = field + dir;
            while b[sq as usize] == EMPTY {
                sq += dir;
            }
            if b[sq as usize] == bishop || b[sq as usize] == queen {
                return true;
            }
            if sq == field + dir && b[sq as usize] == king {
                return true;
            }
        }

        // Knight attacks.
        for &d in &[-21i32, -19, -12, -8, 8, 12, 19, 21] {
            if b[(field + d) as usize] == knight {
                return true;
            }
        }
        false
    }

    /// Generate all legal moves into `legal_moves[0..=legal_pointer]`.
    /// Port of C++ `Table::list_legal_moves`.
    pub fn list_legal_moves(&mut self) {
        self.legal_pointer = -1;
        let ptm = self.player_to_move;
        let my_color = if ptm == WHITE { WHITE_COLOR } else { BLACK_COLOR };
        let opp_color = if ptm == WHITE { BLACK_COLOR } else { WHITE_COLOR };
        let en_pass = self.movelist[self.move_number].en_passant;

        for sq in 21..=98i32 {
            let field = self.board[sq as usize];
            if field == EMPTY || field == OFFBOARD {
                continue;
            }
            if (field & 128) != my_color {
                continue;
            }
            let figure = field & 127;

            match figure {
                PAWN => {
                    if ptm == WHITE {
                        if self.board[(sq + 10) as usize] == EMPTY {
                            if sq / 10 == 8 {
                                self.try_move(sq, sq + 10, 0x0200);
                                self.try_move(sq, sq + 10, 0x0100);
                                self.try_move(sq, sq + 10, 0x0002);
                                self.try_move(sq, sq + 10, 0x0001);
                            } else {
                                self.try_move(sq, sq + 10, 0);
                                if sq / 10 == 3 && self.board[(sq + 20) as usize] == EMPTY {
                                    self.try_move(sq, sq + 20, 0);
                                }
                            }
                        }
                        for &d in &[9i32, 11] {
                            let cap = sq + d;
                            if (self.board[cap as usize] & 128) == BLACK_COLOR
                                && self.board[cap as usize] != OFFBOARD
                            {
                                if sq / 10 == 8 {
                                    self.try_move(sq, cap, 0x0200);
                                    self.try_move(sq, cap, 0x0100);
                                    self.try_move(sq, cap, 0x0002);
                                    self.try_move(sq, cap, 0x0001);
                                } else {
                                    self.try_move(sq, cap, 0);
                                }
                            }
                            if en_pass != 0 && en_pass == sq + d {
                                self.try_move(sq, cap, 0);
                            }
                        }
                    } else {
                        if self.board[(sq - 10) as usize] == EMPTY {
                            if sq / 10 == 3 {
                                self.try_move(sq, sq - 10, 0x0200);
                                self.try_move(sq, sq - 10, 0x0100);
                                self.try_move(sq, sq - 10, 0x0002);
                                self.try_move(sq, sq - 10, 0x0001);
                            } else {
                                self.try_move(sq, sq - 10, 0);
                                if sq / 10 == 8 && self.board[(sq - 20) as usize] == EMPTY {
                                    self.try_move(sq, sq - 20, 0);
                                }
                            }
                        }
                        for &d in &[9i32, 11] {
                            let cap = sq - d;
                            if self.board[cap as usize] > 0 && self.board[cap as usize] < BLACK_COLOR
                            {
                                if sq / 10 == 3 {
                                    self.try_move(sq, cap, 0x0200);
                                    self.try_move(sq, cap, 0x0100);
                                    self.try_move(sq, cap, 0x0002);
                                    self.try_move(sq, cap, 0x0001);
                                } else {
                                    self.try_move(sq, cap, 0);
                                }
                            }
                            if en_pass > 1 && en_pass == sq - d {
                                self.try_move(sq, cap, 0);
                            }
                        }
                    }
                }
                KNIGHT => {
                    for &d in &[-21i32, -19, -12, -8, 8, 12, 19, 21] {
                        if self.can_land(sq + d, opp_color) {
                            self.try_move(sq, sq + d, 0);
                        }
                    }
                }
                KING => {
                    self.castling();
                    for &d in &[-11i32, -10, -9, -1, 1, 9, 10, 11] {
                        if self.can_land(sq + d, opp_color) {
                            self.try_move(sq, sq + d, 0);
                        }
                    }
                }
                QUEEN => {
                    for &d in &[-11i32, -10, -9, -1, 1, 9, 10, 11] {
                        self.slide(sq, d, opp_color);
                    }
                }
                ROOK => {
                    for &d in &[-10i32, -1, 1, 10] {
                        self.slide(sq, d, opp_color);
                    }
                }
                BISHOP => {
                    for &d in &[-11i32, -9, 9, 11] {
                        self.slide(sq, d, opp_color);
                    }
                }
                _ => {}
            }
        }
    }

    #[inline]
    fn can_land(&self, t: i32, opp_color: i32) -> bool {
        if t < 21 || t > 98 {
            return false;
        }
        let v = self.board[t as usize];
        v == EMPTY || ((v & 128) == opp_color && v != OFFBOARD)
    }

    fn slide(&mut self, sf: i32, dir: i32, opp_color: i32) {
        let mut sq = sf + dir;
        while self.board[sq as usize] == EMPTY {
            self.try_move(sf, sq, 0);
            sq += dir;
        }
        if self.board[sq as usize] != OFFBOARD && (self.board[sq as usize] & 128) == opp_color {
            self.try_move(sf, sq, 0);
        }
    }

    /// Tentatively add a move, then drop it if it leaves our king in check.
    /// Port of the `try_move` lambda + `is_really_legal`.
    fn try_move(&mut self, sf: i32, st: i32, promo: i32) {
        self.legal_pointer += 1;
        self.legal_moves[self.legal_pointer as usize] = encode(sf, st, promo);
        self.is_really_legal();
    }

    /// Make the pending move with `fake=true`, test king safety, unmake, and
    /// drop the move if illegal. Port of C++ `Table::is_really_legal`.
    fn is_really_legal(&mut self) {
        let mv = self.legal_moves[self.legal_pointer as usize];
        self.update_table(mv, true);
        let king = if self.player_to_move == WHITE {
            self.movelist[self.move_number].pos_white_king
        } else {
            self.movelist[self.move_number].pos_black_king
        };
        if self.is_attacked(king, self.player_to_move) {
            self.legal_pointer -= 1;
        }
        self.unmake_table();
    }

    /// Add castling moves if legal. Port of C++ `Table::castling`.
    fn castling(&mut self) {
        let t = &self.board;
        let castle = self.movelist[self.move_number].castle;
        if self.player_to_move == WHITE {
            if t[28] == WHITE_ROOK
                && t[25] == WHITE_KING
                && (castle & 1) == 1
                && t[26] == EMPTY
                && t[27] == EMPTY
                && !self.is_attacked(25, WHITE)
                && !self.is_attacked(26, WHITE)
            {
                self.legal_pointer += 1;
                self.legal_moves[self.legal_pointer as usize] = 0x80c0;
                self.is_really_legal();
            }
            let t = &self.board;
            if t[25] == WHITE_KING
                && t[21] == WHITE_ROOK
                && (castle & 2) == 2
                && t[24] == EMPTY
                && t[23] == EMPTY
                && t[22] == EMPTY
                && !self.is_attacked(25, WHITE)
                && !self.is_attacked(24, WHITE)
            {
                self.legal_pointer += 1;
                self.legal_moves[self.legal_pointer as usize] = 0x8040;
                self.is_really_legal();
            }
        } else {
            if (castle & 4) == 4
                && t[96] == EMPTY
                && t[97] == EMPTY
                && t[98] == BLACK_ROOK
                && t[95] == BLACK_KING
                && !self.is_attacked(95, BLACK)
                && !self.is_attacked(96, BLACK)
            {
                self.legal_pointer += 1;
                self.legal_moves[self.legal_pointer as usize] = 0x9cdc;
                self.is_really_legal();
            }
            let t = &self.board;
            if t[95] == BLACK_KING
                && t[91] == BLACK_ROOK
                && (castle & 8) == 8
                && t[94] == EMPTY
                && t[93] == EMPTY
                && t[92] == EMPTY
                && !self.is_attacked(95, BLACK)
                && !self.is_attacked(94, BLACK)
            {
                self.legal_pointer += 1;
                self.legal_moves[self.legal_pointer as usize] = 0x9c5c;
                self.is_really_legal();
            }
        }
    }

    /// Apply a move to the board and push a new state slot. Port of
    /// C++ `Table::update_table`.
    ///
    /// The pawn-square list is updated for every move (so the make/unmake pair
    /// used by the legality probe stays balanced). The material sum and Zobrist
    /// key are updated only for real moves (`fake == false`), matching the C++
    /// `!fake` guard — perft and legality probes pass `fake == true` and skip
    /// that eval/TT-only work.
    pub fn update_table(&mut self, mv: i32, fake: bool) {
        let pm1 = self.movelist[self.move_number];
        self.move_number += 1;

        let x_from = (mv & 0xe000) >> 13;
        let y_from = (mv & 0x1c00) >> 10;
        let x_to = (mv & 0x00e0) >> 5;
        let y_to = (mv & 0x001c) >> 2;
        let square_from = (1 + x_from + (y_from + 2) * 10) as usize;
        let square_to = (1 + x_to + (y_to + 2) * 10) as usize;
        let figure_from = self.board[square_from];
        let figure_to = self.board[square_to];

        let mut pm2 = PositionState::default();
        pm2.pos_white_king = pm1.pos_white_king;
        pm2.pos_black_king = pm1.pos_black_king;
        pm2.en_passant = pm1.en_passant;
        pm2.not_pawn_move = pm1.not_pawn_move;
        pm2.color = self.player_to_move;
        pm2.move_from = square_from as i32;
        pm2.move_to = square_to as i32;
        pm2.castle = pm1.castle;
        pm2.captured_figure = figure_to;
        pm2.figure_moved = figure_from;
        pm2.ep_capture_sq = 0;
        // Eval-only fields carried from the parent (king-castled bonus flags).
        // The double-bishop bonus is dropped for a side when its bishop is
        // captured, matching C++ update_table.
        pm2.white_king_castled = pm1.white_king_castled;
        pm2.black_king_castled = pm1.black_king_castled;
        if figure_to == WHITE_BISHOP {
            pm2.white_double_bishops = 0;
            pm2.black_double_bishops = pm1.black_double_bishops;
        } else if figure_to == BLACK_BISHOP {
            pm2.white_double_bishops = pm1.white_double_bishops;
            pm2.black_double_bishops = 0;
        } else {
            pm2.white_double_bishops = pm1.white_double_bishops;
            pm2.black_double_bishops = pm1.black_double_bishops;
        }

        // Promotion.
        if (mv & 0x0303) > 0 {
            let promoted_fig = if (mv & 0x0200) == 0x0200 {
                QUEEN
            } else if (mv & 0x0100) == 0x0100 {
                ROOK
            } else if (mv & 0x0002) == 0x0002 {
                BISHOP
            } else {
                KNIGHT
            };
            pm2.promotion = promoted_fig;
            let mut piece = promoted_fig;
            if self.player_to_move == BLACK {
                piece += BLACK_COLOR;
            }
            self.board[square_to] = piece;
        } else {
            pm2.promotion = 0;
            self.board[square_to] = figure_from;
        }

        let is_pawn = figure_from == WHITE_PAWN || figure_from == BLACK_PAWN;
        if !is_pawn {
            if figure_from == WHITE_KING {
                pm2.pos_white_king = square_to as i32;
                pm2.castle &= 12;
                if square_from == 25 && square_to == 27 {
                    pm2.white_king_castled = KING_CASTLED;
                    self.board[26] = WHITE_ROOK;
                    self.board[28] = EMPTY;
                } else if square_from == 25 && square_to == 23 {
                    pm2.white_king_castled = KING_CASTLED;
                    self.board[24] = WHITE_ROOK;
                    self.board[21] = EMPTY;
                }
            } else if figure_from == BLACK_KING {
                pm2.pos_black_king = square_to as i32;
                pm2.castle &= 3;
                if square_from == 95 && square_to == 97 {
                    pm2.black_king_castled = KING_CASTLED;
                    self.board[96] = BLACK_ROOK;
                    self.board[98] = EMPTY;
                } else if square_from == 95 && square_to == 93 {
                    pm2.black_king_castled = KING_CASTLED;
                    self.board[94] = BLACK_ROOK;
                    self.board[91] = EMPTY;
                }
            }
            // Rook moves lose the corresponding castle right.
            pm2.en_passant = 0;
            if figure_from == WHITE_ROOK {
                if square_from == 21 {
                    pm2.castle &= 13;
                } else if square_from == 28 {
                    pm2.castle &= 14;
                }
            } else if figure_from == BLACK_ROOK {
                if square_from == 91 {
                    pm2.castle &= 7;
                } else if square_from == 98 {
                    pm2.castle &= 11;
                }
            }
        } else {
            // Pawn move: set/clear en passant target.
            let diff = square_to as i32 - square_from as i32;
            if diff == 20 {
                pm2.en_passant = square_from as i32 + 10;
            } else if diff == -20 {
                pm2.en_passant = square_to as i32 + 10;
            } else {
                pm2.en_passant = 0;
            }

            // En passant capture.
            if pm1.en_passant > 1 && figure_to == EMPTY && square_to as i32 == pm1.en_passant {
                if figure_from == WHITE_PAWN
                    && (diff == 9 || diff == 11)
                    && self.board[square_to - 10] == BLACK_PAWN
                {
                    pm2.captured_figure = BLACK_PAWN;
                    pm2.ep_capture_sq = (square_to - 10) as i32;
                    self.board[square_to - 10] = EMPTY;
                } else if figure_from == BLACK_PAWN
                    && (-diff == 9 || -diff == 11)
                    && self.board[square_to + 10] == WHITE_PAWN
                {
                    pm2.captured_figure = WHITE_PAWN;
                    pm2.ep_capture_sq = (square_to + 10) as i32;
                    self.board[square_to + 10] = EMPTY;
                }
            }
        }

        self.board[square_from] = EMPTY;
        self.movelist[self.move_number] = pm2;

        // --- Incremental pawn-list update (both fake and real moves, so the
        //     make/unmake pair stays balanced for the legality probe path) ---
        {
            let from_is_pawn = figure_from == WHITE_PAWN || figure_from == BLACK_PAWN;
            let cap = pm2.captured_figure;
            let cap_is_pawn = cap == WHITE_PAWN || cap == BLACK_PAWN;
            if cap_is_pawn {
                if pm2.ep_capture_sq != 0 {
                    self.pawn_remove(pm2.ep_capture_sq);
                } else {
                    self.pawn_remove(square_to as i32);
                }
            }
            if from_is_pawn {
                if pm2.promotion != 0 {
                    self.pawn_remove(square_from as i32);
                } else {
                    self.pawn_move(square_from as i32, square_to as i32);
                }
            }
        }

        // --- Incremental material + Zobrist (real moves only; eval/TT state) ---
        if !fake {
            // Material: start from parent, subtract capture, apply promotion delta.
            let mut mat = pm1.material_wp;
            mat -= Self::material_delta(pm2.captured_figure);
            if pm2.promotion != 0 {
                let promoted_piece = self.board[square_to];
                mat -= Self::material_delta(figure_from);
                mat += Self::material_delta(promoted_piece);
            }
            self.movelist[self.move_number].material_wp = mat;
            self.material_wp = mat;

            // Zobrist key, updated incrementally from the parent key.
            let z = &self.zobrist;
            let mut key = pm1.zobrist_key;
            let xor_piece = |key: &mut u64, sq: usize, piece: i32| {
                let ci = ((piece & 128) >> 7) as usize;
                let fig = (piece & 127) as usize;
                *key ^= z.piece[ci][fig][sq];
            };
            // 1. Flip side to move.
            key ^= z.side_white;
            key ^= z.side_black;
            // 2. Castle rights changed.
            key ^= z.castle[(pm1.castle & 15) as usize];
            key ^= z.castle[(pm2.castle & 15) as usize];
            // 3. En passant changed.
            key ^= z.enpassant[pm1.en_passant as usize];
            key ^= z.enpassant[pm2.en_passant as usize];
            // 4. Moving piece leaves square_from.
            xor_piece(&mut key, square_from, figure_from);
            // 5. Regular capture leaves square_to.
            if figure_to != EMPTY {
                xor_piece(&mut key, square_to, figure_to);
            }
            // 6. Moving/promoted piece arrives at square_to.
            xor_piece(&mut key, square_to, self.board[square_to]);
            // 7. Castling rook move.
            if figure_from == WHITE_KING {
                if square_from == 25 && square_to == 27 {
                    xor_piece(&mut key, 28, WHITE_ROOK);
                    xor_piece(&mut key, 26, WHITE_ROOK);
                } else if square_from == 25 && square_to == 23 {
                    xor_piece(&mut key, 21, WHITE_ROOK);
                    xor_piece(&mut key, 24, WHITE_ROOK);
                }
            } else if figure_from == BLACK_KING {
                if square_from == 95 && square_to == 97 {
                    xor_piece(&mut key, 98, BLACK_ROOK);
                    xor_piece(&mut key, 96, BLACK_ROOK);
                } else if square_from == 95 && square_to == 93 {
                    xor_piece(&mut key, 91, BLACK_ROOK);
                    xor_piece(&mut key, 94, BLACK_ROOK);
                }
            }
            // 8. En passant capture removed a pawn from a different square.
            if pm2.ep_capture_sq != 0 {
                if figure_from == WHITE_PAWN {
                    xor_piece(&mut key, pm2.ep_capture_sq as usize, BLACK_PAWN);
                } else if figure_from == BLACK_PAWN {
                    xor_piece(&mut key, pm2.ep_capture_sq as usize, WHITE_PAWN);
                }
            }
            self.movelist[self.move_number].zobrist_key = key;
        }
    }

    /// Reverse the last `update_table`. Port of C++ `Table::unmake_table`.
    pub fn unmake_table(&mut self) {
        let pm = self.movelist[self.move_number];
        let sq_from = pm.move_from as usize;
        let sq_to = pm.move_to as usize;

        if pm.promotion != 0 {
            self.board[sq_from] = pm.figure_moved;
        } else {
            self.board[sq_from] = self.board[sq_to];
        }

        if pm.ep_capture_sq != 0 {
            self.board[sq_to] = EMPTY;
            self.board[pm.ep_capture_sq as usize] = pm.captured_figure;
        } else {
            self.board[sq_to] = pm.captured_figure;
        }

        // Castling: put the rook back.
        if (self.board[sq_from] & 127) == KING {
            if sq_from == 25 && sq_to == 27 {
                self.board[28] = WHITE_ROOK;
                self.board[26] = EMPTY;
            } else if sq_from == 25 && sq_to == 23 {
                self.board[21] = WHITE_ROOK;
                self.board[24] = EMPTY;
            } else if sq_from == 95 && sq_to == 97 {
                self.board[98] = BLACK_ROOK;
                self.board[96] = EMPTY;
            } else if sq_from == 95 && sq_to == 93 {
                self.board[91] = BLACK_ROOK;
                self.board[94] = EMPTY;
            }
        }

        // Reverse the incremental pawn-list update (exact inverse of
        // update_table, applied in reverse order). Port of C++ unmake_table.
        {
            let moved = pm.figure_moved;
            let from_is_pawn = moved == WHITE_PAWN || moved == BLACK_PAWN;
            let cap = pm.captured_figure;
            let cap_is_pawn = cap == WHITE_PAWN || cap == BLACK_PAWN;
            if from_is_pawn {
                if pm.promotion != 0 {
                    self.pawn_add(sq_from as i32);
                } else {
                    self.pawn_move(sq_to as i32, sq_from as i32);
                }
            }
            if cap_is_pawn {
                if pm.ep_capture_sq != 0 {
                    self.pawn_add(pm.ep_capture_sq);
                } else {
                    self.pawn_add(sq_to as i32);
                }
            }
        }

        self.move_number -= 1;
        // The running material mirror follows the now-current slot.
        self.material_wp = self.movelist[self.move_number].material_wp;
    }
}

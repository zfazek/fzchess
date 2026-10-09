//! Minimal board state and move generation needed to run `perft`.
//!
//! This is a faithful port of the move-generation / make-unmake core from the
//! C++ `Table` class, reduced to exactly what perft needs. Perft only counts
//! leaf nodes of the legal-move tree, so three bookkeeping systems that the C++
//! engine maintains for search/evaluation are intentionally omitted here:
//!   * the incremental Zobrist key (only used by repetition detection),
//!   * the incremental White-perspective material sum (only used by eval),
//!   * the incremental pawn-square list (only used by eval).
//! They are omitted from BOTH make and unmake, so the make/unmake pair stays
//! perfectly balanced — which is all perft requires.
//!
//! The board is the same 10x12 mailbox (`[i32; 120]`) with 0xff off-board
//! sentinels, and piece codes are identical to the C++ engine, so perft node
//! counts must match the C++ `golden.sh` baseline exactly.

use crate::types::*;

// Piece codes including color (match C++ Table.cpp constants).
const WHITE_PAWN: i32 = 0x01;
const WHITE_KNIGHT: i32 = 0x02;
const WHITE_BISHOP: i32 = 0x03;
const WHITE_ROOK: i32 = 0x04;
const WHITE_KING: i32 = 0x06;

const BLACK_PAWN: i32 = 0x81;
const BLACK_KNIGHT: i32 = 0x82;
const BLACK_BISHOP: i32 = 0x83;
const BLACK_ROOK: i32 = 0x84;
const BLACK_KING: i32 = 0x86;

const WHITE_COLOR: i32 = 0; // color bit value for white pieces
const BLACK_COLOR: i32 = 128; // color bit value for black pieces

/// Full board state needed for perft. A trimmed analogue of the C++ `Chess`
/// object: the mailbox board, the per-ply state stack, side to move, and the
/// scratch legal-move list.
pub struct Board {
    pub board: [i32; 120],
    pub movelist: Vec<PositionState>,
    pub move_number: usize,
    pub player_to_move: i32,
    pub legal_moves: [i32; MAX_LEGAL_MOVES],
    pub legal_pointer: i32,
}

impl Board {
    pub fn new() -> Self {
        let mut b = Board {
            board: [OFFBOARD; 120],
            movelist: vec![PositionState::default(); MAX_MOVES],
            move_number: 0,
            player_to_move: WHITE,
            legal_moves: [0; MAX_LEGAL_MOVES],
            legal_pointer: -1,
        };
        b.reset_startpos();
        b
    }

    /// Set up the standard starting position. Port of C++ `Table::reset_movelist`
    /// (minus eval-only fields). All castle rights available, no en passant.
    pub fn reset_startpos(&mut self) {
        #[rustfmt::skip]
        let start: [i32; 120] = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0x04, 0x02, 0x03, 0x05, 0x06, 0x03, 0x02, 0x04, 0xff,
            0xff, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0xff,
            0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
            0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
            0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
            0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
            0xff, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0xff,
            0xff, 0x84, 0x82, 0x83, 0x85, 0x86, 0x83, 0x82, 0x84, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ];
        for m in self.movelist.iter_mut() {
            *m = PositionState::default();
            m.castle = 15;
            m.pos_white_king = 25;
            m.pos_black_king = 95;
        }
        self.board = start;
        self.move_number = 0;
        self.player_to_move = WHITE;
    }

    /// Parse a FEN (board + side + castle + en passant fields) into the state.
    /// Port of the parsing core of C++ `Table::setboard`, hardened the same way
    /// (tolerates a FEN with the halfmove/fullmove counters omitted).
    ///
    /// Accepts the FEN body only (not the "position fen " prefix), e.g.
    /// "r3k2r/... w KQkq - 0 1".
    pub fn setboard(&mut self, fen: &str) {
        self.reset_startpos();
        // Clear the board region; ranks are filled below.
        self.move_number = 1;

        let bytes = fen.as_bytes();
        let mut n = 0usize;
        let mut x = 1i32;
        let mut y = 9i32;
        // 1. Piece placement, until the first space.
        while n < bytes.len() && bytes[n] != b' ' {
            let ch = bytes[n];
            if ch > b'0' && ch < b'9' {
                for _ in 0..(ch - b'0') {
                    self.board[(y * 10 + x) as usize] = EMPTY;
                    x += 1;
                }
                x -= 1;
            }
            if ch != b'/' {
                if let Some(code) = fen_piece_code(ch) {
                    self.board[(y * 10 + x) as usize] = code;
                }
                x += 1;
            }
            if ch == b'/' {
                y -= 1;
                x = 1;
            }
            n += 1;
        }
        n += 1; // skip space

        // 2. Side to move.
        if n < bytes.len() && bytes[n] == b'w' {
            self.player_to_move = WHITE;
            self.movelist[self.move_number].color = WHITE;
        } else {
            self.player_to_move = BLACK;
            self.movelist[self.move_number].color = BLACK;
        }
        n += 2; // skip side char + space

        // 3. Castle rights, until the next space or end of string.
        self.movelist[self.move_number].castle = 0;
        while n < bytes.len() && bytes[n] != b' ' {
            match bytes[n] {
                b'-' => self.movelist[self.move_number].castle = 0,
                b'K' => self.movelist[self.move_number].castle |= 1,
                b'Q' => self.movelist[self.move_number].castle |= 2,
                b'k' => self.movelist[self.move_number].castle |= 4,
                b'q' => self.movelist[self.move_number].castle |= 8,
                _ => {}
            }
            n += 1;
        }
        // Mirror castle rights to the slot below, as the C++ engine does.
        let castle = self.movelist[self.move_number].castle;
        self.movelist[self.move_number - 1].castle = castle;
        n += 1; // skip space

        // 4. En passant target (optional). "-" means none.
        if n < bytes.len() && bytes[n] == b'-' {
            self.movelist[self.move_number].en_passant = 0;
        } else if n + 1 < bytes.len() && (b'a'..=b'h').contains(&bytes[n]) {
            self.movelist[self.move_number].en_passant =
                1 + (bytes[n] - b'a') as i32 + ((bytes[n + 1] - b'1') as i32 + 2) * 10;
        }
        // Halfmove/fullmove counters are not needed for perft, so we stop here.

        // Record king positions from the parsed board.
        for i in 20..100 {
            match self.board[i] {
                WHITE_KING => self.movelist[self.move_number].pos_white_king = i as i32,
                BLACK_KING => self.movelist[self.move_number].pos_black_king = i as i32,
                _ => {}
            }
        }
    }

    pub fn invert_player_to_move(&mut self) {
        self.player_to_move = -self.player_to_move;
    }

    /// `perft(depth)`: count leaf nodes of the legal move tree. Port of C++
    /// `Chess::perft` (without the periodic time `checkup`, irrelevant to counts).
    pub fn perft(&mut self, depth: i32) -> u64 {
        if depth == 0 {
            return 1;
        }
        self.list_legal_moves();
        let nbr_legal = (self.legal_pointer + 1) as usize;
        let mut moves = [0i32; MAX_LEGAL_MOVES];
        moves[..nbr_legal].copy_from_slice(&self.legal_moves[..nbr_legal]);

        let mut nodes = 0u64;
        for &mv in moves.iter().take(nbr_legal) {
            self.update_table(mv, true);
            self.invert_player_to_move();
            nodes += self.perft(depth - 1);
            self.invert_player_to_move();
            self.unmake_table();
        }
        nodes
    }
}

/// Map a FEN piece letter to its board code. Port of the C++ `graphical_figure`
/// lookup used in `setboard`.
fn fen_piece_code(ch: u8) -> Option<i32> {
    Some(match ch {
        b'P' => WHITE_PAWN,
        b'N' => WHITE_KNIGHT,
        b'B' => WHITE_BISHOP,
        b'R' => WHITE_ROOK,
        b'Q' => 0x05,
        b'K' => WHITE_KING,
        b'p' => BLACK_PAWN,
        b'n' => BLACK_KNIGHT,
        b'b' => BLACK_BISHOP,
        b'r' => BLACK_ROOK,
        b'q' => 0x85,
        b'k' => BLACK_KING,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Low-level smoke test that `Board::perft` works directly on the board
    // type. Exhaustive reference-count pinning lives in the public perft API
    // tests (`perft.rs`) and the integration test (`tests/perft.rs`).
    #[test]
    fn board_perft_startpos_smoke() {
        assert_eq!(Board::new().perft(1), 20);
        assert_eq!(Board::new().perft(3), 8902);
    }

    #[test]
    fn setboard_then_perft_depth1() {
        let mut b = Board::new();
        b.setboard("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");
        assert_eq!(b.perft(1), 48);
    }
}

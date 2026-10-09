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

    // --- Incremental bookkeeping (used by evaluation/search, not by perft) ---
    // Maintained in update_table/unmake_table so the search can read them in
    // O(1). Perft does not read these; they are kept here (rather than in a
    // separate struct) so make/unmake stay a single balanced pair.
    /// White-perspective material sum (white total minus black total).
    pub material_wp: i32,
    /// Pawn squares (both colors) and an index map for O(1) removal.
    pub pawn_sq: [i32; 16],
    pub pawn_at: [i32; 120],
    pub n_pawns: usize,
    /// Zobrist hashing tables (random but internally consistent per `Board`).
    pub zobrist: Zobrist,
}

/// Zobrist random tables. The C++ engine seeds these from `time()` (random per
/// run); the exact values are neither reproducible nor needed for correctness,
/// so the Rust port uses a fixed-seed PRNG for deterministic tests. Only
/// internal consistency matters: the incremental key must equal a full
/// recompute, which the cross-check test verifies.
pub struct Zobrist {
    pub piece: [[[u64; 120]; 7]; 2], // [color 0=white/1=black][figure 0..6][square]
    pub side_white: u64,
    pub side_black: u64,
    pub enpassant: [u64; 120],
    pub castle: [u64; 16],
}

impl Zobrist {
    /// Build the tables with a deterministic SplitMix64 PRNG so tests are
    /// reproducible. (The C++ uses `rand()` seeded by time; values need not
    /// match — see type docs.)
    fn new() -> Self {
        let mut state: u64 = 0x9E3779B97F4A7C15; // fixed seed
        let mut next = || -> u64 {
            // SplitMix64
            state = state.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        };
        let mut piece = [[[0u64; 120]; 7]; 2];
        for c in 0..2 {
            for f in 1..7 {
                for k in 0..120 {
                    piece[c][f][k] = next();
                }
            }
        }
        let side_white = next();
        let side_black = next();
        let mut enpassant = [0u64; 120];
        for e in enpassant.iter_mut() {
            *e = next();
        }
        let mut castle = [0u64; 16];
        for c in castle.iter_mut() {
            *c = next();
        }
        Zobrist {
            piece,
            side_white,
            side_black,
            enpassant,
            castle,
        }
    }
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
            material_wp: 0,
            pawn_sq: [0; 16],
            pawn_at: [-1; 120],
            n_pawns: 0,
            zobrist: Zobrist::new(),
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

        // Initialise the incremental bookkeeping from the fresh board, then
        // seed every movelist slot's key/material (the C++ engine does the same
        // in reset_movelist so any slot reads a sane starting value).
        self.rebuild_pawn_list();
        self.material_wp = self.recompute_material();
        let start_key = self.compute_zobrist_key(0);
        for m in self.movelist.iter_mut() {
            m.zobrist_key = start_key;
            m.material_wp = self.material_wp;
        }
    }

    // --- Incremental pawn-list maintenance (port of C++ pawn_add/remove/move) ---

    #[inline]
    pub fn pawn_add(&mut self, sq: i32) {
        self.pawn_at[sq as usize] = self.n_pawns as i32;
        self.pawn_sq[self.n_pawns] = sq;
        self.n_pawns += 1;
    }

    #[inline]
    pub fn pawn_remove(&mut self, sq: i32) {
        let idx = self.pawn_at[sq as usize];
        self.n_pawns -= 1;
        let last = self.n_pawns;
        let moved = self.pawn_sq[last];
        self.pawn_sq[idx as usize] = moved;
        self.pawn_at[moved as usize] = idx;
        self.pawn_at[sq as usize] = -1;
    }

    #[inline]
    pub fn pawn_move(&mut self, from: i32, to: i32) {
        let idx = self.pawn_at[from as usize];
        self.pawn_sq[idx as usize] = to;
        self.pawn_at[to as usize] = idx;
        self.pawn_at[from as usize] = -1;
    }

    /// Rebuild the pawn-square list from the current board. Port of C++
    /// `Chess::rebuild_pawn_list`.
    pub fn rebuild_pawn_list(&mut self) {
        self.n_pawns = 0;
        self.pawn_at = [-1; 120];
        for sq in 20..100i32 {
            let f = self.board[sq as usize];
            if (f & 127) == PAWN && f != OFFBOARD {
                self.pawn_add(sq);
            }
        }
    }

    /// White-perspective material of a piece code: +value for white, -value for
    /// black, 0 for empty/offboard. Port of C++ `Table::material_delta`.
    #[inline]
    pub fn material_delta(field: i32) -> i32 {
        if field == EMPTY || field == OFFBOARD {
            return 0;
        }
        let v = PIECE_VALUE[(field & 127) as usize];
        if (field & 128) != 0 {
            -v
        } else {
            v
        }
    }

    /// Full White-perspective material sum over the board. Used to seed the
    /// incremental `material_wp` and to cross-check it in tests.
    pub fn recompute_material(&self) -> i32 {
        let mut m = 0;
        for i in 20..100 {
            m += Self::material_delta(self.board[i]);
        }
        m
    }

    /// Full Zobrist key recompute for the current position. Port of C++
    /// `Chess::compute_zobrist_key`, with the side-to-move taken from
    /// `player_to_move`.
    ///
    /// Note: unlike the C++ version this is only meaningful for the *current*
    /// move slot (`mn == move_number`), which is the only slot ever recomputed
    /// (setup seeding and the incremental cross-check). The C++ derived side
    /// from `-movelist[mn].color`, but for a position set up via `setboard`
    /// that slot's color labels the side-to-move's negation, which is
    /// inconsistent with the incremental "flip side every ply" update. Using
    /// `player_to_move` keeps the seed and the incremental key in agreement, as
    /// verified by the bookkeeping cross-check tests.
    pub fn compute_zobrist_key(&self, mn: usize) -> u64 {
        let z = &self.zobrist;
        let mut key = if self.player_to_move == WHITE {
            z.side_white
        } else {
            z.side_black
        };
        for k in 20..100 {
            let field = self.board[k];
            if field > EMPTY && field < OFFBOARD {
                let figure = (field & 127) as usize;
                let color_idx = ((field & 128) >> 7) as usize;
                key ^= z.piece[color_idx][figure][k];
            }
        }
        key ^= z.enpassant[self.movelist[mn].en_passant as usize];
        key ^= z.castle[(self.movelist[mn].castle & 15) as usize];
        key
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

        // Initialise incremental bookkeeping for the parsed position and seed
        // the current + prior move slots so the search can read them.
        self.rebuild_pawn_list();
        self.material_wp = self.recompute_material();
        let key = self.compute_zobrist_key(self.move_number);
        self.movelist[self.move_number].zobrist_key = key;
        self.movelist[self.move_number].material_wp = self.material_wp;
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

    /// Walk the legal-move tree with REAL moves (fake=false) and, at every node,
    /// assert the incrementally-maintained Zobrist key and material sum equal a
    /// full recompute from the board. This is the correctness gate for task 1:
    /// it proves the incremental bookkeeping stays in sync through make/unmake
    /// across castling, en passant, promotion, and captures.
    fn verify_bookkeeping(b: &mut Board, depth: i32) {
        // Incremental vs full recompute at this node.
        let inc_key = b.movelist[b.move_number].zobrist_key;
        let full_key = b.compute_zobrist_key(b.move_number);
        assert_eq!(inc_key, full_key, "zobrist key mismatch at mn={}", b.move_number);
        let inc_mat = b.movelist[b.move_number].material_wp;
        let full_mat = b.recompute_material();
        assert_eq!(inc_mat, full_mat, "material mismatch at mn={}", b.move_number);

        if depth == 0 {
            return;
        }
        b.list_legal_moves();
        let n = (b.legal_pointer + 1) as usize;
        let mut moves = [0i32; MAX_LEGAL_MOVES];
        moves[..n].copy_from_slice(&b.legal_moves[..n]);
        for &mv in moves.iter().take(n) {
            b.update_table(mv, false); // REAL move: maintains key + material
            b.invert_player_to_move();
            verify_bookkeeping(b, depth - 1);
            b.invert_player_to_move();
            b.unmake_table();
        }
    }

    #[test]
    fn bookkeeping_consistent_startpos() {
        let mut b = Board::new();
        verify_bookkeeping(&mut b, 3);
    }

    #[test]
    fn bookkeeping_consistent_kiwipete() {
        // Exercises castling, captures, and complex interactions.
        let mut b = Board::new();
        b.setboard("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");
        verify_bookkeeping(&mut b, 3);
    }

    #[test]
    fn bookkeeping_consistent_promotion_pos() {
        // Exercises promotions and en passant.
        let mut b = Board::new();
        b.setboard("r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1");
        verify_bookkeeping(&mut b, 3);
    }
}

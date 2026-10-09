//! Core board types and constants, ported from the C++ engine.
//!
//! The board uses the same 10x12 mailbox representation as the C++ engine
//! (`board[120]`), with `EMPTY`/`OFFBOARD` sentinels and piece codes where the
//! high bit (128) encodes color. Keeping these identical to the C++ engine is
//! what lets perft node counts match exactly across the two implementations.

// --- Board square sentinels (match C++ `#define EMPTY/OFFBOARD` in Table.h) ---
pub const EMPTY: i32 = 0x00;
pub const OFFBOARD: i32 = 0xff;

// --- Side to move (match C++ `Chess::WHITE` / `Chess::BLACK`) ---
pub const WHITE: i32 = 1;
pub const BLACK: i32 = -1;

// --- Piece codes (match C++ `Table::Pawn` .. `Table::King`) ---
// A piece is `figure | color_bit`, where color_bit is 0 for white, 128 for black.
pub const PAWN: i32 = 1;
pub const KNIGHT: i32 = 2;
pub const BISHOP: i32 = 3;
pub const ROOK: i32 = 4;
pub const QUEEN: i32 = 5;
pub const KING: i32 = 6;

/// Color bit set in a piece code for black pieces (match C++ `Table::BlackColor`).
pub const BLACK_COLOR_BIT: i32 = 128;

// --- Fixed sizes (match C++ `#define` values in Chess.h) ---
pub const MAX_MOVES: usize = 1000;
pub const MAX_LEGAL_MOVES: usize = 128;

/// Material value by figure index (empty, pawn, knight, bishop, rook, queen,
/// king). Matches C++ `Table::piece_value`.
pub const PIECE_VALUE: [i32; 7] = [0, 100, 330, 330, 500, 900, 0];

/// A scored move. Mirrors C++ `struct move_t { int move; int value; }`.
///
/// The C++ `operator<` sorts descending by value (best first); the equivalent
/// ordering will be provided when the move-ordering code is ported, so it is
/// intentionally omitted here to avoid committing to a comparison before the
/// consuming code exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Move {
    pub mv: i32,
    pub value: i32,
}

/// Per-ply position parameters. Faithful field-for-field port of C++
/// `struct position_t`. One entry per half-move is stored in the move list so
/// that `unmake` can restore prior state (the C++ engine does the same).
///
/// Field names match the C++ struct to keep the port auditable; they will be
/// revisited for idiomatic naming once the dependent code is ported and tested.
#[derive(Clone, Copy, Debug, Default)]
pub struct PositionState {
    pub zobrist_key: u64,
    pub material_wp: i32,
    pub color: i32,
    pub move_from: i32,
    pub move_to: i32,
    pub captured_figure: i32,
    pub figure_moved: i32,
    pub ep_capture_sq: i32,
    pub promotion: i32,
    pub castle: i32,
    pub not_pawn_move: i32,
    pub en_passant: i32,
    pub white_king_castled: i32,
    pub black_king_castled: i32,
    pub pos_white_king: i32,
    pub pos_black_king: i32,
    pub white_double_bishops: i32,
    pub black_double_bishops: i32,
    pub further: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_color_bit_roundtrip() {
        // A black knight is KNIGHT | BLACK_COLOR_BIT; stripping the color bit
        // recovers the figure index, matching the C++ `field & 127` idiom.
        let black_knight = KNIGHT | BLACK_COLOR_BIT;
        assert_eq!(black_knight & 127, KNIGHT);
        assert_eq!(black_knight & 128, BLACK_COLOR_BIT);
    }

    #[test]
    fn material_values_match_cpp() {
        assert_eq!(PIECE_VALUE[PAWN as usize], 100);
        assert_eq!(PIECE_VALUE[QUEEN as usize], 900);
        assert_eq!(PIECE_VALUE[KING as usize], 0);
    }

    #[test]
    fn move_default_is_zeroed() {
        let m = Move::default();
        assert_eq!(m.mv, 0);
        assert_eq!(m.value, 0);
    }
}

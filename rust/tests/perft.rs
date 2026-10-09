//! Integration test: the public perft API must reproduce the standard
//! reference node counts for every position in the shared suite. These counts
//! are language-independent and match the C++ engine's golden baseline, so this
//! test is the Rust-side correctness gate for move generation.

use fzchess::perft::{PerftCase, SUITE};

/// Expected `(depth, nodes)` results for each case in `perft::SUITE`, in order.
/// Values are the well-known reference perft counts.
const EXPECTED: &[&[(i32, u64)]] = &[
    // startpos
    &[(1, 20), (2, 400), (3, 8902), (4, 197281)],
    // Kiwipete
    &[(1, 48), (2, 2039), (3, 97862), (4, 4085603)],
    // rook endgame
    &[(1, 14), (2, 191), (3, 2812), (4, 43238), (5, 674624)],
    // promotion / tactics
    &[(1, 6), (2, 264), (3, 9467), (4, 422333), (5, 15833292)],
];

#[test]
fn suite_matches_reference_counts() {
    assert_eq!(SUITE.len(), EXPECTED.len(), "suite/expected length mismatch");
    for (case, expected) in SUITE.iter().zip(EXPECTED) {
        assert_eq!(
            case.run(),
            expected.to_vec(),
            "perft mismatch for case {:?}",
            case.fen
        );
    }
}

#[test]
fn startpos_depth5_reference() {
    // A deeper check not in the printed suite, for extra confidence.
    assert_eq!(PerftCase::startpos(5).count(5), 4865609);
}

//! Perft (performance test) move-generation counting.
//!
//! Perft counts the leaf nodes of the legal-move tree to a fixed depth. It is
//! the objective, language-independent correctness gate for move generation:
//! the counts must match reference values (and the C++ engine) exactly.
//!
//! The API returns structured results rather than printing, so callers (the CLI
//! binary, tests, a future benchmark harness) decide how to present them.

use crate::board::Board;

/// A perft test case: a starting position and the maximum depth to search.
///
/// `None` for `fen` means the standard starting position (so the common case
/// needs no FEN string), otherwise the FEN body (without any "position fen "
/// prefix) is used.
#[derive(Clone, Copy, Debug)]
pub struct PerftCase {
    pub fen: Option<&'static str>,
    pub max_depth: i32,
}

impl PerftCase {
    pub const fn startpos(max_depth: i32) -> Self {
        PerftCase { fen: None, max_depth }
    }

    pub const fn from_fen(fen: &'static str, max_depth: i32) -> Self {
        PerftCase { fen: Some(fen), max_depth }
    }

    /// Build a `Board` positioned at this case's starting position.
    fn board(&self) -> Board {
        let mut b = Board::new();
        if let Some(fen) = self.fen {
            b.setboard(fen);
        }
        b
    }

    /// Count nodes at a single depth from this case's starting position.
    pub fn count(&self, depth: i32) -> u64 {
        self.board().perft(depth)
    }

    /// Count nodes at every depth from 1 to `max_depth`, returning
    /// `(depth, nodes)` pairs in order.
    pub fn run(&self) -> Vec<(i32, u64)> {
        (1..=self.max_depth).map(|d| (d, self.count(d))).collect()
    }
}

/// The standard perft suite. Mirrors the positions and depths the C++ engine
/// runs under `./fzchess 2`, so the output can be diffed against the shared
/// golden baseline. The C++ suite prints the startpos block twice; the CLI
/// replicates that for an exact line-by-line match (see `main.rs`).
pub const SUITE: &[PerftCase] = &[
    PerftCase::startpos(4),
    PerftCase::from_fen(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        4,
    ),
    PerftCase::from_fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 5),
    PerftCase::from_fen(
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        5,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_case_matches_reference() {
        let case = PerftCase::startpos(4);
        assert_eq!(case.run(), vec![(1, 20), (2, 400), (3, 8902), (4, 197281)]);
    }

    #[test]
    fn kiwipete_case_matches_reference() {
        let case = PerftCase::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            4,
        );
        assert_eq!(
            case.run(),
            vec![(1, 48), (2, 2039), (3, 97862), (4, 4085603)]
        );
    }

    #[test]
    fn count_single_depth() {
        // The `count` helper should agree with the last entry of `run`.
        let case = PerftCase::startpos(3);
        assert_eq!(case.count(3), 8902);
    }
}

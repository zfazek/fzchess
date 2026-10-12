//! Transposition table, ported from the C++ `TranspositionTable`.
//!
//! A direct-mapped, fixed-size table keyed by Zobrist hash. The replacement
//! scheme and the probe semantics match the C++ engine exactly: `probe` returns
//! the stored score whenever the full key matches (it does not gate on depth or
//! flag), and `store` replaces on an empty slot, a stale age, or a deeper-or-
//! equal search.
//!
//! Note: the C++ Zobrist tables are seeded from `time()` (random per run), so
//! the TT is already non-deterministic between C++ runs — yet the golden
//! bestmoves are deterministic. That tells us the TT affects only node/info
//! statistics, not the final bestmove, so the Rust TT need not reproduce C++
//! values; it only needs to be internally consistent.

/// Score flag for a stored entry. Matches C++ `TTFlag`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TtFlag {
    Exact = 0,
    Alpha = 1,
    Beta = 2,
}

/// Sentinel returned by `probe` on a miss. Matches C++ `TT_MISS == INT_MIN`.
pub const TT_MISS: i32 = i32::MIN;

#[derive(Clone, Copy, Default)]
struct TtEntry {
    zobrist_key: u64,
    score: i16,
    depth: i16,
    best_move: i32, // hash move: best/cutoff move found at this position (0 = none)
    flag: u8,
    age: u8,
}

pub struct TranspositionTable {
    table: Vec<TtEntry>,
    mask: usize,
    /// When false, `probe` always misses and `store` is a no-op — effectively
    /// disabling the TT without changing any call sites. Useful for A/B testing
    /// the TT's effect on search (node counts / move choice).
    enabled: bool,
}

impl TranspositionTable {
    /// Create a table with `num_elements` slots. `num_elements` MUST be a power
    /// of two (the mask-based indexing relies on it). The C++ engine uses
    /// `1 << 20` (~1M entries). The table is enabled by default.
    pub fn with_capacity(num_elements: usize) -> Self {
        debug_assert!(
            num_elements.is_power_of_two(),
            "TT size must be a power of two"
        );
        TranspositionTable {
            table: vec![TtEntry::default(); num_elements],
            mask: num_elements - 1,
            enabled: true,
        }
    }

    /// Enable or disable the table. When disabled the search behaves as if no
    /// transposition table existed (every probe misses, nothing is stored).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Look up a position. Returns the stored score on a key match, else
    /// `TT_MISS`. Port of C++ `TranspositionTable::probe`.
    #[inline]
    pub fn probe(&self, key: u64) -> i32 {
        if !self.enabled || self.table.is_empty() {
            return TT_MISS;
        }
        let e = &self.table[(key as usize) & self.mask];
        if e.zobrist_key == key {
            e.score as i32
        } else {
            TT_MISS
        }
    }

    /// Return the stored hash move for `key` (0 if miss / no move). Used only
    /// for move ordering, so a stale/colliding move is harmless — it is
    /// validated against the legal move list before use.
    #[inline]
    pub fn probe_move(&self, key: u64) -> i32 {
        if !self.enabled || self.table.is_empty() {
            return 0;
        }
        let e = &self.table[(key as usize) & self.mask];
        if e.zobrist_key == key {
            e.best_move
        } else {
            0
        }
    }

    /// Store or update a position. Port of C++ `TranspositionTable::store`,
    /// extended to record the best/cutoff move (`best_move`) for ordering on
    /// future visits. Replaces on: empty slot, stale age, or deeper-or-equal.
    #[inline]
    pub fn store(
        &mut self,
        key: u64,
        depth: i16,
        score: i16,
        best_move: i32,
        flag: TtFlag,
        current_age: u8,
    ) {
        if !self.enabled || self.table.is_empty() {
            return;
        }
        let idx = (key as usize) & self.mask;
        let e = &mut self.table[idx];
        if e.zobrist_key == 0 || e.age != current_age || depth >= e.depth {
            e.zobrist_key = key;
            e.depth = depth;
            e.score = score;
            e.best_move = best_move;
            e.flag = flag as u8;
            e.age = current_age;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_miss_on_empty_and_unknown() {
        let mut tt = TranspositionTable::with_capacity(1 << 10);
        assert_eq!(tt.probe(12345), TT_MISS);
        tt.store(12345, 3, 42, 0, TtFlag::Exact, 1);
        assert_eq!(tt.probe(12345), 42);
        assert_eq!(tt.probe(99999), TT_MISS);
    }

    #[test]
    fn store_replaces_on_deeper_or_equal_depth() {
        let mut tt = TranspositionTable::with_capacity(1 << 10);
        tt.store(7, 2, 10, 0, TtFlag::Exact, 1);
        // Shallower search for a DIFFERENT key that maps to the same slot would
        // need a collision; here just confirm deeper replaces for same key.
        tt.store(7, 5, 20, 0, TtFlag::Exact, 1);
        assert_eq!(tt.probe(7), 20);
    }

    #[test]
    fn store_replaces_on_new_age() {
        let mut tt = TranspositionTable::with_capacity(1 << 10);
        tt.store(7, 9, 100, 0, TtFlag::Exact, 1);
        // A shallower entry from a NEW search age replaces (stale entry).
        tt.store(7, 1, 55, 0, TtFlag::Exact, 2);
        assert_eq!(tt.probe(7), 55);
    }

    #[test]
    fn negative_score_roundtrips() {
        let mut tt = TranspositionTable::with_capacity(1 << 10);
        tt.store(42, 4, -321, 0, TtFlag::Beta, 1);
        assert_eq!(tt.probe(42), -321);
    }

    #[test]
    fn hash_move_roundtrips() {
        let mut tt = TranspositionTable::with_capacity(1 << 10);
        assert_eq!(tt.probe_move(7), 0); // miss
        tt.store(7, 3, 42, 33932 /* e2e4 */, TtFlag::Exact, 1);
        assert_eq!(tt.probe_move(7), 33932);
        assert_eq!(tt.probe_move(999), 0); // wrong key
    }
}

//! Transposition table, ported from the C++ `TranspositionTable`.
//!
//! A direct-mapped, fixed-size table keyed by Zobrist hash, shareable between
//! search threads (Lazy SMP): every slot is two atomic words and a probe is
//! verified with `key ^ data`, so there are no locks and torn reads are misses.
//! `store` replaces on an empty slot, a stale age, or a deeper-or-equal search.
//!
//! Entries record the REMAINING search depth, a bound flag (exact / lower /
//! upper) and the best move. The search uses them for score cutoffs (when the
//! stored depth and bound allow it) and for hash-move ordering. Because of the
//! cutoffs and thread timing, node counts and occasionally scores can differ
//! slightly between runs when `Threads > 1`.

/// Score flag for a stored entry. Matches C++ `TTFlag`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TtFlag {
    Exact = 0,
    Alpha = 1,
    Beta = 2,
}

impl TtFlag {
    #[inline]
    fn from_u8(v: u8) -> TtFlag {
        match v {
            1 => TtFlag::Alpha,
            2 => TtFlag::Beta,
            _ => TtFlag::Exact,
        }
    }
}

/// A verified TT hit with all fields, for score cutoffs.
#[derive(Clone, Copy, Debug)]
pub struct TtHit {
    pub score: i32,
    /// Remaining search depth (plies below this node) the score was searched to.
    pub depth: i32,
    pub flag: TtFlag,
    pub best_move: i32,
}

/// Sentinel returned by `probe` on a miss. Matches C++ `TT_MISS == INT_MIN`.
pub const TT_MISS: i32 = i32::MIN;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// One slot: two atomic words. `data` packs the payload; `check` holds
/// `key ^ data`. A probe accepts a slot only if `check ^ data == key`, so a
/// torn read (key/data written by two different threads) fails verification
/// and is treated as a miss. This makes the table lock-free and safe to share
/// between search threads (Lazy SMP) without any mutex.
///
/// `data` layout (64 bits): score i16 | depth i16 | best_move u16 | flag u8 | age u8.
struct Slot {
    check: AtomicU64,
    data: AtomicU64,
}

#[inline]
fn pack(score: i16, depth: i16, best_move: i32, flag: u8, age: u8) -> u64 {
    (score as u16 as u64)
        | ((depth as u16 as u64) << 16)
        | ((best_move as u16 as u64) << 32)
        | ((flag as u64) << 48)
        | ((age as u64) << 56)
}

#[inline]
fn unpack_score(d: u64) -> i16 {
    d as u16 as i16
}
#[inline]
fn unpack_depth(d: u64) -> i16 {
    (d >> 16) as u16 as i16
}
#[inline]
fn unpack_move(d: u64) -> i32 {
    ((d >> 32) as u16) as i32
}
#[inline]
fn unpack_age(d: u64) -> u8 {
    (d >> 56) as u8
}

pub struct TranspositionTable {
    table: Vec<Slot>,
    mask: usize,
    /// When false, `probe` always misses and `store` is a no-op — effectively
    /// disabling the TT without changing any call sites. Useful for A/B testing
    /// the TT's effect on search (node counts / move choice).
    enabled: AtomicBool,
}

impl TranspositionTable {
    /// Create a table with `num_elements` slots. `num_elements` MUST be a power
    /// of two (the mask-based indexing relies on it). The C++ engine uses
    /// `1 << 20` (~1M entries). The table is enabled by default. All access
    /// goes through `&self`, so the table can be shared via `Arc` across threads.
    pub fn with_capacity(num_elements: usize) -> Self {
        debug_assert!(
            num_elements.is_power_of_two(),
            "TT size must be a power of two"
        );
        let mut table = Vec::with_capacity(num_elements);
        table.resize_with(num_elements, || Slot {
            check: AtomicU64::new(0),
            data: AtomicU64::new(0),
        });
        TranspositionTable {
            table,
            mask: num_elements - 1,
            enabled: AtomicBool::new(true),
        }
    }

    /// Enable or disable the table. When disabled the search behaves as if no
    /// transposition table existed (every probe misses, nothing is stored).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Load a verified payload for `key`, or `None` on miss / torn read.
    #[inline]
    fn load(&self, key: u64) -> Option<u64> {
        if !self.enabled.load(Ordering::Relaxed) || self.table.is_empty() {
            return None;
        }
        let s = &self.table[(key as usize) & self.mask];
        let d = s.data.load(Ordering::Relaxed);
        let c = s.check.load(Ordering::Relaxed);
        if c ^ d == key { Some(d) } else { None }
    }

    /// Look up a position. Returns the stored score on a key match, else
    /// `TT_MISS`. Port of C++ `TranspositionTable::probe`.
    #[inline]
    pub fn probe(&self, key: u64) -> i32 {
        match self.load(key) {
            Some(d) => unpack_score(d) as i32,
            None => TT_MISS,
        }
    }

    /// Full entry for `key`, or `None` on miss. Used for score cutoffs.
    #[inline]
    pub fn probe_entry(&self, key: u64) -> Option<TtHit> {
        self.load(key).map(|d| TtHit {
            score: unpack_score(d) as i32,
            depth: unpack_depth(d) as i32,
            flag: TtFlag::from_u8((d >> 48) as u8),
            best_move: unpack_move(d),
        })
    }

    /// Return the stored hash move for `key` (0 if miss / no move). Used only
    /// for move ordering, so a stale/colliding move is harmless — it is
    /// validated against the legal move list before use.
    #[inline]
    pub fn probe_move(&self, key: u64) -> i32 {
        match self.load(key) {
            Some(d) => unpack_move(d),
            None => 0,
        }
    }

    /// Store or update a position. Port of C++ `TranspositionTable::store`,
    /// extended to record the best/cutoff move (`best_move`) for ordering on
    /// future visits. Replaces on: empty slot, stale age, or deeper-or-equal.
    #[inline]
    pub fn store(
        &self,
        key: u64,
        depth: i16,
        score: i16,
        best_move: i32,
        flag: TtFlag,
        current_age: u8,
    ) {
        if !self.enabled.load(Ordering::Relaxed) || self.table.is_empty() {
            return;
        }
        let s = &self.table[(key as usize) & self.mask];
        let old_d = s.data.load(Ordering::Relaxed);
        let old_c = s.check.load(Ordering::Relaxed);
        let empty = old_d == 0 && old_c == 0;
        if empty || unpack_age(old_d) != current_age || depth >= unpack_depth(old_d) {
            let d = pack(score, depth, best_move, flag as u8, current_age);
            s.data.store(d, Ordering::Relaxed);
            s.check.store(key ^ d, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_miss_on_empty_and_unknown() {
        let tt = TranspositionTable::with_capacity(1 << 10);
        assert_eq!(tt.probe(12345), TT_MISS);
        tt.store(12345, 3, 42, 0, TtFlag::Exact, 1);
        assert_eq!(tt.probe(12345), 42);
        assert_eq!(tt.probe(99999), TT_MISS);
    }

    #[test]
    fn store_replaces_on_deeper_or_equal_depth() {
        let tt = TranspositionTable::with_capacity(1 << 10);
        tt.store(7, 2, 10, 0, TtFlag::Exact, 1);
        // Shallower search for a DIFFERENT key that maps to the same slot would
        // need a collision; here just confirm deeper replaces for same key.
        tt.store(7, 5, 20, 0, TtFlag::Exact, 1);
        assert_eq!(tt.probe(7), 20);
    }

    #[test]
    fn store_replaces_on_new_age() {
        let tt = TranspositionTable::with_capacity(1 << 10);
        tt.store(7, 9, 100, 0, TtFlag::Exact, 1);
        // A shallower entry from a NEW search age replaces (stale entry).
        tt.store(7, 1, 55, 0, TtFlag::Exact, 2);
        assert_eq!(tt.probe(7), 55);
    }

    #[test]
    fn negative_score_roundtrips() {
        let tt = TranspositionTable::with_capacity(1 << 10);
        tt.store(42, 4, -321, 0, TtFlag::Beta, 1);
        assert_eq!(tt.probe(42), -321);
    }

    #[test]
    fn shared_across_threads_never_returns_torn_entries() {
        use std::sync::Arc;
        // Writers store entries whose score/move are derived from the key; any
        // successful probe must return the consistent pair.
        let tt = Arc::new(TranspositionTable::with_capacity(1 << 4));
        let mut hs = Vec::new();
        for t in 0..4u64 {
            let tt = Arc::clone(&tt);
            hs.push(std::thread::spawn(move || {
                for i in 0..200_000u64 {
                    // Zobrist-like (well mixed) keys; tiny structured keys could
                    // collide with XOR-mixed torn pairs far more often than real ones.
                    let key = (i * 4 + t + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    let v = ((key >> 20) % 30000) as i16;
                    tt.store(key, 3, v, v as i32, TtFlag::Exact, 1);
                    let s = tt.probe(key);
                    if s != TT_MISS {
                        assert_eq!(s, v as i32);
                        assert_eq!(tt.probe_move(key), v as i32);
                    }
                }
            }));
        }
        for h in hs {
            h.join().unwrap();
        }
    }

    #[test]
    fn probe_entry_roundtrips_all_fields() {
        let tt = TranspositionTable::with_capacity(1 << 10);
        assert!(tt.probe_entry(5).is_none());
        tt.store(5, 6, -123, 777, TtFlag::Beta, 3);
        let h = tt.probe_entry(5).unwrap();
        assert_eq!((h.score, h.depth, h.best_move), (-123, 6, 777));
        assert_eq!(h.flag, TtFlag::Beta);
    }

    #[test]
    fn hash_move_roundtrips() {
        let tt = TranspositionTable::with_capacity(1 << 10);
        assert_eq!(tt.probe_move(7), 0); // miss
        tt.store(7, 3, 42, 33932 /* e2e4 */, TtFlag::Exact, 1);
        assert_eq!(tt.probe_move(7), 33932);
        assert_eq!(tt.probe_move(999), 0); // wrong key
    }
}

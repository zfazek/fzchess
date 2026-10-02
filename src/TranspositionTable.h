#pragma once

#include <vector>
#include <cstdint>
#include <cstddef>
#include <climits>

// Flags to represent the type of evaluation score stored in the node
enum TTFlag : uint8_t {
    TT_EXACT, // The score is precise and exact
    TT_ALPHA, // The score is an upper bound (fail-low)
    TT_BETA   // The score is a lower bound (fail-high)
};

// 16-byte entry: key(8) + score(2) + depth(2) + flag(1) + age(1) + pad(2)
struct TTEntry {
    uint64_t zobristKey = 0; // The full 64-bit Zobrist hash key for verification
    int16_t  score      = 0; // The evaluation score of the position
    int16_t  depth      = 0; // The search depth at which this position was evaluated
    uint8_t  flag       = 0; // TTFlag: EXACT, ALPHA, or BETA
    uint8_t  age        = 0; // Search cycle identifier to track and replace obsolete entries
    int16_t  pad        = 0; // Padding to 16 bytes (reserved)
};
static_assert(sizeof(TTEntry) == 16, "TTEntry must be 16 bytes");

// Sentinel value returned by probe() on a cache miss
static constexpr int TT_MISS = INT_MIN;

class TranspositionTable {
private:
    std::vector<TTEntry> table;
    size_t mask; // Bitmask used for ultra-fast index calculation

public:
    TranspositionTable() : mask(0) {}

    // Configures the table size. numElements MUST be a power of 2 (e.g., 2^20, 2^24).
    void resize(size_t numElements);

    // Looks up a position. Returns the stored score on a hit, TT_MISS on a miss.
    inline int probe(uint64_t key) const {
        if (__builtin_expect(table.empty(), 0)) return TT_MISS;
        const TTEntry& e = table[key & mask];
        if (e.zobristKey == key) return e.score;
        return TT_MISS;
    }

    // Stores or updates a position in the table using a replacement scheme
    void store(uint64_t key, int16_t depth, int16_t score, uint8_t flag, uint8_t currentAge);
};

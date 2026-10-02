#include "TranspositionTable.h"

void TranspositionTable::resize(size_t numElements) {
    table.clear();
    table.resize(numElements);
    mask = numElements - 1; // Works only if numElements is a power of 2
}

void TranspositionTable::store(uint64_t key, int16_t depth, int16_t score, uint8_t flag, uint8_t currentAge) {
    if (__builtin_expect(table.empty(), 0)) return;
    TTEntry& existing = table[key & mask];

    // Replace if: empty slot, stale age, or deeper/equal search
    if (existing.zobristKey == 0 || existing.age != currentAge || depth >= existing.depth) {
        existing.zobristKey = key;
        existing.depth      = depth;
        existing.score      = score;
        existing.flag       = flag;
        existing.age        = currentAge;
    }
}

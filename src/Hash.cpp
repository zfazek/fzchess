#include "Hash.h"

#include <iostream>

#include "Chess.h"
#include "Util.h"

Hash::Hash() {
    init_hash();
}

void Hash::reset_counters() {
    hash_nodes = 0;
}

void Hash::init_hash() {
    srand(time(0));

    // WHITE: i = 0, BLACK: i = 1
    for (int i = 0; i < 2; i++) {

        // PAWN = 1, KNIGHT = 2, ..., KING = 7
        for (int j = 1; j < 7; j++) {
            for (int k = 0; k < 120; k++) {
                hash_piece[i][j][k] = hash_rand();
            }
        }
    }
    hash_side_white = hash_rand();
    hash_side_black = hash_rand();
    for (int i = 0; i < 120; i++) {
        hash_enpassant[i] = hash_rand();
    }
    for (int i = 0; i < 15; i++) {
        hash_castle[i] = hash_rand();
    }
}

// Set the hash variable of the current position
void Hash::set_hash(const Chess *chess) {
    hash = 0;
    // XOR the side to move
    if (chess->player_to_move == chess->WHITE) {
        hash ^= hash_side_white;
    } else {
        hash ^= hash_side_black;
    }
    // XOR the figures;
    for (int k = 20; k < 100; k++) {
        const int field = chess->board[k];
        if (field > EMPTY && field < OFFBOARD) {
            const int figure = (field & 127);
            const int i = (field & 128) >> 7;
            hash ^= hash_piece[i][figure][k];
        }
    }
    // XOR the enpassant position
    hash ^= hash_enpassant[chess->movelist[chess->move_number].en_passant];
    // XOR the castling possibilities
    hash ^= hash_castle[chess->movelist[chess->move_number].castle];
}

bool Hash::posInHashtable() const {
    return hashes.find(hash) != hashes.end();
}

int Hash::getU() const {
    return hashes.at(hash);
}

void Hash::setU(const int u) {
    hashes[hash] = u;
}

uint64_t Hash::rand64() const {
    uint64_t output;
    output = rand();
    output <<= 15;
    output ^= rand();
    output <<= 15;
    output ^= rand();
    output <<= 15;
    output ^= rand();
    output <<= 15;
    output ^= rand();
    return output;
}

uint64_t Hash::hash_rand() const {
    uint64_t r = 0LLU;
    for (int i = 0; i < 32; i++) {
        r ^= rand64() << i;
    }
    return r;
}

void Hash::printStatistics(const int nodes) const {
    if (nodes == 0) {
        return;
    }
    std::cout << "Hash found " << hash_nodes << ", hash/nodes: " << 100 * hash_nodes / nodes << "%" << std::endl;
}

void Hash::clear() {
    hashes.clear();
}

#include "Hash.h"

#include "Chess.h"
#include "Util.h"

#include <cstdlib>
#include <cstdio>

const unsigned int HASHSIZE = 1024 * 1024 * 16;

Hash::Hash() {
    init_hash();
}

Hash::~Hash() {
}

void Hash::reset_counters() {
    hash_nodes = 0;
}

void Hash::init_hash() {
    int i, j, k;
    srand(0);

    //WHITE: i = 0, BLACK: i = 1
    for (i = 0; i < 2; i++)

        //PAWN = 1, KNIGHT = 2, ..., KING = 7
        for (j = 1; j < 7; j++)
            for (k = 0; k < 120; k++)
                hash_piece[i][j][k] = hash_rand();
    hash_side_white = hash_rand();
    hash_side_black = hash_rand();
    for (i = 0; i < 120; i++) hash_enpassant[i] = hash_rand();
    for (i = 0; i < 15; i++) hash_castle[i] = hash_rand();
}

//Set the hash variable of the current position
void Hash::set_hash(const Chess* chess) {
    int i, k;
    int field, figure;
    hash = 0;
    // XOR the side to move
    if (chess->player_to_move == chess->WHITE) {
        hash ^= hash_side_white;
    }
    else {
        hash ^= hash_side_black;
    }
    // XOR the figures;
    for (k = 0; k < 120; k++) {
        field = chess->tablelist[chess->move_number][k];
        if (field > 0 && field < OFFBOARD) {
            figure = (field & 127);
            if ((field & 128) == 128) i = 1; else i = 0;
            hash ^= hash_piece[i][figure][k];
        }
    }
    // XOR the enpassant position
    hash ^= hash_enpassant[chess->movelist[chess->move_number].en_passant];
    // XOR the castling possibilities
    hash ^= hash_castle[chess->movelist[chess->move_number].castle];
    hash_index = hash % HASHSIZE;
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
    int i;
    uint64_t r = 0LLU;
    for (i = 0; i < 32; i++) r ^= rand64() << i;
    return r;
}

void Hash::printStatistics(const int nodes) const {
    printf("Hash found %d, hash/nodes: %d%%\n",
            hash_nodes,
            100 * hash_nodes/nodes);
    Util::flush();
}

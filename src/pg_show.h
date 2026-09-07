#pragma once

#ifdef __cplusplus
extern "C" {
#endif

#include <inttypes.h>
#include <stdio.h>

typedef unsigned char uint8;
typedef unsigned short uint16;
typedef unsigned int uint32;

typedef unsigned long long int uint64;

typedef struct {
    uint64 key;
    uint16 move;
    uint16 weight;
    uint32 learn;
} entry_t;

#define MAX_MOVES 100

int int_from_file(FILE *f, int l, uint64 *r);

int entry_from_file(FILE *f, entry_t *entry);

int find_key(FILE *f, uint64 key, entry_t *entry);

void move_to_string(char move_s[6], uint16 move);

int get_book_move_by_key(FILE *f, uint64 key, char out_move[6]);

int get_book_move(const char *book_file, uint64 key, char out_move[6]);

#ifdef __cplusplus
}
#endif


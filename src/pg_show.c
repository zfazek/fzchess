/* 
 * This utility looks up the moves and their scores in a Polyglot book
 *
 * Usage:
 * pg_show <book> <hex key>"
 *
 * You can find the hex key of a FEN using pg_key. 
 *
 * This code is released in the public domain by Michel Van den Bergh.
 *
 */
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <time.h>
#include <inttypes.h>

#include "pg_show.h"

entry_t entry_none = {
    0, 0, 0, 0
};

char *promote_pieces = " nbrq";

#define MAX_MOVES 100

int int_from_file(FILE *f, int l, uint64 *r) {
    int i,c;
    for (i = 0; i < l; i++) {
        c = fgetc(f);
        if (c == EOF) {
            return 1;
        }
        (*r) = ((*r) << 8) + c;
    }
    return 0;
}

int entry_from_file(FILE *f, entry_t *entry) {
    int ret;
    uint64 r;
    ret = int_from_file(f,8,&r);
    if (ret) return 1;
    entry->key = r;
    ret = int_from_file(f,2,&r);
    if (ret) return 1;
    entry->move = r;
    ret = int_from_file(f,2,&r);
    if (ret) return 1;
    entry->weight = r;
    ret = int_from_file(f,4,&r);
    if (ret) return 1;
    entry->learn = r;
    return 0;
}

int find_key(FILE *f, uint64 key, entry_t *entry) {
    int first, last, middle;
    entry_t first_entry = entry_none, last_entry,middle_entry;
    first = -1;
    if (fseek(f,-16,SEEK_END)) {
        *entry = entry_none;
        entry->key = key+1; //hack
        return -1;
    }
    last = ftell(f)/16;
    entry_from_file(f,&last_entry);
    while (1) {
        if (last-first == 1) {
            *entry = last_entry;
            return last;
        }
        middle = (first+last)/2;
        fseek(f,16*middle,SEEK_SET);
        entry_from_file(f,&middle_entry);
        if (key <= middle_entry.key) {
            last = middle;
            last_entry = middle_entry;
        } else {
            first = middle;
            first_entry = middle_entry;
        }
    }
}

void move_to_string(char move_s[6], uint16 move) {
    int f,fr,ff,t,tr,tf,p;
    f = (move>>6)&077;
    fr = (f>>3)&0x7;
    ff = f&0x7;
    t = move&077;
    tr = (t>>3)&0x7;
    tf = t&0x7;
    p = (move>>12)&0x7;
    move_s[0] = ff+'a';
    move_s[1] = fr+'1';
    move_s[2] = tf+'a';
    move_s[3] = tr+'1';
    if (p) {
        move_s[4] = promote_pieces[p];
        move_s[5] = '\0';
    } else {
        move_s[4] = '\0';
    }
    if (!strcmp(move_s,"e1h1")) {
        move_s[2] = 'g';
    } else if (!strcmp(move_s,"e1a1")) {
        move_s[2] = 'c';
    } else if (!strcmp(move_s,"e8h8")) {
        move_s[2] = 'g';
    } else if (!strcmp(move_s,"e8a8")) {
        move_s[2] = 'c';
    }
}

int get_book_move_by_key(FILE *f, uint64 key, char out_move[6]) {
    entry_t entry;
    int offset;
    entry_t entries[MAX_MOVES];
    int count = 0;
    int ret, i;
    int selected_idx = 0;
    uint32 total_weight = 0;
    uint32 current_weight = 0;
    uint32 random_weight;

    if (!f || !out_move) return 0;

    offset = find_key(f, key, &entry);
    if (entry.key != key) {
        return 0;
    }

    entries[0] = entry;
    count = 1;
    fseek(f, 16 * (offset + 1), SEEK_SET);
    while (1) {
        ret = entry_from_file(f, &entry);
        if (ret || entry.key != key) {
            break;
        }
        if (count == MAX_MOVES) {
            break;
        }
        entries[count++] = entry;
    }

    for (i = 0; i < count; i++) {
        total_weight += entries[i].weight;
    }

    if (total_weight > 0) {
        random_weight = (uint32)rand() % total_weight;
        for (i = 0; i < count; i++) {
            current_weight += entries[i].weight;
            if (random_weight < current_weight) {
                selected_idx = i;
                break;
            }
        }
    } else {
        selected_idx = rand() % count;
    }

    move_to_string(out_move, entries[selected_idx].move);
    return 1;
}

int get_book_move(const char *book_file, uint64 key, char out_move[6]) {
    FILE *f;
    int res;

    if (!book_file) return 0;

    f = fopen(book_file, "rb");
    if (!f) return 0;

    res = get_book_move_by_key(f, key, out_move);
    fclose(f);
    return res;
}

/*
int main(int argc, char *argv[]) {
    uint64 key;
    char move[6];

    srand((unsigned int)time(NULL));

    if (argc <= 2) {
        printf("Usage: %s <book file> <hex key>\n", argv[0]);
        return 1;
    }

    if (sscanf(argv[2], "%llx", &key) != 1) {
        printf("Invalid hex key: %s\n", argv[2]);
        return 1;
    }

    if (get_book_move(argv[1], key, move)) {
        printf("Selected move: %s\n", move);
    } else {
        printf("No move found for key.\n");
    }

    return 0;
}
*/

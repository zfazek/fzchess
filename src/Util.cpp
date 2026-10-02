#include "Util.h"

#include <cstdarg>
#include <cstdio>
#include <chrono>

void Util::flush() {
    fflush(stdout);
}

uint64_t Util::get_ms() {
    uint64_t milliseconds = std::chrono::duration_cast<std::chrono::milliseconds>
            (std::chrono::system_clock::now().time_since_epoch()).count();
    return milliseconds;
}

int Util::str2move(const char move_old[6]) {
    const int x_from = move_old[0] - 'a';
    const int y_from = move_old[1] - '1';
    const int x_to = move_old[2] - 'a';
    const int y_to = move_old[3] - '1';
    int move_hi = x_from * 32 + y_from * 4;
    int move_lo = x_to * 32 + y_to * 4;
    switch (move_old[4]) {
        case 'q':
        case 'Q':
            move_hi += 2;
            break;
        case 'r':
        case 'R':
            move_hi += 1;
            break;
        case 'b':
        case 'B':
            move_lo += 2;
            break;
        case 'n':
        case 'N':
            move_lo += 1;
            break;
        default:
            break;
    }
    return move_hi * 256 + move_lo;
}

char *Util::move2str(const int move) {
    // Returns the move in coordinate notation with no trailing space:
    //   normal moves are 4 chars ("e2e4"), promotions are 5 ("e7e8q").
    static char move_str[] = "      ";
    move_str[0] = (move & 0xe000) / 256 / 32 + 'a';
    move_str[1] = (move & 0x1c00) / 256 / 4 + '1';
    move_str[2] = (move & 0x00e0) % 256 / 32 + 'a';
    move_str[3] = (move & 0x001c) % 256 / 4 + '1';
    if ((move & 0x0303) > 0) {
        if ((move & 0x0200) == 0x0200) {
            move_str[4] = 'q';
        } else if ((move & 0x0100) == 0x0100) {
            move_str[4] = 'r';
        } else if ((move & 0x0002) == 0x0002) {
            move_str[4] = 'b';
        } else if ((move & 0x0001) == 0x0001) {
            move_str[4] = 'n';
        }
        move_str[5] = '\0';
    } else {
        move_str[4] = '\0';
    }
    return move_str;
}

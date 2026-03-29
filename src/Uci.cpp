#include "Uci.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "Chess.h"
#include "Util.h"

Uci::Uci(Chess *ch) : chess(ch) {}

void Uci::position_received(const char *input) {
    static char move_old[6];
    if (!strstr(input, "move")) {
        return;
    }
    strncpy(move_old, "     ", 6);
    chess->start_game();
    for (size_t i = 24; i < strlen(input) - 1; i++) {
        move_old[0] = input[i];
        i++;
        move_old[1] = input[i];
        i++;
        move_old[2] = input[i];
        i++;
        move_old[3] = input[i];
        i++;
        if (input[i] != ' ' && input[i] != '\n') {
            move_old[4] = input[i];
            i++;
        } else {
            move_old[4] = '\0';
        }
        chess->table->update_table(Util::str2move(move_old), false);
        chess->invert_player_to_move();
    }
    // print_table();
}

[[noreturn]] void Uci::processCommands(const char *cmd) {
    static char input[1000] = {0};
    int movestogo = 40;
    int wtime = 0;
    int btime = 0;
    int winc = 0;
    int binc = 0;
    chess->gui_depth = 0;
    if (strstr(cmd, "uci")) {
        printf("id name FZChess++\n");
        printf("id author Zoltan FAZEKAS\n");
        printf("option name OwnBook type check defult false\n");
        printf("option name Ponder type check default false\n");
        printf("option name MultiPV type spin default 1 min 1 max 1\n");
        printf("uciok\n");
        Util::flush();
    }
    while (true) {
        char *ret = fgets(input, 1000, stdin);
        if (ret && strstr(input, "quit")) {
            exit(EXIT_SUCCESS);
        }
        if (ret && strstr(input, "stop")) {
            chess->stop_received = true;
        }
        if (th_make_move.joinable()) {
            th_make_move.join();
        }
        // printf("Process input: %s\n", input);Util::flush();
        if (strstr(input, "isready")) {
            printf("readyok\n");
            Util::flush();
        }
        if (strstr(input, "position startpos")) {
            position_received(input);
        }
        if (strstr(input, "position fen")) {
            chess->table->setboard(input);
        }
        if (strstr(input, "go")) {
            chess->FZChess = chess->player_to_move;
            // if (strstr(input, "ponder")) continue;
            chess->movetime = 0;
            if (strstr(input, "movetime")) {
                sscanf(strstr(input, "movetime"), "movetime %d",
                       &chess->max_time);
                chess->movetime = chess->max_time;
                chess->gui_depth = 0;
            } else {
                if (strstr(input, "movestogo")) {
                    sscanf(strstr(input, "movestogo"), "movestogo %d",
                           &movestogo);
                }
                if (strstr(input, "wtime")) {
                    sscanf(strstr(input, "wtime"), "wtime %d", &wtime);
                }
                if (strstr(input, "btime")) {
                    sscanf(strstr(input, "btime"), "btime %d", &btime);
                }
                if (strstr(input, "winc")) {
                    sscanf(strstr(input, "winc"), "winc %d", &winc);
                }
                if (strstr(input, "binc")) {
                    sscanf(strstr(input, "binc"), "binc %d", &binc);
                }
                if (chess->FZChess == chess->WHITE) {
                    chess->max_time = (wtime + movestogo * winc) / movestogo;
                    chess->gui_depth = 0;
                } else {
                    chess->max_time = (btime + movestogo * binc) / movestogo;
                    chess->gui_depth = 0;
                }
                if (strstr(input, "depth")) {
                    sscanf(strstr(input, "depth"), "depth %d",
                           &chess->gui_depth);
                    chess->max_time = 0;
                }
                if (strstr(input, "infinite")) {
                    chess->gui_depth = 99;
                    chess->max_time = 0;
                }
            }
            chess->stop_received = false;
            th_make_move = std::thread(&Chess::make_move, chess);
        }
    }
}

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
        // Bare "position startpos" (no "moves" list): per the UCI spec the
        // "moves" section is optional and means "set up the start position with
        // no moves played". Reset the board to the opening instead of ignoring
        // the command.
        chess->start_game();
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
        chess->table->update_table(*chess, Util::str2move(move_old), false);
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
        printf("option name OwnBook type check default false\n");
        printf("option name Ponder type check default false\n");
        printf("option name MultiPV type spin default 1 min 1 max 1\n");
        printf("uciok\n");
        Util::flush();
    } else if (strstr(cmd, "isready")) {
        printf("readyok\n");
        Util::flush();
    }
    while (true) {
        char *ret = fgets(input, 1000, stdin);
        if (ret == nullptr) {
            // EOF or read error: abort any running search, then exit.
            // Without this, the stale `input` buffer (e.g. a prior "go") was
            // re-processed forever, causing infinite self-play and a
            // move_number overflow crash past MAX_MOVES.
            chess->stop_received = true;
            if (th_make_move.joinable()) {
                th_make_move.join();
            }
            exit(EXIT_SUCCESS);
        }
        if (ret && strstr(input, "quit")) {
            chess->stop_received = true;
            if (th_make_move.joinable()) {
                th_make_move.join();
            }
            exit(EXIT_SUCCESS);
        }
        if (ret && strstr(input, "stop")) {
            // Signal the running search to abort. The search polls
            // stop_received and unwinds; we then join below. We do NOT join
            // before setting the flag, otherwise stop would only take effect
            // after the search had already finished on its own.
            chess->stop_received = true;
        }
        // Reap a finished (or now-stopping) search before starting a new one.
        // A new search must never be launched while th_make_move is still
        // running, so join here is required before any "go" below.
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
            chess->table->setboard(*chess, input);
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
                // Guard against division by zero: movestogo 0 (or negative) is
                // treated as a safe default. Prevents SIGFPE in the time formula below.
                if (movestogo <= 0) {
                    movestogo = 40;
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

#include <cassert>
#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "Chess.h"
#include "Eval.h"
#include "Util.h"

void test_move_h2h4() {
    Chess chess;
    chess.start_game();
    const char input[] = "position startpos moves h2h4";
    chess.uci->position_received(input);
    // chess.max_time = 300000 / 40;
    chess.max_time = 0;
    chess.gui_depth = 4;
    chess.make_move();
    assert(chess.nodes > 0);
    assert(chess.table->eval->tt_nodes > 0);
}

void test_perft_pos(const char *input, const int depth, const uint64_t *expected_nodes) {
    for (int i = 1; i <= depth; i++) {
        Chess chess;
        chess.start_game();
        chess.table->setboard(input);
        uint64_t start_time = Util::get_ms();
        uint64_t nodes = chess.perft(i);
        uint64_t stop_time = Util::get_ms();
        uint64_t duration = stop_time - start_time;
        printf("depth: %d nodes: %lu time: %lu ms knps: %lu\n", i, nodes,
               duration, duration == 0 ? 0 : nodes / duration);
        assert(nodes == expected_nodes[i - 1]);
    }
}

void test_perft_startpos(const int depth) {
    const char input[] = "position startpos";
    puts(input);
    for (int i = 1; i <= depth; i++) {
        Chess chess;
        chess.start_game();
        chess.nodes = 0;
        chess.uci->position_received(input);
        chess.max_time = 0;
        uint64_t start_time = Util::get_ms();
        uint64_t nodes = chess.perft(i);
        uint64_t stop_time = Util::get_ms();
        uint64_t duration = stop_time - start_time;
        printf("depth: %d nodes: %lu time: %lu ms knps: %lu\n", i, nodes,
               duration, duration == 0 ? 0 : nodes / duration);
        switch (i) {
        case 1:
            assert(nodes == 20);
            break;
        case 2:
            assert(nodes == 400);
            break;
        case 3:
            assert(nodes == 8902);
            break;
        case 4:
            assert(nodes == 197281);
            break;
        case 5:
            assert(nodes == 4865609);
            break;
        default:
            break;
        }
    }
    puts("");
}

void test_perft() {
    const time_t mytime = time(NULL);
    printf("%s", ctime(&mytime));
    test_perft_startpos(4);
    {
        const char input[] = "position fen "
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w "
            "KQkq - 0 1";
        const uint64_t expected_nodes[] = {48, 2039, 97862, 4085603, 193690690};
        test_perft_pos(input, 4, expected_nodes);
    }
    {
        const char input[] = "position fen "
                             "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -";
        const uint64_t expected_nodes[] = {14, 191, 2812, 43238, 674624};
        test_perft_pos(input, 5, expected_nodes);
    }
    {
        const char input[] = "position fen "
            "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1";
        const uint64_t expected_nodes[] = {6, 264, 9467, 422333, 15833292};
        test_perft_pos(input, 5, expected_nodes);
    }
}

void test_eval_depth_1() {
    const char input[] = "position fen 4k3/8/6p1/6P1/6p1/6P1/4K3/8 w - - 0 1";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 1;
    chess.default_seldepth = 0;
    chess.make_move();
}

void test_eval_depth_2() {
    const char input[] = "position fen 4k2n/8/6p1/6P1/6p1/6P1/8/4K2N w - - 0 1 ";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 5;
    chess.default_seldepth = 0;
    chess.make_move();
}

void test_speed() {
    const char input[] = "position fen r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 2;
    chess.make_move();
}

void test_bratko_kopec_1() {
    // 2018.01.23. linode, c++: info depth 5 seldepth 5 time 16964 nodes 1865184 nps 109949
    // 2018.01.23. core i5 c++: info depth 5 seldepth 5 time 8952 nodes 1865184 nps 208353
    const char input[] =
        "position fen 1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - "
        "- 0 1 bm Qd1+; id BK.01;";
    puts(input);
    Chess chess;
    chess.sort_alfarray = true;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 5;
    chess.default_seldepth = 0;
    chess.break_if_mate_found = false; // remove
    chess.make_move();
    assert(strcmp(Util::move2str(chess.best_move), "d6d1 ") == 0);
    assert(chess.mate_score == 21995);
    assert(chess.root_moves[0].value == 21995);
}

void test_bratko_kopec_1a() {
    const char input[] =
        "position fen 1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - "
        "- 0 1 bm Qd1+; id BK.01;";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 6;
    chess.default_seldepth = 0;
    chess.make_move();
    assert(strcmp(Util::move2str(chess.best_move), "d6d1 ") == 0);
    assert(chess.mate_score == 21995);
    assert(chess.root_moves[0].value == 21995);
    assert(chess.depth == 5);
}

void test_bratko_kopec_1b() {
    const char input[] =
        "position fen 1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - "
        "- 0 1 bm Qd1+; id BK.01;";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 1;
    chess.make_move();
    assert(strcmp(Util::move2str(chess.best_move), "d6d1 ") == 0);
    assert(chess.mate_score == 21995);
    assert(chess.root_moves[0].value == 21995);
    assert(chess.depth == 1);
}

void test_bratko_kopec_2() {
    const char input[] =
        "position fen 3r1k2/4npp1/1ppr3p/p6P/P2PPPP1/1NR5/5K2/2R5 w - "
        "- 0 1 bm d5; id BK.02;";
    puts(input);
    Chess chess;
    chess.sort_alfarray = true;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 5;
    chess.make_move();
    assert(strcmp(Util::move2str(chess.best_move), "d4d5 ") == 0);
}

void test_bratko_kopec_10() {
    const char input[] = "position fen "
        "3rr1k1/pp3pp1/1qn2np1/8/3p4/PP1R1P2/2P1NQPP/R1B3K1 b - - 0 "
        "1 bm Ne5; id BK.10;";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 5;
    chess.make_move();
    assert(strcmp(Util::move2str(chess.best_move), "c6e5 ") == 0);
}

void test_bratko_kopec_12() {
    const char input[] =
        "position fen r3r1k1/ppqb1ppp/8/4p1NQ/8/2P5/PP3PPP/R3R1K1 b - "
        "- 0 1 bm Bf5; id BK.12;";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 2;
    chess.make_move();
    assert(strcmp(Util::move2str(chess.best_move), "d7f5 ") == 0);
}

void test_bratko_kopec() {
    test_bratko_kopec_2();
    /*
    test_bratko_kopec_1();
    test_bratko_kopec_1a();
    test_bratko_kopec_1b();
    test_bratko_kopec_10();
    test_bratko_kopec_12();
    */
}

void test_calculate_evarray(Chess &chess) {
    for (int i = 0; i < chess.nof_legal_root_moves; i++) {
        chess.root_moves[i].move = 100 - i;
        chess.root_moves[i].value = i;
    }
    chess.calculate_evarray();
    for (int i = 0; i < chess.nof_legal_root_moves - 1; i++) {
        assert(chess.root_moves[i].value >= chess.root_moves[i + 1].value);
    }
}

void test_calculate_evarray_new(Chess &chess) {
    for (int i = 0; i < chess.nof_legal_root_moves; i++) {
        chess.root_moves[i].move = 100 - i;
        chess.root_moves[i].value = i;
    }
    /*
  for (int i = 0; i < chess.nof_legal_root_moves; i++) {
      printf("%2d. %2d %2d\n",
              i + 1,
              chess.root_moves[i].move,
              chess.root_moves[i].value);
  }
  */
    chess.calculate_evarray_new();
    /*
  for (int i = 0; i < chess.nof_legal_root_moves; i++) {
      printf("%2d. %2d %2d\n",
              i + 1,
              chess.root_moves[i].move,
              chess.root_moves[i].value);
  }
  */
    for (int i = 0; i < chess.nof_legal_root_moves - 1; i++) {
        assert(chess.root_moves[i].value >= chess.root_moves[i + 1].value);
    }
}

void test_mate_in_2() {
    const char input[] =
        "position fen 2bqkbn1/2pppp2/np2N3/r3P1p1/p2N2B1/5Q2/PPPPKPP1/RNB2r2 w "
        "KQkq - 0 1";
    puts(input);
    Chess chess;
    chess.start_game();
    chess.table->setboard(input);
    chess.max_time = 0;
    chess.gui_depth = 5;
    chess.default_seldepth = 0;
    chess.make_move();
}

void test() {
    test_move_h2h4();
}

int main(int argc, char *argv[]) {
    if (argc > 1) {
        if (strcmp(argv[1], "0") == 0) {
            test_speed();
        } else if (strcmp(argv[1], "1") == 0) {
            test();
        } else if (strcmp(argv[1], "2") == 0) {
            test_perft_startpos(4);
            test_perft();
        } else if (strcmp(argv[1], "3") == 0) {
            test_bratko_kopec();
        } else if (strcmp(argv[1], "4") == 0) {
            test_mate_in_2();
            test_eval_depth_1();
            test_eval_depth_2();
        }
    } else {
        // Start here
        // Waiting for GUI
        char *ret;
        char input[1001];
        while (true) {
            ret = fgets(input, 1000, stdin);
            if (ret) {
                if (strstr(input, "quit")) {
                    exit(EXIT_SUCCESS);
                } else if (strstr(input, "uci") || strstr(input, "isready")) {
                    break;
                }
            }
        }
        Chess chess;
        chess.sort_alfarray = true;
        chess.start_game();
        chess.processCommands(input);
    }
}

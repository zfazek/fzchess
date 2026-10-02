cmake_minimum_required(VERSION 3.5)

project(FZChess)

set (CMAKE_CXX_FLAGS "-Wall")
if (CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    FIND_PACKAGE(Threads)
    set(CMAKE_CXX_FLAGS "-Wall -Wextra -Werror -std=c++17 -g -O2 -pthread -funroll-loops")
endif()


add_subdirectory(src)

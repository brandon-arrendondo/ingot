# C++ compile/link/run check for ingot's --emit-tinyfsm output.
#
# Driven by `invoke test`. Compiles the generated dm_key_events_wrapper.cpp
# (which #includes the generated dm_key_events.hpp + key_definitions.h and
# the consumer-seam fsmlist.hpp) against real tinyfsm, then links and runs it.
#
# Required cache vars:
#   -DGEN_DIR=<dir>       ingot output generated WITH --emit-tinyfsm
#   -DTINYFSM_DIR=<dir>   directory containing tinyfsm.hpp
cmake_minimum_required(VERSION 3.10)
project(ingot_tinyfsm_fixture CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
add_compile_options(-Wall -Wextra)

if(NOT DEFINED GEN_DIR)
    message(FATAL_ERROR "Set GEN_DIR to the ingot-generated output dir (built with --emit-tinyfsm)")
endif()
if(NOT DEFINED TINYFSM_DIR)
    message(FATAL_ERROR "Set TINYFSM_DIR to the directory containing tinyfsm.hpp")
endif()

add_executable(tinyfsm_dispatch
    main.cpp
    ${GEN_DIR}/dm_key_events_wrapper.cpp
)
target_include_directories(tinyfsm_dispatch PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}   # fsmlist.hpp — the consumer seam
    ${GEN_DIR}                    # generated key_definitions.h, dm_key_events*.hpp
    ${TINYFSM_DIR}                # tinyfsm.hpp
)

enable_testing()
add_test(NAME tinyfsm_dispatch COMMAND tinyfsm_dispatch)

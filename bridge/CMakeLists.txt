cmake_minimum_required(VERSION 3.25)
project(ElarisLadybirdBridge LANGUAGES C CXX)

if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/../vendor/ladybird/CMakeLists.txt")
    message(FATAL_ERROR "Initialize the Ladybird submodule at vendor/ladybird first")
endif()

# Ladybird owns its dependency graph, generated headers, helper programs, and service targets.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../vendor/ladybird" "${CMAKE_CURRENT_BINARY_DIR}/ladybird")

add_library(elaris_ladybird SHARED src/bridge.cpp)
target_compile_features(elaris_ladybird PRIVATE cxx_std_20)
target_include_directories(elaris_ladybird PUBLIC include)
target_link_libraries(elaris_ladybird PRIVATE LibWebView LibWebCommon LibCore LibGfx LibURL)

if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
    target_compile_options(elaris_ladybird PRIVATE -Wall -Wextra -Wpedantic)
endif()

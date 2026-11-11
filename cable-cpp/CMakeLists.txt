cmake_minimum_required(VERSION 3.20)
project(cable_cpp LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_executable(cable
    core.cpp
    ui.cpp
    app.cpp
    key.cpp
    term_posix.cpp
    term_windows.cpp
    main.cpp
    bench.cpp
)

if(MSVC)
    target_compile_options(cable PRIVATE /O2 /W4 /utf-8)
else()
    target_compile_options(cable PRIVATE -O3 -DNDEBUG -Wall -Wextra)
endif()

if(WIN32)
    target_compile_definitions(cable PRIVATE _CRT_SECURE_NO_WARNINGS)
endif()

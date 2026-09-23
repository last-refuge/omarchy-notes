cmake_minimum_required(VERSION 3.24)

project(OmarchyNotes
    VERSION 0.1.0
    DESCRIPTION "Native notes for Omarchy"
    LANGUAGES CXX)

# Working name; the application ID is what desktop files, Hyprland rules and
# D-Bus activation will key on, so it lives in exactly one place.
set(ONOTES_APP_ID "app.omarchynotes.Notes" CACHE STRING "Reverse-DNS application ID")
set(ONOTES_BINARY_NAME "omarchy-notes")

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Qt 6.8 is the LTS floor; the editor relies on TextEdit.textDocument (6.7+).
find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2 Test)
find_package(PkgConfig REQUIRED)
pkg_check_modules(SQLITE3 REQUIRED IMPORTED_TARGET sqlite3>=3.35)

qt_standard_project_setup(REQUIRES 6.8)
include(GNUInstallDirs)

add_compile_options(-Wall -Wextra -Wpedantic)

add_subdirectory(src/core)
add_subdirectory(src/app)

include(CTest)
if(BUILD_TESTING)
    add_subdirectory(tests)
endif()

cmake_minimum_required (VERSION 3.9)
cmake_policy(SET CMP0048 NEW)
project (nix-fci VERSION 1.0.0)

find_package(PkgConfig REQUIRED)

find_package(nlohmann_json REQUIRED)

pkg_check_modules(NIX REQUIRED nix-cmd>=2.4)

pkg_check_modules(BOEHM REQUIRED bdw-gc)

find_path(BOOST_INCLUDE_DIR boost/format.hpp)
if(BOOST_INCLUDE_DIR STREQUAL "BOOST_INCLUDE_DIR-NOTFOUND")
  message(FATAL_ERROR "Could not find Boost formatting library.")
endif()

if(APPLE)
  set(CMAKE_MODULE_LINKER_FLAGS "${CMAKE_MODULE_LINKER_FLAGS} -flat_namespace -undefined suppress")
endif()

add_library(nix-fci MODULE main.cc)
target_include_directories(nix-fci PRIVATE ${NIX_INCLUDE_DIRS})
target_include_directories(nix-fci PRIVATE ${BOOST_INCLUDE_DIR})
target_include_directories(nix-fci PRIVATE ${BOEHM_INCLUDE_DIRS})
target_compile_options(nix-fci PRIVATE ${NIX_CFLAGS_OTHER})
target_compile_options(nix-fci PRIVATE ${BOEHM_CFLAGS_OTHER})
target_link_libraries(nix-fci nlohmann_json::nlohmann_json)
install(TARGETS nix-fci DESTINATION lib/nix/plugins)

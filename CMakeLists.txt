cmake_minimum_required(VERSION 3.24)
if(CMAKE_HOST_SYSTEM_NAME STREQUAL "Darwin" AND NOT CMAKE_OSX_DEPLOYMENT_TARGET)
  set(CMAKE_OSX_DEPLOYMENT_TARGET "12.0" CACHE STRING "Minimum macOS version" FORCE)
endif()
project(MohrFrameQt VERSION 0.9.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Widgets)
find_program(CARGO_EXECUTABLE cargo REQUIRED)
find_program(RUSTC_EXECUTABLE rustc REQUIRED)

set(RUST_PROFILE "release" CACHE STRING "Rust profile for the Qt application")
option(MOHR_BUILD_RUST_CORE "Build the Rust static library with Cargo" ON)
if(RUST_PROFILE STREQUAL "release")
  set(RUST_FLAG --release)
endif()
set(RUST_ROOT "${CMAKE_SOURCE_DIR}/target/${RUST_PROFILE}")
set(RUST_LIBRARY "${RUST_ROOT}/${CMAKE_STATIC_LIBRARY_PREFIX}mohr_frame${CMAKE_STATIC_LIBRARY_SUFFIX}")
if(WIN32)
  set(RUST_LIBRARY "${RUST_ROOT}/mohr_frame.lib")
endif()
set(MOHR_RUST_LIBRARY "${RUST_LIBRARY}" CACHE FILEPATH "Rust static library")

if(MOHR_BUILD_RUST_CORE)
  add_custom_target(rust_core ALL
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CMAKE_SOURCE_DIR}/target
      MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}
      RUSTC=${RUSTC_EXECUTABLE}
      ${CARGO_EXECUTABLE} build --locked --lib --no-default-features ${RUST_FLAG}
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    BYPRODUCTS ${RUST_LIBRARY}
    COMMENT "Building Rust engineering core and CXX bridge")
endif()

add_executable(mohr-frame-qt
  qt/main.cpp
  qt/main_window.cpp
  qt/main_window.h
  qt/geometry_editor.cpp
  qt/geometry_editor.h
  qt/property_editor.cpp
  qt/property_editor.h
  qt/frame_view.cpp
  qt/frame_view.h
  qt/resources.qrc)
if(MOHR_BUILD_RUST_CORE)
  add_dependencies(mohr-frame-qt rust_core)
endif()
target_include_directories(mohr-frame-qt PRIVATE
  "${CMAKE_SOURCE_DIR}/target/cxxbridge"
  "${CMAKE_SOURCE_DIR}/target/cxxbridge/rust")
target_link_libraries(mohr-frame-qt PRIVATE Qt6::Core Qt6::Gui Qt6::Widgets "${MOHR_RUST_LIBRARY}")
if(APPLE)
  target_link_libraries(mohr-frame-qt PRIVATE
    "-framework Security" "-framework CoreFoundation" "-framework SystemConfiguration"
    "-framework AppKit" "-framework Foundation" "-framework IOKit")
endif()
if(WIN32)
  target_link_libraries(mohr-frame-qt PRIVATE
    ws2_32 userenv bcrypt ntdll iphlpapi advapi32 crypt32 secur32)
endif()
set_target_properties(mohr-frame-qt PROPERTIES
  WIN32_EXECUTABLE TRUE
  MACOSX_BUNDLE TRUE
  MACOSX_BUNDLE_INFO_PLIST "${CMAKE_SOURCE_DIR}/packaging/macos/Info.plist"
  OUTPUT_NAME "Mohr Frame")

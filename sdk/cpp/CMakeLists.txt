# Reflow C++ SDK — header-only over libreflow_rt_capi.
#
# Two ways to consume this:
#
# 1) Drop into your tree:
#      add_subdirectory(third_party/reflow/sdk/cpp)
#      target_link_libraries(myapp PRIVATE reflow::cpp)
#
# 2) Install + find_package:
#      cmake --install <build> --prefix /usr/local
#      find_package(reflow REQUIRED)   # in your project
#      target_link_libraries(myapp PRIVATE reflow::cpp)
#
# Either path expects libreflow_rt_capi.{so,dylib,dll} to be reachable
# at link time. By default we look in CMAKE_PREFIX_PATH, then the
# standard system lib dirs. Override with REFLOW_RT_CAPI_LIB to point
# at a specific .so/.dylib/.dll (e.g. when consuming the GitHub Release
# tarballs published from sdk/go/v* tags).

cmake_minimum_required(VERSION 3.16)
project(reflow_cpp VERSION 0.2.4 LANGUAGES CXX)

option(REFLOW_CPP_BUILD_EXAMPLES "Build the bundled C++ examples" OFF)

add_library(reflow_cpp INTERFACE)
add_library(reflow::cpp ALIAS reflow_cpp)

target_include_directories(reflow_cpp INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
target_compile_features(reflow_cpp INTERFACE cxx_std_17)

# Try to locate libreflow_rt_capi. The user can override by setting
# REFLOW_RT_CAPI_LIB on the cache.
if(NOT DEFINED REFLOW_RT_CAPI_LIB)
    find_library(REFLOW_RT_CAPI_LIB
        NAMES reflow_rt_capi
        HINTS ${REFLOW_RT_CAPI_DIR}
              ${CMAKE_PREFIX_PATH}
              ${CMAKE_INSTALL_PREFIX}/lib
        PATH_SUFFIXES lib
    )
endif()

if(REFLOW_RT_CAPI_LIB)
    message(STATUS "reflow_cpp: linking ${REFLOW_RT_CAPI_LIB}")
    target_link_libraries(reflow_cpp INTERFACE ${REFLOW_RT_CAPI_LIB})
else()
    # Defer to the linker — assume the user passed it via LDFLAGS or
    # has it on the default link path. Emit a warning so they're not
    # surprised at link time.
    message(WARNING "reflow_cpp: libreflow_rt_capi not found via find_library; "
                    "falling back to bare -lreflow_rt_capi. Set REFLOW_RT_CAPI_LIB "
                    "to point at the dylib if linking fails.")
    target_link_libraries(reflow_cpp INTERFACE reflow_rt_capi)
endif()

if(REFLOW_CPP_BUILD_EXAMPLES)
    add_executable(reflow_cpp_quickstart examples/quickstart.cpp)
    target_link_libraries(reflow_cpp_quickstart PRIVATE reflow::cpp)
endif()

# install rules — `cmake --install` copies headers + a CMake config so
# downstream projects can `find_package(reflow)`.
install(DIRECTORY include/ DESTINATION include)
install(TARGETS reflow_cpp EXPORT reflow-targets)
install(EXPORT reflow-targets
        FILE reflow-targets.cmake
        NAMESPACE reflow::
        DESTINATION lib/cmake/reflow)

# Minimal config file. Consumers do `find_package(reflow REQUIRED)`.
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/reflow-config.cmake"
"include(\"\${CMAKE_CURRENT_LIST_DIR}/reflow-targets.cmake\")\n")
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/reflow-config.cmake"
        DESTINATION lib/cmake/reflow)

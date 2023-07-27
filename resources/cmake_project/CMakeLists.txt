cmake_minimum_required(VERSION 3.20)

set(IDF_TARGET $ENV{IDF_TARGET} CACHE STRING "IDF Build Target")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)

project(libespidf)

set(EXTRA_COMPONENT_DIRS $ENV{EXTRA_COMPONENT_DIRS})
set(COMPONENTS $ENV{ESP_IDF_COMPONENTS})
set(SDKCONFIG $ENV{SDKCONFIG})
set(SDKCONFIG_DEFAULTS $ENV{SDKCONFIG_DEFAULTS})

# used to query the components in the build script
idf_build_get_property(comps BUILD_COMPONENTS)
set(BUILD_COMPONENTS ${comps} CACHE STRING "all esp-idf components" FORCE)

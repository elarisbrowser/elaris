# Included through CMAKE_PROJECT_ladybird_INCLUDE. Defer target creation until
# Ladybird has declared its libraries, while keeping Ladybird the top-level
# project for its Rust rules and helper-process paths.
set(ELARIS_BRIDGE_DIR "${CMAKE_CURRENT_LIST_DIR}")

include("${ELARIS_BRIDGE_DIR}/Peek.cmake")

function(elaris_add_ladybird_bridge)
    elaris_patch_ladybird(peek
        Libraries/LibWeb/DOM/Element.cpp
        Libraries/LibWebView/WebContentPage.cpp
        Libraries/LibWebView/ViewImplementation.h)
    elaris_patch_ladybird(color-picker-cancel Libraries/LibWeb/HTML/HTMLInputElement.cpp)
    elaris_patch_ladybird(file-picker-document Libraries/LibWeb/Page/Page.cpp)
    elaris_patch_ladybird(external-url-host Libraries/LibWebView/ViewImplementation.cpp)
    elaris_patch_ladybird(audio-output Libraries/LibWebView/HelperProcess.cpp)
    target_include_directories(LibWebView PRIVATE "${ELARIS_BRIDGE_DIR}/src")
    elaris_patch_ladybird(private-request-servers
        Libraries/LibWebView/Application.cpp
        Libraries/LibWebView/RequestServerManager.cpp
        Libraries/LibWebView/RequestServerManager.h)
    if(TARGET harfbuzz AND ELARIS_VCPKG_PREFIX)
        # The installed vcpkg config incorrectly publishes `//include/harfbuzz`.
        set_property(TARGET harfbuzz PROPERTY INTERFACE_INCLUDE_DIRECTORIES
            "${ELARIS_VCPKG_PREFIX}/include/harfbuzz")
    endif()

    if(APPLE AND ELARIS_VCPKG_PREFIX)
        foreach(helper IN LISTS ladybird_helper_processes)
            if(TARGET ${helper})
                target_link_options(${helper} PRIVATE
                    "-Wl,-rpath,${CMAKE_BINARY_DIR}/lib"
                    "-Wl,-rpath,${ELARIS_VCPKG_PREFIX}/lib")
            endif()
        endforeach()
    endif()

    add_library(elaris_ladybird SHARED "${ELARIS_BRIDGE_DIR}/src/bridge.cpp")
    if(APPLE)
        set(objc_sources
            "${ELARIS_BRIDGE_DIR}/src/notification_backend_macos.mm"
            "${ELARIS_BRIDGE_DIR}/src/external_url_handler_macos.mm"
            "${LADYBIRD_SOURCE_DIR}/UI/Qt/MicrophoneAuthorizerMacOS.mm")
        target_sources(elaris_ladybird PRIVATE ${objc_sources})
        set_source_files_properties(${objc_sources} PROPERTIES COMPILE_FLAGS "-fobjc-arc")
        target_link_libraries(elaris_ladybird PRIVATE
            "-framework AVFoundation" "-framework CoreServices" "-framework UserNotifications" "-framework Cocoa")
    elseif(LINUX)
        find_package(PkgConfig REQUIRED)
        pkg_check_modules(ELARIS_GIO REQUIRED IMPORTED_TARGET gio-2.0)
        target_sources(elaris_ladybird PRIVATE "${ELARIS_BRIDGE_DIR}/src/external_url_handler_linux.cpp")
        target_link_libraries(elaris_ladybird PRIVATE PkgConfig::ELARIS_GIO)
    endif()
    target_compile_features(elaris_ladybird PRIVATE cxx_std_20)
    target_include_directories(elaris_ladybird PUBLIC "${ELARIS_BRIDGE_DIR}/include")
    target_compile_definitions(elaris_ladybird PRIVATE
        "ELARIS_LADYBIRD_HELPER_DIR=\"$<TARGET_FILE_DIR:WebContent>\"")
    if(ELARIS_VCPKG_PREFIX)
        set_property(TARGET elaris_ladybird APPEND PROPERTY BUILD_RPATH "${ELARIS_VCPKG_PREFIX}/lib")
        target_link_options(elaris_ladybird PRIVATE "-Wl,-rpath,${ELARIS_VCPKG_PREFIX}/lib")
    endif()
    get_target_property(patched_headers LibWebView ELARIS_PATCHED_INCLUDE_DIRS)
    target_include_directories(elaris_ladybird SYSTEM BEFORE PRIVATE ${patched_headers})
    target_include_directories(elaris_ladybird SYSTEM PRIVATE
        "${LADYBIRD_SOURCE_DIR}"
        "${LADYBIRD_SOURCE_DIR}/Libraries"
        "${LADYBIRD_SOURCE_DIR}/Services"
        "${CMAKE_BINARY_DIR}"
        "${CMAKE_BINARY_DIR}/Libraries"
        "${CMAKE_BINARY_DIR}/Services"
    )
    target_link_libraries(elaris_ladybird PRIVATE AK LibWebView LibWebCommon LibCore LibFileSystem LibGfx LibIPC LibURL)

    if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
        target_compile_options(elaris_ladybird PRIVATE -Wall -Wextra -Wpedantic)
    endif()
    if(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
        # Ladybird's application declaration macro expands AK::TRY's GNU statement expression.
        target_compile_options(elaris_ladybird PRIVATE -Wno-gnu-statement-expression-from-macro-expansion)
    endif()

    if(ELARIS_BUILD_SMOKE_TESTS)
        target_compile_definitions(elaris_ladybird PRIVATE ELARIS_BUILD_SMOKE_TESTS)
        add_executable(elaris_ladybird_smoke "${ELARIS_BRIDGE_DIR}/tests/smoke.cpp")
        target_compile_features(elaris_ladybird_smoke PRIVATE cxx_std_20)
        target_link_libraries(elaris_ladybird_smoke PRIVATE elaris_ladybird)
        if(ELARIS_VCPKG_PREFIX)
            set_property(TARGET elaris_ladybird_smoke APPEND PROPERTY BUILD_RPATH "${ELARIS_VCPKG_PREFIX}/lib")
            target_link_options(elaris_ladybird_smoke PRIVATE "-Wl,-rpath,${ELARIS_VCPKG_PREFIX}/lib")
        endif()
        add_test(NAME elaris_ladybird_smoke
            COMMAND elaris_ladybird_smoke
                "${LADYBIRD_SOURCE_DIR}/Base/res"
                "${CMAKE_BINARY_DIR}/elaris-smoke-profile"
                "${ELARIS_BRIDGE_DIR}/tests/smoke.html"
        )
        add_test(NAME elaris_ladybird_form_pickers
            COMMAND elaris_ladybird_smoke
                "${LADYBIRD_SOURCE_DIR}/Base/res"
                "${CMAKE_BINARY_DIR}/elaris-picker-smoke-profile"
                "${ELARIS_BRIDGE_DIR}/tests/smoke.html"
                --form-pickers
        )
        if(APPLE)
            set_tests_properties(elaris_ladybird_smoke elaris_ladybird_form_pickers PROPERTIES
                ENVIRONMENT "DYLD_LIBRARY_PATH=${CMAKE_BINARY_DIR}/lib:${ELARIS_VCPKG_PREFIX}/lib")
        elseif(UNIX)
            set_tests_properties(elaris_ladybird_smoke elaris_ladybird_form_pickers PROPERTIES
                ENVIRONMENT "LD_LIBRARY_PATH=${CMAKE_BINARY_DIR}/lib:${ELARIS_VCPKG_PREFIX}/lib")
        endif()

        add_executable(elaris_ladybird_download_smoke "${ELARIS_BRIDGE_DIR}/tests/download_smoke.cpp")
        target_compile_features(elaris_ladybird_download_smoke PRIVATE cxx_std_20)
        target_link_libraries(elaris_ladybird_download_smoke PRIVATE elaris_ladybird)
        if(ELARIS_VCPKG_PREFIX)
            set_property(TARGET elaris_ladybird_download_smoke APPEND PROPERTY BUILD_RPATH "${ELARIS_VCPKG_PREFIX}/lib")
            target_link_options(elaris_ladybird_download_smoke PRIVATE "-Wl,-rpath,${ELARIS_VCPKG_PREFIX}/lib")
        endif()
        add_test(NAME elaris_ladybird_download_smoke
            COMMAND elaris_ladybird_download_smoke
                "${LADYBIRD_SOURCE_DIR}/Base/res"
                "${CMAKE_BINARY_DIR}/elaris-download-smoke"
        )
        if(APPLE)
            set_tests_properties(elaris_ladybird_download_smoke PROPERTIES
                ENVIRONMENT "DYLD_LIBRARY_PATH=${CMAKE_BINARY_DIR}/lib:${ELARIS_VCPKG_PREFIX}/lib")
        elseif(UNIX)
            set_tests_properties(elaris_ladybird_download_smoke PROPERTIES
                ENVIRONMENT "LD_LIBRARY_PATH=${CMAKE_BINARY_DIR}/lib:${ELARIS_VCPKG_PREFIX}/lib")
        endif()
    endif()
endfunction()

cmake_language(DEFER DIRECTORY "${CMAKE_BINARY_DIR}" CALL elaris_add_ladybird_bridge)

# Build the small Elaris link-activation patch from copies of the upstream sources.
function(elaris_patch_peek)
    find_program(ELARIS_PATCH patch REQUIRED)
    set(patch_file "${ELARIS_BRIDGE_DIR}/patches/peek.patch")
    set(patched_dir "${CMAKE_BINARY_DIR}/elaris-peek")
    set(inputs
        "Libraries/LibWeb/DOM/Element.cpp"
        "Libraries/LibWebView/WebContentPage.cpp")
    foreach(input IN LISTS inputs)
        configure_file("${LADYBIRD_SOURCE_DIR}/${input}" "${patched_dir}/${input}.orig" COPYONLY)
    endforeach()

    # Patch staged inputs on every configure, then update the compiled copies only if they changed.
    foreach(input IN LISTS inputs)
        file(COPY_FILE "${patched_dir}/${input}.orig" "${patched_dir}/${input}")
    endforeach()
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${patch_file}")
    execute_process(
        COMMAND "${ELARIS_PATCH}" --batch --forward -p1 -i "${patch_file}"
        WORKING_DIRECTORY "${patched_dir}"
        COMMAND_ERROR_IS_FATAL ANY)
    foreach(input IN LISTS inputs)
        string(REGEX MATCH "Libraries/([^/]+)/(.*)" match "${input}")
        set(target "${CMAKE_MATCH_1}")
        set(relative_source "${CMAKE_MATCH_2}")
        configure_file("${patched_dir}/${input}" "${patched_dir}/compiled/${input}" COPYONLY)
        get_target_property(sources "${target}" SOURCES)
        if(NOT relative_source IN_LIST sources)
            message(FATAL_ERROR "Could not replace ${input} for Elaris Peek")
        endif()
        list(REMOVE_ITEM sources "${relative_source}")
        set_property(TARGET "${target}" PROPERTY SOURCES "${sources}")
        target_sources("${target}" PRIVATE "${patched_dir}/compiled/${input}")
    endforeach()
    target_include_directories(LibWebView PRIVATE "${ELARIS_BRIDGE_DIR}/src")
endfunction()

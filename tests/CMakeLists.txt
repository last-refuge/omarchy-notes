function(onotes_test name)
    add_executable(${name} ${name}.cpp ${ARGN})
    target_link_libraries(${name} PRIVATE notes_core Qt6::Test Qt6::Gui)
    add_test(NAME ${name} COMMAND ${name})
    set_tests_properties(${name} PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endfunction()

onotes_test(tst_document)
onotes_test(tst_theme)
onotes_test(tst_store)
onotes_test(tst_exchange)

# Child process for the kill -9 durability test.
add_executable(crash_helper crash_helper.cpp)
target_link_libraries(crash_helper PRIVATE notes_core)
target_compile_definitions(tst_store PRIVATE CRASH_HELPER_PATH="$<TARGET_FILE:crash_helper>")
add_dependencies(tst_store crash_helper)

if(TARGET notes_app_lib AND EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/tst_editor.cpp)
    onotes_test(tst_editor)
    target_link_libraries(tst_editor PRIVATE notes_app_lib notes_app_libplugin Qt6::Quick Qt6::Qml Qt6::QuickControls2)
    target_compile_definitions(tst_editor PRIVATE SCREENSHOT_DIR="${CMAKE_BINARY_DIR}/screenshots")
    set_tests_properties(tst_editor PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
endif()

qt_add_library(notes_app_lib STATIC)

qt_add_qml_module(notes_app_lib
    URI OmarchyNotes
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/OmarchyNotes
    SOURCES
        AppLibrary.cpp AppLibrary.h
        EditorController.cpp EditorController.h
        ThemeController.cpp ThemeController.h
    QML_FILES
        qml/Main.qml
        qml/NoteList.qml
        qml/EditorPane.qml
        qml/ToolIcon.qml
        qml/NoteLinkPicker.qml
        qml/Toast.qml
    RESOURCES
        icons/compose.svg
        icons/bullets.svg
        icons/numbers.svg
        icons/checklist.svg
        icons/table.svg
        icons/image.svg
        icons/attach.svg
        icons/link.svg
        icons/highlight.svg
        icons/back.svg
        icons/pin.svg
)

target_include_directories(notes_app_lib PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(notes_app_lib PUBLIC
    notes_core Qt6::Quick Qt6::Qml Qt6::QuickControls2)

qt_add_executable(${ONOTES_BINARY_NAME} main.cpp)
target_link_libraries(${ONOTES_BINARY_NAME} PRIVATE notes_app_lib notes_app_libplugin)
set_target_properties(${ONOTES_BINARY_NAME} PROPERTIES OUTPUT_NAME ${ONOTES_BINARY_NAME})

install(TARGETS ${ONOTES_BINARY_NAME} RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})

set(ONOTES_BIN "${CMAKE_INSTALL_FULL_BINDIR}/${ONOTES_BINARY_NAME}")
configure_file(${PROJECT_SOURCE_DIR}/data/app.omarchynotes.Notes.desktop.in
               ${CMAKE_CURRENT_BINARY_DIR}/${ONOTES_APP_ID}.desktop @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/${ONOTES_APP_ID}.desktop
        DESTINATION ${CMAKE_INSTALL_DATADIR}/applications)
install(FILES ${PROJECT_SOURCE_DIR}/data/app.omarchynotes.Notes.svg
        DESTINATION ${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps
        RENAME ${ONOTES_APP_ID}.svg)

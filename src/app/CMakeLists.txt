qt_add_library(notes_app_lib STATIC)

qt_add_qml_module(notes_app_lib
    URI OmarchyNotes
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/OmarchyNotes
    SOURCES
        AppController.cpp AppController.h
        AppLibrary.cpp AppLibrary.h
        InstanceChannel.cpp InstanceChannel.h
        BlobImageProvider.cpp BlobImageProvider.h
        EditorController.cpp EditorController.h
        ThemeController.cpp ThemeController.h
    QML_FILES
        qml/Main.qml
        qml/NoteList.qml
        qml/EditorPane.qml
        qml/ToolIcon.qml
        qml/NoteLinkPicker.qml
        qml/Toast.qml
        qml/Sidebar.qml
        qml/FolderNameDialog.qml
        qml/ConfirmDialog.qml
        qml/MoveNoteDialog.qml
        qml/SmartFolderDialog.qml
        qml/NoteMenu.qml
        qml/AttachmentsView.qml
        qml/QuickNoteWindow.qml
        qml/StartupError.qml
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
        icons/trash.svg
        icons/folder.svg
        icons/folder-new.svg
        icons/all-notes.svg
        icons/note.svg
        icons/search.svg
        icons/sidebar.svg
        icons/more.svg
        icons/recover.svg
        icons/close.svg
        icons/smart-folder.svg
        icons/tag.svg
        icons/gallery.svg
        icons/list.svg
        icons/share.svg
)

target_include_directories(notes_app_lib PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(notes_app_lib PUBLIC
    notes_core Qt6::Quick Qt6::Qml Qt6::QuickControls2 Qt6::Network)

qt_add_executable(${ONOTES_BINARY_NAME} main.cpp)
target_link_libraries(${ONOTES_BINARY_NAME} PRIVATE notes_app_lib notes_app_libplugin)
set_target_properties(${ONOTES_BINARY_NAME} PROPERTIES OUTPUT_NAME ${ONOTES_BINARY_NAME})

install(TARGETS ${ONOTES_BINARY_NAME} RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})

set(ONOTES_BIN "${CMAKE_INSTALL_FULL_BINDIR}/${ONOTES_BINARY_NAME}")
configure_file(${PROJECT_SOURCE_DIR}/packaging/desktop/org.omarchy.Notes.desktop.in
               ${CMAKE_CURRENT_BINARY_DIR}/${ONOTES_APP_ID}.desktop @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/${ONOTES_APP_ID}.desktop
        DESTINATION ${CMAKE_INSTALL_DATADIR}/applications)
install(FILES ${PROJECT_SOURCE_DIR}/packaging/icons/org.omarchy.Notes.svg
        DESTINATION ${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps
        RENAME ${ONOTES_APP_ID}.svg)
install(FILES ${PROJECT_SOURCE_DIR}/packaging/metainfo/org.omarchy.Notes.metainfo.xml
        DESTINATION ${CMAKE_INSTALL_DATADIR}/metainfo
        RENAME ${ONOTES_APP_ID}.metainfo.xml)

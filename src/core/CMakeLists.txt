add_library(notes_core STATIC
    BlobStore.cpp BlobStore.h
    Database.cpp Database.h
    DocumentConverter.cpp DocumentConverter.h
    LibraryPaths.cpp LibraryPaths.h
    LibraryService.cpp LibraryService.h
    NoteStore.cpp NoteStore.h
    RichDocument.cpp RichDocument.h
    SampleLibrary.cpp SampleLibrary.h
    ThemePalette.cpp ThemePalette.h
)

target_include_directories(notes_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(notes_core
    PUBLIC Qt6::Core Qt6::Gui
    PRIVATE PkgConfig::SQLITE3)
target_compile_definitions(notes_core PUBLIC
    ONOTES_APP_ID="${ONOTES_APP_ID}"
    ONOTES_VERSION="${PROJECT_VERSION}")

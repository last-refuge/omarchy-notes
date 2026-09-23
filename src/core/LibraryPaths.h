#pragma once

#include <QString>

namespace onotes {

// Where a library lives on disk. User data goes under XDG_DATA_HOME so that
// uninstalling the package never touches notes.
struct LibraryPaths {
    QString root;

    QString database() const;
    QString blobs() const;
    QString staging() const;

    // $OMARCHY_NOTES_DATA_DIR, else $XDG_DATA_HOME/omarchy-notes.
    static LibraryPaths standard();
    static LibraryPaths at(const QString &root);
};

// Cache for disposable files (e.g. copies handed to external viewers).
QString cacheDirectory();

} // namespace onotes

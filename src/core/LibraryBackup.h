#pragma once

#include "LibraryPaths.h"

#include <QString>

namespace onotes {

// A backup is a folder:
//   manifest.json      written last; its presence marks the backup complete
//   library.sqlite3    consistent snapshot (single file, rollback journal)
//   blobs/xx/<sha256>  every attachment the snapshot refers to
//
// NoteStore::backupTo writes one and NoteStore::inspectBackup validates one.
// The functions below swap files while the library is closed; LibraryService
// orchestrates closing and reopening around them.

// A folder name like "Omarchy Notes Backup 2026-09-23 1530".
QString defaultBackupName();

// Moves the current library files aside into `*safetyDir` (a sibling of the
// library folder) and copies the backup in. On failure the original files are
// put back.
bool replaceLibraryFiles(const LibraryPaths &paths, const QString &backupDir, QString *safetyDir,
                         QString *error);

// Undoes replaceLibraryFiles: removes the restored files and moves the
// originals back from `safetyDir`.
bool rollbackLibraryFiles(const LibraryPaths &paths, const QString &safetyDir, QString *error);

} // namespace onotes

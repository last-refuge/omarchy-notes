#pragma once

#include "LibraryPaths.h"

#include <QString>

#include <optional>

class QIODevice;

namespace onotes {

// Content-addressed attachment bytes: blobs/<first two hex>/<sha256>.
// Files are written to staging, fsynced, and renamed into place before any
// database row refers to them, so a committed reference always resolves.
class BlobStore
{
public:
    struct Installed {
        QString hash;
        qint64 size = 0;
    };

    explicit BlobStore(LibraryPaths paths);

    // Creates directories and discards staging files left by an interrupted run.
    bool prepare(QString *error);

    std::optional<Installed> install(QIODevice &source, QString *error);
    std::optional<Installed> installFile(const QString &path, QString *error);
    std::optional<Installed> installBytes(const QByteArray &bytes, QString *error);

    // Empty when `hash` is not a well-formed blob hash.
    QString path(const QString &hash) const;
    bool contains(const QString &hash) const;

private:
    LibraryPaths m_paths;
};

// fsync helpers; return false on failure.
bool syncFileDescriptor(int fd);
bool syncDirectory(const QString &path);

} // namespace onotes

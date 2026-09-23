#include "LibraryPaths.h"

#include <QDir>
#include <QStandardPaths>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {
constexpr auto kDirName = "omarchy-notes";
}

QString LibraryPaths::database() const
{
    return root + u"/library.sqlite3"_s;
}

QString LibraryPaths::blobs() const
{
    return root + u"/blobs"_s;
}

QString LibraryPaths::staging() const
{
    return root + u"/staging"_s;
}

LibraryPaths LibraryPaths::standard()
{
    const QString override = qEnvironmentVariable("OMARCHY_NOTES_DATA_DIR");
    if (!override.isEmpty())
        return at(override);
    const QString base = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
    return at(base + u'/' + QLatin1StringView(kDirName));
}

LibraryPaths LibraryPaths::at(const QString &root)
{
    return LibraryPaths{QDir::cleanPath(QDir(root).absolutePath())};
}

QString cacheDirectory()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation) + u'/'
        + QLatin1StringView(kDirName);
}

} // namespace onotes

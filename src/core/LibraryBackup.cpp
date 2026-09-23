#include "LibraryBackup.h"

#include "BlobStore.h"
#include "Database.h"
#include "NoteStore.h"

#include <QDateTime>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>

#include <cstdio>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

constexpr auto kFormat = "omarchy-notes-backup";
const QStringList kDatabaseFiles = {u"library.sqlite3"_s, u"library.sqlite3-wal"_s, u"library.sqlite3-shm"_s};

bool fail(QString *error, const QString &message)
{
    if (error)
        *error = message;
    return false;
}

bool copyDurably(const QString &from, const QString &to)
{
    if (!QFile::copy(from, to))
        return false;
    QFile copied(to);
    return copied.open(QIODevice::ReadWrite) && syncFileDescriptor(copied.handle());
}

bool writeDurably(const QString &path, const QByteArray &data)
{
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    return file.write(data) == data.size() && file.flush() && syncFileDescriptor(file.handle());
}

bool renamePath(const QString &from, const QString &to)
{
    return std::rename(QFile::encodeName(from).constData(), QFile::encodeName(to).constData()) == 0;
}

// Copies blobs/xx/<hash> files, fsyncing each file and directory.
bool copyBlobTree(const QString &fromRoot, const QString &toRoot, QString *error)
{
    if (!QDir().mkpath(toRoot))
        return fail(error, u"Couldn't create %1."_s.arg(toRoot));
    QDirIterator it(fromRoot, QDir::Files, QDirIterator::Subdirectories);
    QStringList shards;
    while (it.hasNext()) {
        const QFileInfo file(it.next());
        const QString shard = file.dir().dirName();
        const QString targetDir = toRoot + u'/' + shard;
        if (!shards.contains(shard)) {
            if (!QDir().mkpath(targetDir))
                return fail(error, u"Couldn't create %1."_s.arg(targetDir));
            shards << shard;
        }
        if (!copyDurably(file.filePath(), targetDir + u'/' + file.fileName()))
            return fail(error, u"Couldn't copy attachment %1."_s.arg(file.fileName()));
    }
    for (const QString &shard : shards)
        syncDirectory(toRoot + u'/' + shard);
    syncDirectory(toRoot);
    return true;
}

} // namespace

QString defaultBackupName()
{
    return u"Omarchy Notes Backup "_s + QDateTime::currentDateTime().toString(u"yyyy-MM-dd HHmm"_s);
}

std::optional<BackupManifest> NoteStore::backupTo(const QString &directory, QString *error)
{
    const QString target = QDir::cleanPath(directory);
    if (QFileInfo::exists(target)) {
        fail(error, u"%1 already exists. Choose a new name for the backup."_s.arg(target));
        return std::nullopt;
    }
    // Build in a temporary folder and rename at the end, so a half-written
    // backup never looks like a real one.
    const QString partial = target + u".partial"_s;
    QDir(partial).removeRecursively();
    if (!QDir().mkpath(partial)) {
        fail(error, u"Couldn't create the backup folder at %1."_s.arg(target));
        return std::nullopt;
    }
    auto abandon = [&](const QString &message) -> std::optional<BackupManifest> {
        QDir(partial).removeRecursively();
        fail(error, message);
        return std::nullopt;
    };

    const QString snapshotPath = partial + u"/library.sqlite3"_s;
    QString dbError;
    if (!backupDatabase(snapshotPath, &dbError))
        return abandon(u"Couldn't copy the notes database: %1"_s.arg(dbError));

    BackupManifest manifest;
    manifest.path = target;
    manifest.appVersion = QStringLiteral(ONOTES_VERSION);
    manifest.schemaVersion = SchemaVersion;
    manifest.createdAt = QDateTime::currentMSecsSinceEpoch();
    {
        Database snapshot;
        if (!snapshot.open(snapshotPath, &dbError))
            return abandon(dbError);
        // A standalone single file, readable without WAL side files.
        snapshot.exec("PRAGMA journal_mode=DELETE");
        Statement notes = snapshot.prepare("SELECT COUNT(*) FROM notes");
        manifest.notes = notes.next() ? int(notes.int64(0)) : 0;

        // Copy exactly the attachments the snapshot records. They're
        // content-addressed and never modified, so copying after the
        // snapshot is consistent.
        Statement blobs = snapshot.prepare("SELECT hash, size FROM blobs");
        QStringList shards;
        while (blobs.next()) {
            const QString hash = blobs.text(0);
            const QString source = m_blobs.path(hash);
            if (source.isEmpty() || QFileInfo(source).size() != blobs.int64(1))
                return abandon(u"An attachment is missing or damaged (%1). The backup was not written."_s.arg(hash.left(12)));
            const QString shardDir = partial + u"/blobs/"_s + hash.left(2);
            if (!shards.contains(hash.left(2))) {
                QDir().mkpath(shardDir);
                shards << hash.left(2);
            }
            if (!copyDurably(source, shardDir + u'/' + hash))
                return abandon(u"Couldn't copy an attachment into the backup."_s);
            ++manifest.blobs;
            manifest.bytes += blobs.int64(1);
        }
        for (const QString &shard : shards)
            syncDirectory(partial + u"/blobs/"_s + shard);
    }
    QDir().mkpath(partial + u"/blobs"_s);
    syncDirectory(partial + u"/blobs"_s);
    manifest.bytes += QFileInfo(snapshotPath).size();

    QJsonObject json;
    json[u"format"] = QLatin1StringView(kFormat);
    json[u"formatVersion"] = BackupManifest::FormatVersion;
    json[u"appVersion"] = manifest.appVersion;
    json[u"schemaVersion"] = manifest.schemaVersion;
    json[u"createdAt"] = manifest.createdAt;
    json[u"notes"] = manifest.notes;
    json[u"blobs"] = manifest.blobs;
    json[u"bytes"] = manifest.bytes;
    if (!writeDurably(partial + u"/manifest.json"_s, QJsonDocument(json).toJson()))
        return abandon(u"Couldn't finish writing the backup."_s);
    syncDirectory(partial);
    if (!renamePath(partial, target))
        return abandon(u"Couldn't finish writing the backup."_s);
    syncDirectory(QFileInfo(target).absolutePath());
    return manifest;
}

std::optional<BackupManifest> NoteStore::inspectBackup(const QString &directory, QString *error)
{
    const QString dir = QDir::cleanPath(directory);
    QFile manifestFile(dir + u"/manifest.json"_s);
    if (!manifestFile.open(QIODevice::ReadOnly)) {
        fail(error, u"This folder isn't an Omarchy Notes backup (no manifest.json)."_s);
        return std::nullopt;
    }
    const QJsonObject json = QJsonDocument::fromJson(manifestFile.readAll()).object();
    if (json[u"format"].toString() != QLatin1StringView(kFormat)) {
        fail(error, u"This folder isn't an Omarchy Notes backup."_s);
        return std::nullopt;
    }
    BackupManifest manifest;
    manifest.path = dir;
    manifest.appVersion = json[u"appVersion"].toString();
    manifest.schemaVersion = json[u"schemaVersion"].toInt();
    manifest.createdAt = json[u"createdAt"].toInteger();
    manifest.notes = json[u"notes"].toInt();
    manifest.blobs = json[u"blobs"].toInt();
    manifest.bytes = json[u"bytes"].toInteger();
    if (json[u"formatVersion"].toInt() > BackupManifest::FormatVersion || manifest.schemaVersion > SchemaVersion) {
        fail(error, u"This backup was made by a newer version of Omarchy Notes (%1). Update the app to restore it."_s
                        .arg(manifest.appVersion));
        return std::nullopt;
    }

    Database db;
    QString dbError;
    if (!db.openReadOnly(dir + u"/library.sqlite3"_s, &dbError)) {
        fail(error, u"The backup's notes database can't be opened: %1"_s.arg(dbError));
        return std::nullopt;
    }
    Statement check = db.prepare("PRAGMA quick_check");
    if (!check.next() || check.text(0) != u"ok") {
        fail(error, u"The backup's notes database is damaged."_s);
        return std::nullopt;
    }
    Statement blobs = db.prepare("SELECT hash, size FROM blobs");
    const BlobStore store(LibraryPaths::at(dir));
    while (blobs.next()) {
        const QString path = store.path(blobs.text(0));
        if (path.isEmpty() || QFileInfo(path).size() != blobs.int64(1)) {
            fail(error, u"The backup is incomplete: some attachments are missing."_s);
            return std::nullopt;
        }
    }
    return manifest;
}

bool replaceLibraryFiles(const LibraryPaths &paths, const QString &backupDir, QString *safetyDir,
                         QString *error)
{
    const QString root = paths.root;
    const QString safety = root + u".before-restore-"_s
        + QDateTime::currentDateTime().toString(u"yyyyMMdd-HHmmss"_s);
    if (!QDir().mkpath(safety))
        return fail(error, u"Couldn't set your current notes aside at %1."_s.arg(safety));

    // Set the current library aside. Renames within one filesystem are
    // atomic and cheap, and nothing is deleted.
    for (const QString &name : kDatabaseFiles) {
        const QString from = root + u'/' + name;
        if (QFileInfo::exists(from) && !renamePath(from, safety + u'/' + name))
            return fail(error, u"Couldn't set your current notes aside."_s);
    }
    if (QFileInfo::exists(paths.blobs()) && !renamePath(paths.blobs(), safety + u"/blobs"_s)) {
        rollbackLibraryFiles(paths, safety, nullptr);
        return fail(error, u"Couldn't set your current attachments aside."_s);
    }
    syncDirectory(root);
    syncDirectory(safety);

    QString copyError;
    if (!copyDurably(backupDir + u"/library.sqlite3"_s, paths.database())
        || !copyBlobTree(backupDir + u"/blobs"_s, paths.blobs(), &copyError)) {
        rollbackLibraryFiles(paths, safety, nullptr);
        return fail(error, copyError.isEmpty() ? u"Couldn't copy the backup into place."_s : copyError);
    }
    syncDirectory(root);
    if (safetyDir)
        *safetyDir = safety;
    return true;
}

bool rollbackLibraryFiles(const LibraryPaths &paths, const QString &safetyDir, QString *error)
{
    for (const QString &name : kDatabaseFiles)
        QFile::remove(paths.root + u'/' + name);
    QDir(paths.blobs()).removeRecursively();
    bool ok = true;
    for (const QString &name : kDatabaseFiles) {
        const QString from = safetyDir + u'/' + name;
        if (QFileInfo::exists(from))
            ok = renamePath(from, paths.root + u'/' + name) && ok;
    }
    if (QFileInfo::exists(safetyDir + u"/blobs"_s))
        ok = renamePath(safetyDir + u"/blobs"_s, paths.blobs()) && ok;
    syncDirectory(paths.root);
    if (ok)
        QDir().rmdir(safetyDir);
    else
        fail(error, u"Your original notes are still safe in %1."_s.arg(safetyDir));
    return ok;
}

} // namespace onotes

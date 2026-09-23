#include "BlobStore.h"

#include "RichDocument.h"

#include <QBuffer>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>

#include <cerrno>
#include <cstdio>
#include <cstring>
#include <fcntl.h>
#include <unistd.h>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

bool fail(QString *error, const QString &message)
{
    if (error)
        *error = message;
    return false;
}

QString errnoString()
{
    return QString::fromLocal8Bit(std::strerror(errno));
}

} // namespace

bool syncFileDescriptor(int fd)
{
    int rc;
    do {
        rc = ::fsync(fd);
    } while (rc != 0 && errno == EINTR);
    return rc == 0;
}

bool syncDirectory(const QString &path)
{
    const int fd = ::open(QFile::encodeName(path).constData(), O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0)
        return false;
    const bool ok = syncFileDescriptor(fd);
    ::close(fd);
    return ok;
}

BlobStore::BlobStore(LibraryPaths paths) : m_paths(std::move(paths)) {}

bool BlobStore::prepare(QString *error)
{
    QDir dir;
    if (!dir.mkpath(m_paths.blobs()) || !dir.mkpath(m_paths.staging()))
        return fail(error, u"Could not create the attachment directories in %1."_s.arg(m_paths.root));
    QDir staging(m_paths.staging());
    for (const QString &name : staging.entryList(QDir::Files | QDir::Hidden))
        staging.remove(name);
    return true;
}

std::optional<BlobStore::Installed> BlobStore::install(QIODevice &source, QString *error)
{
    const QString stagingPath = m_paths.staging() + u'/' + newId() + u".part"_s;
    QFile staged(stagingPath);
    if (!staged.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        fail(error, u"Could not write attachment: %1"_s.arg(staged.errorString()));
        return std::nullopt;
    }

    QCryptographicHash sha(QCryptographicHash::Sha256);
    qint64 total = 0;
    QByteArray chunk;
    while (!source.atEnd()) {
        chunk = source.read(1 << 20);
        if (chunk.isEmpty() && source.atEnd())
            break;
        if (chunk.isEmpty()) {
            staged.remove();
            fail(error, u"Could not read attachment: %1"_s.arg(source.errorString()));
            return std::nullopt;
        }
        sha.addData(chunk);
        if (staged.write(chunk) != chunk.size()) {
            const QString why = staged.errorString();
            staged.remove();
            fail(error, u"Could not write attachment: %1"_s.arg(why));
            return std::nullopt;
        }
        total += chunk.size();
    }
    if (!staged.flush() || !syncFileDescriptor(staged.handle())) {
        staged.remove();
        fail(error, u"Could not write attachment to disk: %1"_s.arg(errnoString()));
        return std::nullopt;
    }
    staged.close();

    const QString hash = QString::fromLatin1(sha.result().toHex());
    const QString target = path(hash);
    const QString shard = QFileInfo(target).absolutePath();
    if (!QFileInfo::exists(shard)) {
        if (!QDir().mkpath(shard) || !syncDirectory(m_paths.blobs())) {
            QFile::remove(stagingPath);
            fail(error, u"Could not create attachment directory."_s);
            return std::nullopt;
        }
    }

    if (QFileInfo::exists(target)) {
        // Same content already stored.
        QFile::remove(stagingPath);
    } else {
        if (std::rename(QFile::encodeName(stagingPath).constData(),
                        QFile::encodeName(target).constData()) != 0) {
            const QString why = errnoString();
            QFile::remove(stagingPath);
            fail(error, u"Could not store attachment: %1"_s.arg(why));
            return std::nullopt;
        }
        if (!syncDirectory(shard)) {
            fail(error, u"Could not store attachment durably: %1"_s.arg(errnoString()));
            return std::nullopt;
        }
    }
    return Installed{hash, total};
}

std::optional<BlobStore::Installed> BlobStore::installFile(const QString &filePath, QString *error)
{
    QFile file(filePath);
    if (!file.open(QIODevice::ReadOnly)) {
        fail(error, u"Could not open %1: %2"_s.arg(QFileInfo(filePath).fileName(), file.errorString()));
        return std::nullopt;
    }
    return install(file, error);
}

std::optional<BlobStore::Installed> BlobStore::installBytes(const QByteArray &bytes, QString *error)
{
    QBuffer buffer;
    buffer.setData(bytes);
    buffer.open(QIODevice::ReadOnly);
    return install(buffer, error);
}

QString BlobStore::path(const QString &hash) const
{
    if (!isValidBlobHash(hash))
        return {};
    return m_paths.blobs() + u'/' + hash.left(2) + u'/' + hash;
}

bool BlobStore::contains(const QString &hash) const
{
    const QString p = path(hash);
    return !p.isEmpty() && QFileInfo::exists(p);
}

} // namespace onotes

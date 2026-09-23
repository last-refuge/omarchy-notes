#include "BlobImageProvider.h"

#include "LibraryService.h"

#include <QImageReader>

BlobImageProvider::BlobImageProvider(onotes::LibraryService *service)
    : QQuickImageProvider(QQuickImageProvider::Image, QQmlImageProviderBase::ForceAsynchronousImageLoading)
    , m_service(service)
{
}

QImage BlobImageProvider::requestImage(const QString &id, QSize *size, const QSize &requestedSize)
{
    // blobPath() validates the hash, so ids can't reach other files.
    QImageReader reader(m_service->blobPath(id));
    reader.setAutoTransform(true);
    const QSize original = reader.size();
    if (size)
        *size = original;
    // Never decode more than a thumbnail needs (and cap unbounded requests).
    const QSize bound = requestedSize.isValid() && !requestedSize.isEmpty() ? requestedSize : QSize(1024, 1024);
    if (original.isValid() && (original.width() > bound.width() || original.height() > bound.height()))
        reader.setScaledSize(original.scaled(bound, Qt::KeepAspectRatioByExpanding));
    return reader.read();
}

#pragma once

#include <QQuickImageProvider>

namespace onotes {
class LibraryService;
}

// Serves stored images to QML as image://blob/<sha256>, decoded off the UI
// thread and scaled to the requested size (thumbnails in the gallery and
// attachment browser).
class BlobImageProvider : public QQuickImageProvider
{
public:
    explicit BlobImageProvider(onotes::LibraryService *service);
    QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override;

private:
    onotes::LibraryService *m_service;
};

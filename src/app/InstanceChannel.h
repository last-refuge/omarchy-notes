#pragma once

#include <QJsonObject>
#include <QLocalServer>
#include <QObject>
#include <QString>

namespace onotes {
struct LibraryPaths;
}

// Lets a second launch hand its request to the running instance instead of
// opening the library twice. One JSON object per connection, answered "ok".
class InstanceChannel : public QObject
{
    Q_OBJECT

public:
    using QObject::QObject;

    // A socket in $XDG_RUNTIME_DIR, one per library.
    static QString socketPath(const onotes::LibraryPaths &paths);
    // Delivers `request` to a running instance. False if none answered.
    static bool send(const QString &socketPath, const QJsonObject &request, int timeoutMs = 1500);

    bool listen(const QString &socketPath, QString *error);

signals:
    void requestReceived(const QJsonObject &request);

private:
    QLocalServer m_server;
};

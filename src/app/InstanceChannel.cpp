#include "InstanceChannel.h"

#include "LibraryPaths.h"

#include <QCryptographicHash>
#include <QDir>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QStandardPaths>

using namespace Qt::StringLiterals;

QString InstanceChannel::socketPath(const onotes::LibraryPaths &paths)
{
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty())
        runtime = QDir::tempPath();
    const QByteArray key = QCryptographicHash::hash(paths.root.toUtf8(), QCryptographicHash::Sha256).toHex().left(12);
    return runtime + u"/omarchy-notes-"_s + QString::fromLatin1(key) + u".sock"_s;
}

bool InstanceChannel::send(const QString &socketPath, const QJsonObject &request, int timeoutMs)
{
    QLocalSocket socket;
    socket.connectToServer(socketPath);
    if (!socket.waitForConnected(timeoutMs))
        return false;
    socket.write(QJsonDocument(request).toJson(QJsonDocument::Compact) + '\n');
    if (!socket.waitForBytesWritten(timeoutMs))
        return false;
    return socket.waitForReadyRead(timeoutMs) && socket.readAll().startsWith("ok");
}

bool InstanceChannel::listen(const QString &socketPath, QString *error)
{
    // Only called while holding the library lock, so a leftover socket file
    // belongs to a process that's gone.
    QLocalServer::removeServer(socketPath);
    m_server.setSocketOptions(QLocalServer::UserAccessOption);
    if (!m_server.listen(socketPath)) {
        if (error)
            *error = m_server.errorString();
        return false;
    }
    connect(&m_server, &QLocalServer::newConnection, this, [this] {
        while (QLocalSocket *socket = m_server.nextPendingConnection()) {
            connect(socket, &QLocalSocket::disconnected, socket, &QObject::deleteLater);
            connect(socket, &QLocalSocket::readyRead, this, [this, socket] {
                if (!socket->canReadLine())
                    return;
                const QJsonObject request = QJsonDocument::fromJson(socket->readLine()).object();
                socket->write("ok\n");
                socket->flush();
                socket->disconnectFromServer();
                if (!request.isEmpty())
                    emit requestReceived(request);
            });
        }
    });
    return true;
}

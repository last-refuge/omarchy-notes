// Test-only Qt oracle. Not linked into the Rust application.
#include "RichDocument.h"
#include <QCoreApplication>
#include <QLockFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <sqlite3.h>
#include <iostream>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    if (argc != 3) return 2;
    const QString root = QString::fromLocal8Bit(argv[2]);
    QLockFile lock(root + "/.lock");
    if (!lock.tryLock(0)) return 3;
    if (std::string(argv[1]) == "hold") {
        std::cout << "LOCKED" << std::endl;
        std::string line; std::getline(std::cin, line);
        return 0;
    }
    sqlite3 *db = nullptr;
    if (sqlite3_open_v2((root + "/library.sqlite3").toUtf8().constData(), &db, SQLITE_OPEN_READONLY, nullptr) != SQLITE_OK) return 4;
    sqlite3_stmt *query = nullptr;
    sqlite3_prepare_v2(db, "SELECT body,title,snippet,plain_text,body_schema FROM notes", -1, &query, nullptr);
    int count = 0;
    while (sqlite3_step(query) == SQLITE_ROW) {
        auto text = [&](int index) { return QString::fromUtf8(reinterpret_cast<const char *>(sqlite3_column_text(query,index))); };
        QString error;
        const auto document = onotes::RichDocument::fromJsonBytes(text(0).toUtf8(), &error);
        if (!document || document->title() != text(1) || document->snippet() != text(2)
            || document->plainText() != text(3) || sqlite3_column_int(query,4) != 1) {
            std::cerr << "Qt could not validate a Rust-written note: " << error.toStdString() << std::endl;
            return 5;
        }
        ++count;
    }
    sqlite3_finalize(query); sqlite3_close(db);
    std::cout << "Qt verified " << count << " notes and their derived text." << std::endl;
    return 0;
}

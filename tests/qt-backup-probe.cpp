// Test-only oracle using the supported Qt edition's actual backup implementation.
#include "NoteStore.h"
#include <QCoreApplication>
#include <QLockFile>
#include <iostream>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    if (argc < 3) return 2;
    const std::string mode(argv[1]);
    const QString root = QString::fromLocal8Bit(argv[2]);
    onotes::NoteStore store(onotes::LibraryPaths::at(root));
    QString error;
    if (mode == "inspect") {
        const auto backup = store.inspectBackup(root, &error);
        if (!backup) { std::cerr << error.toStdString() << '\n'; return 3; }
        std::cout << "Qt inspected schema " << backup->schemaVersion << ": " << backup->notes << " notes, " << backup->blobs << " blobs.\n";
        return 0;
    }
    QLockFile lock(root + "/.lock");
    if (!lock.tryLock(0)) return 4;
    if (!store.open(&error)) { std::cerr << error.toStdString() << '\n'; return 5; }
    if (!store.verify().isEmpty()) { std::cerr << store.verify().join("; ").toStdString() << '\n'; return 6; }
    if (mode == "backup" && argc == 4) {
        if (!store.backupTo(QString::fromLocal8Bit(argv[3]), &error)) {
            std::cerr << error.toStdString() << '\n'; return 7;
        }
        std::cout << "Qt created a format-1 backup.\n";
    } else if (mode == "verify") {
        std::cout << "Qt opened and verified the restored library.\n";
    } else { return 2; }
    return 0;
}

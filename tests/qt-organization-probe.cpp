// Test oracle against the supported Qt edition's actual folder/note APIs.
#include "NoteStore.h"
#include <QCoreApplication>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLockFile>
#include <iostream>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    if (argc != 2) return 2;
    const QString root = QString::fromLocal8Bit(argv[1]);
    auto require = [](bool condition, const QString &message) {
        if (!condition) { std::cerr << message.toStdString() << '\n'; std::exit(3); }
    };
    QFile input(root + "/expected.json");
    require(input.open(QIODevice::ReadOnly), "Missing expected state");
    const auto expected = QJsonDocument::fromJson(input.readAll()).object();
    QLockFile lock(root + "/.lock");
    require(lock.tryLock(0), "Library locked");
    onotes::NoteStore store(onotes::LibraryPaths::at(root));
    QString error;
    require(store.open(&error), error);
    require(store.verify().isEmpty(), "Library integrity failure");
    const auto folders = store.listFolders();
    require(folders.size() == expected["folders"].toArray().size(), "Folder count changed");
    for (const auto value : expected["folders"].toArray()) {
        const auto f = value.toObject();
        const auto found = std::find_if(folders.begin(), folders.end(), [&](const auto &v) { return v.id == f["id"].toString(); });
        require(found != folders.end(), "Missing stable folder identity");
        require(found->name == f["name"].toString() && found->parentId == f["parent"].toString(), "Folder path changed");
    }
    const auto original = expected["note"].toObject();
    const auto noteId = original["id"].toString();
    const auto note = store.loadNote(noteId, &error);
    require(note.has_value(), error);
    require(note->folderId == original["folder"].toString(), "Filing lost to content save");
    require(note->body.toJson() == original["body"].toObject(), "Rich content changed");
    require(note->revision == original["revision"].toInteger(), "Revision changed");
    require(store.listNotes(onotes::NoteQuery::folder(note->folderId)).size() == 1, "Folder note isolation failed");
    const auto work = expected["work"].toString();
    require(!store.createFolder("clients", work, &error), "Duplicate sibling accepted");
    require(store.renameFolder(expected["subject"].toString(), "Renamed by Qt", &error), error);
    const auto destination = store.createFolder("Research", work, &error);
    require(destination.has_value(), error);
    const auto child = store.createFolder("Clients", destination->id, &error);
    require(child.has_value(), error);
    require(store.moveNote(noteId, destination->id, &error), error);
    const auto moved = store.loadNote(noteId, &error);
    require(moved && moved->body.toJson() == note->body.toJson() && moved->revision == note->revision
        && moved->updatedAt == note->updatedAt, "Qt move changed note content");
    require(store.verify().isEmpty(), "Final library integrity failure");
    QFile output(root + "/qt-result.json");
    require(output.open(QIODevice::WriteOnly), "Could not write Qt result");
    require(output.write(QJsonDocument(QJsonObject{{"destination", destination->id}, {"child", child->id}}).toJson()) > 0, "Result write failed");
    std::cout << "PASS: Qt reads Rust folders and documents, rejects duplicate siblings, and writes compatible moves and nested folders.\n";
}

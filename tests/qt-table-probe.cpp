// Test oracle only: actual QTextTable merge and app-converter round trips.
#include "DocumentConverter.h"
#include <QGuiApplication>
#include <QFile>
#include <QJsonDocument>
#include <QTextDocument>
#include <QTextTable>
#include <QTextCursor>
#include <iostream>
using namespace onotes;
using namespace Qt::StringLiterals;
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    if (argc != 3) return 2;
    if (std::string(argv[1]) == "create") {
        QTextDocument text;
        QTextCursor cursor(&text);
        cursor.insertText(u"A table with room to grow"_s);
        cursor.insertBlock();
        QTextTable *table = cursor.insertTable(3, 3, DocumentConverter::tableFormat(DocumentStyle{}));
        const QStringList values{u"Merged ideas"_s, u"é and 👩‍👩‍👧‍👦"_s, u"Status"_s, u"Keep every paragraph"_s, u"中文"_s, u"Ready"_s, u"Owner"_s, u"Next step"_s, u"Friday"_s};
        for (int row=0; row<3; ++row) for (int col=0; col<3; ++col) {
            QTextCursor cell = table->cellAt(row,col).firstCursorPosition();
            QTextCharFormat format;
            if (row == 0) format.setFontWeight(QFont::Bold);
            if (col == 1) format.setFontItalic(true);
            cell.insertText(values[row*3+col],format);
        }
        table->mergeCells(0,0,2,2);
        QFile file(QString::fromLocal8Bit(argv[2]));
        if (!file.open(QIODevice::WriteOnly | QIODevice::NewOnly)) return 3;
        file.write(QJsonDocument(DocumentConverter::read(&text).toJson()).toJson(QJsonDocument::Indented));
        return 0;
    }
    QFile file(QString::fromLocal8Bit(argv[2]));
    if (!file.open(QIODevice::ReadOnly)) return 4;
    QString error;
    auto original=RichDocument::fromJsonBytes(file.readAll(), &error);
    if (!original) { std::cerr<<error.toStdString()<<std::endl;return 5; }
    QTextDocument text;
    DocumentConverter::load(*original,&text,DocumentStyle{});
    const auto restored=DocumentConverter::read(&text);
    if (!(restored == *original)) {
        std::cerr<<"Qt editable document changed the Rust table structure or content: "<<argv[2]<<std::endl;
        return 6;
    }
    std::cout<<"Qt editable table verified: "<<argv[2]<<std::endl;
    return 0;
}

#pragma once

#include <QJsonObject>
#include <QList>
#include <QString>

#include <optional>

namespace onotes {

// The persisted note format. This is the contract between the editor and
// storage: everything the editor can express must round-trip through it, and
// anything it cannot express is reported rather than silently dropped.
//
// Serialized as JSON:
//   { "schema": 1, "blocks": [ Block, ... ] }
// Block:
//   { "id": "...", "type": "paragraph", "spans": [ Span, ... ] }
//   { "id": "...", "type": "heading", "level": 1..3, "spans": [...] }
//   { "id": "...", "type": "list", "list": "bullet|ordered|check",
//     "indent": 0.., "checked": bool, "spans": [...] }
//   { "id": "...", "type": "table", "rows": n, "columns": n,
//     "cells": [ { "row", "col", "rowSpan", "colSpan", "blocks": [...] } ] }
// Span (exactly one of text / image / attachment):
//   { "text": "...", "marks": ["bold", ...], "href": "..." }
//   { "image": "<sha256>", "width": px, "height": px, "alt": "..." }
//   { "attachment": "<uuid>" }

enum Mark : quint32 {
    Bold = 1u << 0,
    Italic = 1u << 1,
    Underline = 1u << 2,
    Strikethrough = 1u << 3,
    Highlight = 1u << 4,
    Code = 1u << 5,
};
constexpr quint32 AllMarks = Bold | Italic | Underline | Strikethrough | Highlight | Code;

struct Span {
    enum class Kind { Text, Image, Attachment };

    Kind kind = Kind::Text;
    QString text;      // Text; soft line breaks are '\n'
    quint32 marks = 0; // Text
    QString href;      // Text
    QString ref;       // Image: blob hash; Attachment: attachment id
    int width = 0;     // Image, logical pixels (0 = natural size)
    int height = 0;
    QString alt;

    static Span plain(const QString &text, quint32 marks = 0, const QString &href = {});
    static Span image(const QString &blobHash, int width, int height, const QString &alt = {});
    static Span attachment(const QString &attachmentId);

    bool operator==(const Span &) const = default;
};

struct Block;

struct TableCell {
    int row = 0;
    int column = 0;
    int rowSpan = 1;
    int columnSpan = 1;
    QList<Block> blocks;

    bool operator==(const TableCell &) const;
};

enum class ListKind { None, Bullet, Ordered, Check };

struct Block {
    enum class Type { Paragraph, Heading, ListItem, Table };

    Type type = Type::Paragraph;
    QString id;
    int level = 0;      // Heading: 1..3
    ListKind list = ListKind::None;
    int indent = 0;     // ListItem: 0-based nesting depth
    bool checked = false;
    QList<Span> spans;  // Paragraph, Heading, ListItem
    int rows = 0;       // Table
    int columns = 0;
    QList<TableCell> cells;

    static Block paragraph(const QList<Span> &spans = {});
    static Block heading(int level, const QList<Span> &spans);
    static Block listItem(ListKind kind, int indent, const QList<Span> &spans, bool checked = false);

    QString plainText() const;
    bool isEmptyParagraph() const;
    bool operator==(const Block &) const;
};

struct RichDocument {
    static constexpr int SchemaVersion = 1;

    QList<Block> blocks;

    // The first non-empty line, like Apple Notes.
    QString title() const;
    // Text after the title, for list excerpts.
    QString snippet(int maxLength = 160) const;
    // All searchable text.
    QString plainText() const;

    QStringList referencedBlobs() const;
    QStringList referencedAttachments() const;

    // Gives every block and table a unique, stable id, keeping existing ones.
    void assignMissingIds();

    QJsonObject toJson() const;
    QByteArray toJsonBytes() const;
    static std::optional<RichDocument> fromJson(const QJsonObject &json, QString *error = nullptr);
    static std::optional<RichDocument> fromJsonBytes(const QByteArray &bytes, QString *error = nullptr);

    bool operator==(const RichDocument &) const = default;
};

QString newId();
bool isValidBlobHash(const QString &hash);

} // namespace onotes

Q_DECLARE_METATYPE(onotes::RichDocument)

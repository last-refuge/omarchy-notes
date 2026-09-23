#pragma once

#include <QByteArray>
#include <QString>

struct sqlite3;
struct sqlite3_stmt;

namespace onotes {

class Statement
{
public:
    Statement() = default;
    Statement(sqlite3 *db, const char *sql);
    Statement(Statement &&other) noexcept;
    Statement &operator=(Statement &&other) noexcept;
    Statement(const Statement &) = delete;
    Statement &operator=(const Statement &) = delete;
    ~Statement();

    bool isValid() const { return m_stmt != nullptr; }

    // Strings always bind as TEXT (a null QString binds ''); use bindNull for NULL.
    Statement &bind(int index, const QString &value);
    Statement &bind(int index, qint64 value);
    Statement &bindNull(int index);

    // Returns true while a row is available.
    bool next();
    // Runs to completion; true on success.
    bool exec();
    bool failed() const { return m_failed; }

    QString text(int column) const;
    qint64 int64(int column) const;
    bool isNull(int column) const;

private:
    sqlite3 *m_db = nullptr;
    sqlite3_stmt *m_stmt = nullptr;
    bool m_failed = false;
};

class Database
{
public:
    Database() = default;
    Database(const Database &) = delete;
    Database &operator=(const Database &) = delete;
    ~Database();

    bool open(const QString &path, QString *error);
    bool openReadOnly(const QString &path, QString *error);
    void close();
    bool isOpen() const { return m_db != nullptr; }

    bool exec(const char *sql);
    Statement prepare(const char *sql);
    QString lastError() const;
    int userVersion();
    // Rows changed by the most recent INSERT, UPDATE or DELETE.
    int changes() const;

    sqlite3 *handle() const { return m_db; }

private:
    bool openWithFlags(const QString &path, int flags, QString *error);

    sqlite3 *m_db = nullptr;
};

// BEGIN IMMEDIATE ... COMMIT, rolled back unless committed. Inside another
// transaction it becomes a savepoint, so helpers can use it freely.
class Transaction
{
public:
    explicit Transaction(Database &db);
    ~Transaction();
    bool isActive() const { return m_active; }
    bool commit();

private:
    Database &m_db;
    bool m_active = false;
    QByteArray m_savepoint; // non-empty when nested
};

} // namespace onotes

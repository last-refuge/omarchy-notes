#include "Database.h"

#include <QFile>

#include <sqlite3.h>

namespace onotes {

Statement::Statement(sqlite3 *db, const char *sql) : m_db(db)
{
    if (sqlite3_prepare_v2(db, sql, -1, &m_stmt, nullptr) != SQLITE_OK) {
        m_stmt = nullptr;
        m_failed = true;
    }
}

Statement::Statement(Statement &&other) noexcept
    : m_db(other.m_db), m_stmt(other.m_stmt), m_failed(other.m_failed)
{
    other.m_stmt = nullptr;
}

Statement &Statement::operator=(Statement &&other) noexcept
{
    if (this != &other) {
        sqlite3_finalize(m_stmt);
        m_db = other.m_db;
        m_stmt = other.m_stmt;
        m_failed = other.m_failed;
        other.m_stmt = nullptr;
    }
    return *this;
}

Statement::~Statement()
{
    sqlite3_finalize(m_stmt);
}

Statement &Statement::bind(int index, const QString &value)
{
    if (!m_stmt)
        return *this;
    const QByteArray utf8 = value.toUtf8();
    if (sqlite3_bind_text(m_stmt, index, utf8.constData(), int(utf8.size()), SQLITE_TRANSIENT) != SQLITE_OK)
        m_failed = true;
    return *this;
}

Statement &Statement::bind(int index, qint64 value)
{
    if (m_stmt && sqlite3_bind_int64(m_stmt, index, value) != SQLITE_OK)
        m_failed = true;
    return *this;
}

Statement &Statement::bindNull(int index)
{
    if (m_stmt && sqlite3_bind_null(m_stmt, index) != SQLITE_OK)
        m_failed = true;
    return *this;
}

bool Statement::next()
{
    if (!m_stmt || m_failed)
        return false;
    const int rc = sqlite3_step(m_stmt);
    if (rc == SQLITE_ROW)
        return true;
    if (rc != SQLITE_DONE)
        m_failed = true;
    return false;
}

bool Statement::exec()
{
    while (next()) {
    }
    return !m_failed;
}

QString Statement::text(int column) const
{
    const auto *data = reinterpret_cast<const char *>(sqlite3_column_text(m_stmt, column));
    if (!data)
        return {};
    return QString::fromUtf8(data, sqlite3_column_bytes(m_stmt, column));
}

qint64 Statement::int64(int column) const
{
    return sqlite3_column_int64(m_stmt, column);
}

bool Statement::isNull(int column) const
{
    return sqlite3_column_type(m_stmt, column) == SQLITE_NULL;
}

Database::~Database()
{
    close();
}

bool Database::open(const QString &path, QString *error)
{
    return openWithFlags(path, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_NOMUTEX, error);
}

bool Database::openReadOnly(const QString &path, QString *error)
{
    return openWithFlags(path, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX, error);
}

bool Database::openWithFlags(const QString &path, int flags, QString *error)
{
    close();
    const int rc = sqlite3_open_v2(QFile::encodeName(path).constData(), &m_db, flags, nullptr);
    if (rc != SQLITE_OK) {
        if (error)
            *error = m_db ? QString::fromUtf8(sqlite3_errmsg(m_db)) : QStringLiteral("out of memory");
        close();
        return false;
    }
    sqlite3_extended_result_codes(m_db, 1);
    sqlite3_busy_timeout(m_db, 5000);
    return true;
}

void Database::close()
{
    if (m_db) {
        sqlite3_close_v2(m_db);
        m_db = nullptr;
    }
}

bool Database::exec(const char *sql)
{
    return sqlite3_exec(m_db, sql, nullptr, nullptr, nullptr) == SQLITE_OK;
}

Statement Database::prepare(const char *sql)
{
    return Statement(m_db, sql);
}

QString Database::lastError() const
{
    return m_db ? QString::fromUtf8(sqlite3_errmsg(m_db)) : QString();
}

QString Database::friendlyError() const
{
    if (!m_db)
        return QStringLiteral("The notes library isn't open.");
    switch (sqlite3_extended_errcode(m_db) & 0xff) {
    case SQLITE_FULL:
        return QStringLiteral("Your disk is full. Free up some space and try again.");
    case SQLITE_READONLY:
        return QStringLiteral("Your notes folder is read-only, so changes can't be saved.");
    case SQLITE_IOERR:
        return QStringLiteral("The disk reported an error while saving.");
    case SQLITE_CORRUPT:
    case SQLITE_NOTADB:
        return QStringLiteral("The notes database is damaged. Restore it from a backup.");
    case SQLITE_BUSY:
    case SQLITE_LOCKED:
        return QStringLiteral("The notes database is busy. Try again in a moment.");
    case SQLITE_CANTOPEN:
        return QStringLiteral("The notes database can't be opened. Check the folder's permissions.");
    default:
        return lastError();
    }
}

int Database::changes() const
{
    return m_db ? sqlite3_changes(m_db) : 0;
}

int Database::userVersion()
{
    Statement s = prepare("PRAGMA user_version");
    return s.next() ? int(s.int64(0)) : -1;
}

Transaction::Transaction(Database &db) : m_db(db)
{
    if (sqlite3_get_autocommit(m_db.handle()) == 0) {
        static int counter = 0;
        m_savepoint = "tx" + QByteArray::number(++counter);
        m_active = m_db.exec(("SAVEPOINT " + m_savepoint).constData());
    } else {
        m_active = m_db.exec("BEGIN IMMEDIATE");
    }
}

Transaction::~Transaction()
{
    if (!m_active)
        return;
    if (m_savepoint.isEmpty()) {
        m_db.exec("ROLLBACK");
    } else {
        m_db.exec(("ROLLBACK TO " + m_savepoint).constData());
        m_db.exec(("RELEASE " + m_savepoint).constData());
    }
}

bool Transaction::commit()
{
    if (!m_active)
        return false;
    m_active = false;
    if (!m_savepoint.isEmpty())
        return m_db.exec(("RELEASE " + m_savepoint).constData());
    if (m_db.exec("COMMIT"))
        return true;
    m_db.exec("ROLLBACK");
    return false;
}

} // namespace onotes

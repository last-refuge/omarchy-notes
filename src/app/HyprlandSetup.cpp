#include "HyprlandSetup.h"

#include <QDateTime>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QProcess>
#include <QRegularExpression>
#include <QStandardPaths>

using namespace Qt::StringLiterals;

namespace {

QString read(const QString &path)
{
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? QString::fromUtf8(file.readAll()) : QString();
}

QString normalizeKeys(QString keys)
{
    return keys.toUpper().remove(u' ');
}

// Every "MODS + KEY" string bound with o.bind or hl.bind in `text`.
QStringList boundKeys(const QString &text)
{
    static const QRegularExpression bind(uR"re((?:o|hl)\.bind\(\s*"([^"]+)")re"_s);
    QStringList out;
    for (auto it = bind.globalMatch(text); it.hasNext();)
        out << normalizeKeys(it.next().captured(1));
    return out;
}

// Keys the user explicitly unbound (hl.unbind) are free again.
QStringList unboundKeys(const QString &text)
{
    static const QRegularExpression unbind(uR"re(hl\.unbind\(\s*"([^"]+)")re"_s);
    QStringList out;
    for (auto it = unbind.globalMatch(text); it.hasNext();)
        out << normalizeKeys(it.next().captured(1));
    return out;
}

bool append(const QString &path, const QString &lines, QStringList *backups)
{
    const QString backup = path + u".bak."_s + QString::number(QDateTime::currentSecsSinceEpoch());
    if (QFile::exists(path) && !QFile::copy(path, backup))
        return false;
    if (QFile::exists(backup))
        backups->append(backup);
    QFile file(path);
    if (!file.open(QIODevice::Append))
        return false;
    file.write(lines.toUtf8());
    return true;
}

} // namespace

HyprlandSetupResult setUpHyprland(const QString &keys)
{
    HyprlandSetupResult result;
    const QString hypr = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + u"/hypr"_s;
    const QString bindings = hypr + u"/bindings.lua"_s;
    const QString main = hypr + u"/hyprland.lua"_s;
    if (!QFile::exists(bindings) || !QFile::exists(main)) {
        result.messages << u"This doesn't look like an Omarchy setup (no %1 or %2). "
                           "See the README for doing this by hand."_s.arg(bindings, main);
        return result;
    }

    const QString userBindings = read(bindings);
    const QString userMain = read(main);
    const bool haveBinding = userBindings.contains(u"omarchy-notes --quick-note"_s);
    const bool haveRule = userMain.contains(u"org\\\\.omarchy\\\\.Notes"_s) || userMain.contains(u"org.omarchy.Notes"_s);

    QString additions;
    if (haveBinding) {
        result.messages << u"The Quick Note key binding is already set up."_s;
    } else {
        // Omarchy's own bindings count unless the user unbound them.
        QString defaults;
        const QString omarchy = qEnvironmentVariable("OMARCHY_PATH", u"/usr/share/omarchy"_s);
        QDirIterator it(omarchy + u"/default/hypr"_s, {u"*.lua"_s}, QDir::Files, QDirIterator::Subdirectories);
        while (it.hasNext())
            defaults += read(it.next()) + u'\n';
        const QString wanted = normalizeKeys(keys);
        const bool takenByDefault = boundKeys(defaults).contains(wanted) && !unboundKeys(userBindings).contains(wanted);
        if (boundKeys(userBindings).contains(wanted) || takenByDefault) {
            result.messages << u"%1 is already bound, so nothing was changed. Pick another key and add the "
                               "binding by hand (see the README)."_s.arg(keys);
            return result;
        }
        additions = u"\n-- Added by omarchy-notes --setup-hyprland\n"
                    "o.bind(\"%1\", \"Quick Note\", { launch = \"omarchy-notes --quick-note\" })\n"_s.arg(keys);
    }

    QStringList backups;
    if (!additions.isEmpty()) {
        if (!append(bindings, additions, &backups)) {
            result.messages << u"Couldn't update %1."_s.arg(bindings);
            return result;
        }
        result.messages << u"Added %1 for Quick Note to %2."_s.arg(keys, bindings);
    }
    if (haveRule) {
        result.messages << u"The Quick Note window rule is already set up."_s;
    } else {
        const QString rule = u"\n-- Added by omarchy-notes --setup-hyprland: float the Quick Note window\n"
                             "o.window({ class = \"^org\\\\.omarchy\\\\.Notes$\", title = \"^Quick Note$\" }, "
                             "{ tag = \"+floating-window\" })\n"_s;
        if (!append(main, rule, &backups)) {
            result.messages << u"Couldn't update %1."_s.arg(main);
            return result;
        }
        result.messages << u"Added a rule to %1 so Quick Note floats."_s.arg(main);
    }

    // Let Hyprland check the result; undo if it objects.
    if (qEnvironmentVariableIsSet("HYPRLAND_INSTANCE_SIGNATURE") && !backups.isEmpty()) {
        QProcess::execute(u"hyprctl"_s, {u"reload"_s});
        QProcess check;
        check.start(u"hyprctl"_s, {u"configerrors"_s});
        check.waitForFinished(5000);
        const QString errors = QString::fromUtf8(check.readAllStandardOutput()).trimmed();
        if (!errors.isEmpty() && !errors.contains(u"no errors"_s, Qt::CaseInsensitive)) {
            for (const QString &backup : backups) {
                const QString original = backup.section(u".bak."_s, 0, 0);
                QFile::remove(original);
                QFile::copy(backup, original);
            }
            QProcess::execute(u"hyprctl"_s, {u"reload"_s});
            result.messages << u"Hyprland reported a problem, so the changes were undone:"_s << errors;
            return result;
        }
    }
    if (!backups.isEmpty())
        result.messages << u"Backups: %1"_s.arg(backups.join(u", "_s));
    result.ok = true;
    return result;
}

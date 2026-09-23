#include "ThemeController.h"

#include <QDir>
#include <QFileInfo>
#include <QQmlEngine>

using namespace Qt::StringLiterals;

namespace {
ThemeController *s_instance = nullptr;
}

ThemeController::ThemeController(QObject *parent)
    : QObject(parent), m_palette(onotes::ThemePalette::loadActive())
{
    // Theme switches rewrite several files; coalesce them into one reload.
    m_debounce.setSingleShot(true);
    m_debounce.setInterval(150);
    connect(&m_debounce, &QTimer::timeout, this, &ThemeController::reload);
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, &m_debounce, qOverload<>(&QTimer::start));
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, &m_debounce, qOverload<>(&QTimer::start));
    rearmWatcher();
}

ThemeController *ThemeController::create(QQmlEngine *, QJSEngine *)
{
    Q_ASSERT(s_instance);
    QQmlEngine::setObjectOwnership(s_instance, QQmlEngine::CppOwnership);
    return s_instance;
}

void ThemeController::setInstance(ThemeController *instance)
{
    s_instance = instance;
}

ThemeController *ThemeController::instance()
{
    return s_instance;
}

void ThemeController::reload()
{
    onotes::ThemePalette next = onotes::ThemePalette::loadActive();
    rearmWatcher();
    if (next == m_palette)
        return;
    m_palette = std::move(next);
    emit changed();
}

void ThemeController::rearmWatcher()
{
    if (!m_watcher.files().isEmpty())
        m_watcher.removePaths(m_watcher.files());
    if (!m_watcher.directories().isEmpty())
        m_watcher.removePaths(m_watcher.directories());

    QStringList paths;
    for (const QString &dir : onotes::ThemePalette::candidateDirectories()) {
        const QString parent = QFileInfo(dir).absolutePath();
        for (const QString &path : {parent, dir, dir + u"/colors.toml"_s, dir + u"/alacritty.toml"_s}) {
            if (QFileInfo::exists(path) && !paths.contains(path))
                paths << path;
        }
    }
    if (!paths.isEmpty())
        m_watcher.addPaths(paths);
}

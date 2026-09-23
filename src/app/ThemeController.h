#pragma once

#include "ThemePalette.h"

#include <QColor>
#include <QFileSystemWatcher>
#include <QObject>
#include <QTimer>
#include <QtQml/qqmlregistration.h>

class QQmlEngine;
class QJSEngine;

// Exposes the active Omarchy palette to QML as `Theme` and follows theme
// switches live. Omarchy replaces the whole current/theme directory when the
// theme changes, so the parent directory is watched as well as the files.
class ThemeController : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Theme)
    QML_SINGLETON

    Q_PROPERTY(QString name READ name NOTIFY changed)
    Q_PROPERTY(QString source READ source NOTIFY changed)
    Q_PROPERTY(bool dark READ dark NOTIFY changed)
    Q_PROPERTY(QColor canvas READ canvas NOTIFY changed)
    Q_PROPERTY(QColor sidebar READ sidebar NOTIFY changed)
    Q_PROPERTY(QColor raised READ raised NOTIFY changed)
    Q_PROPERTY(QColor divider READ divider NOTIFY changed)
    Q_PROPERTY(QColor text READ text NOTIFY changed)
    Q_PROPERTY(QColor secondaryText READ secondaryText NOTIFY changed)
    Q_PROPERTY(QColor selection READ selection NOTIFY changed)
    Q_PROPERTY(QColor selectionText READ selectionText NOTIFY changed)
    Q_PROPERTY(QColor accent READ accent NOTIFY changed)
    Q_PROPERTY(QColor accentText READ accentText NOTIFY changed)
    Q_PROPERTY(QColor focus READ focus NOTIFY changed)
    Q_PROPERTY(QColor link READ link NOTIFY changed)
    Q_PROPERTY(QColor error READ error NOTIFY changed)
    Q_PROPERTY(QColor highlight READ highlight NOTIFY changed)
    Q_PROPERTY(QColor tableBorder READ tableBorder NOTIFY changed)

public:
    // Not default-constructible on purpose: QML must use create(), which
    // returns the one shared instance, rather than making its own.
    explicit ThemeController(QObject *parent);

    static ThemeController *create(QQmlEngine *, QJSEngine *);
    static void setInstance(ThemeController *instance);
    static ThemeController *instance();

    const onotes::ThemePalette &palette() const { return m_palette; }

    QString name() const { return m_palette.name; }
    QString source() const { return m_palette.source; }
    bool dark() const { return m_palette.dark; }
    QColor canvas() const { return m_palette.canvas; }
    QColor sidebar() const { return m_palette.sidebar; }
    QColor raised() const { return m_palette.raised; }
    QColor divider() const { return m_palette.divider; }
    QColor text() const { return m_palette.text; }
    QColor secondaryText() const { return m_palette.secondaryText; }
    QColor selection() const { return m_palette.selection; }
    QColor selectionText() const { return m_palette.selectionText; }
    QColor accent() const { return m_palette.accent; }
    QColor accentText() const { return m_palette.accentText; }
    QColor focus() const { return m_palette.focus; }
    QColor link() const { return m_palette.link; }
    QColor error() const { return m_palette.error; }
    QColor highlight() const { return m_palette.highlight; }
    QColor tableBorder() const { return m_palette.tableBorder; }

    Q_INVOKABLE void reload();

signals:
    void changed();

private:
    void rearmWatcher();

    onotes::ThemePalette m_palette;
    QFileSystemWatcher m_watcher;
    QTimer m_debounce;
};

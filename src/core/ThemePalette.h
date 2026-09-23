#pragma once

#include <QColor>
#include <QHash>
#include <QString>
#include <QStringList>

#include <optional>

namespace onotes {

// Semantic colors for the app, derived from the active Omarchy theme and
// corrected for contrast. The UI only ever uses these roles, never raw
// palette entries, so themes without a given key still render sensibly.
struct ThemePalette {
    QString name;
    QString source; // file the palette came from, or "built-in"
    bool dark = true;

    QColor canvas;        // window and editor background
    QColor sidebar;       // list / sidebar background
    QColor raised;        // hover, pressed and popup surfaces
    QColor divider;
    QColor text;
    QColor secondaryText; // timestamps, excerpts
    QColor selection;
    QColor selectionText;
    QColor accent;        // selected rows, primary actions
    QColor accentText;    // text on accent
    QColor focus;         // focus rings
    QColor link;
    QColor error;
    QColor highlight;     // text highlight mark (translucent)
    QColor tableBorder;

    // Contrast corrections that were applied, for diagnostics.
    QStringList adjustments;

    static ThemePalette builtIn(bool dark = true);

    // Omarchy 3.x+/4.x colors.toml (flat keys: background, foreground, accent...).
    static std::optional<ThemePalette> fromColorsToml(const QString &path, QString *error = nullptr);
    // Older themes that only ship alacritty.toml.
    static std::optional<ThemePalette> fromAlacrittyToml(const QString &path, QString *error = nullptr);

    // Loads from the first usable theme directory, else the built-in palette.
    static ThemePalette loadActive();
    static ThemePalette loadFromDirectory(const QString &dir);

    // Theme directories to try, most specific first:
    // $OMARCHY_NOTES_THEME_DIR, $XDG_STATE_HOME/omarchy/current/theme (4.x),
    // $XDG_CONFIG_HOME/omarchy/current/theme (3.x).
    static QStringList candidateDirectories();

    bool operator==(const ThemePalette &) const = default;
};

double relativeLuminance(const QColor &color);
double contrastRatio(const QColor &a, const QColor &b);
QColor mix(const QColor &a, const QColor &b, double t);
// Composites a translucent color over an opaque one.
QColor composite(const QColor &over, const QColor &under);

// Minimal TOML reader for theme files: [sections], key = "string" | bare.
// Keys are returned as "section.key" (or "key" at top level).
QHash<QString, QString> parseSimpleToml(const QString &text);

} // namespace onotes

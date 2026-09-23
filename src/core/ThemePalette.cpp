#include "ThemePalette.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QStandardPaths>

#include <cmath>

using namespace Qt::StringLiterals;

namespace onotes {

namespace {

constexpr double kTextContrast = 4.5;
constexpr double kUiContrast = 3.0;

QColor parseColor(const QString &value)
{
    QString v = value.trimmed();
    if (v.startsWith(u"0x"_s, Qt::CaseInsensitive))
        v = u'#' + v.mid(2);
    QColor c = QColor::fromString(v);
    return c.isValid() ? c : QColor();
}

QString readFile(const QString &path, QString *error)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        if (error)
            *error = file.errorString();
        return {};
    }
    return QString::fromUtf8(file.readAll());
}

// Moves `color` toward `target` until it reaches `ratio` against `background`.
QColor ensureContrast(QColor color, const QColor &background, const QColor &target, double ratio,
                      const QString &role, QStringList *log)
{
    if (contrastRatio(color, background) >= ratio)
        return color;
    for (int step = 1; step <= 20; ++step) {
        const QColor candidate = mix(color, target, step / 20.0);
        if (contrastRatio(candidate, background) >= ratio) {
            log->append(u"%1 adjusted for contrast"_s.arg(role));
            return candidate;
        }
    }
    log->append(u"%1 replaced for contrast"_s.arg(role));
    return target;
}

QColor bestTextOn(const QColor &background, const QColor &a, const QColor &b)
{
    return contrastRatio(a, background) >= contrastRatio(b, background) ? a : b;
}

bool nearlyEqual(const QColor &a, const QColor &b)
{
    return std::abs(a.redF() - b.redF()) + std::abs(a.greenF() - b.greenF())
        + std::abs(a.blueF() - b.blueF()) < 0.12;
}

struct Inputs {
    QColor background, foreground, accent, selection, muted;
    QColor darkBackground, dimForeground, red, yellow, blue;
    std::optional<bool> dark;
};

ThemePalette derive(const Inputs &in, const QString &name, const QString &source)
{
    ThemePalette p;
    p.name = name;
    p.source = source;
    p.dark = in.dark.value_or(relativeLuminance(in.background) < 0.4);

    const QColor bg = in.background;
    const QColor extreme = bestTextOn(bg, Qt::white, Qt::black);
    const QColor fg = ensureContrast(in.foreground, bg, extreme, kTextContrast, u"text"_s, &p.adjustments);
    const QColor away = p.dark ? Qt::black : Qt::white;

    p.canvas = bg;
    p.text = fg;
    p.sidebar = in.darkBackground.isValid() ? in.darkBackground
                                            : mix(bg, p.dark ? Qt::black : QColor(0x60, 0x60, 0x60), 0.06);
    // On low-contrast themes a tinted sidebar can push text below the
    // threshold; keep the sidebar on the canvas color then.
    if (contrastRatio(fg, p.sidebar) < kTextContrast)
        p.sidebar = bg;
    p.raised = mix(bg, fg, p.dark ? 0.08 : 0.06);
    p.divider = mix(bg, fg, p.dark ? 0.16 : 0.14);
    p.tableBorder = mix(bg, fg, 0.30);

    const QColor dim = in.dimForeground.isValid() ? in.dimForeground : mix(fg, bg, 0.40);
    // Secondary text sits on both the canvas and the sidebar.
    p.secondaryText = ensureContrast(dim, p.sidebar, fg, kTextContrast, u"secondary text"_s, &p.adjustments);
    p.secondaryText = ensureContrast(p.secondaryText, bg, fg, kTextContrast, u"secondary text"_s, &p.adjustments);

    p.selection = in.selection.isValid() ? in.selection : mix(bg, fg, 0.25);
    p.selectionText = bestTextOn(p.selection, fg, bg);
    if (contrastRatio(p.selectionText, p.selection) < kTextContrast) {
        p.selection = ensureContrast(p.selection, p.selectionText, away, kTextContrast,
                                     u"selection"_s, &p.adjustments);
    }

    // Some themes set accent == foreground; fall back to a hue so selected
    // and focused states stay distinguishable from text.
    QColor accent = in.accent.isValid() ? in.accent : in.blue;
    if (!accent.isValid() || nearlyEqual(accent, fg))
        accent = in.blue.isValid() ? in.blue : mix(fg, QColor(0x3b, 0x82, 0xf6), 0.6);
    p.accent = ensureContrast(accent, bg, fg, kUiContrast, u"accent"_s, &p.adjustments);
    p.accentText = bestTextOn(p.accent, bg, fg);
    p.focus = p.accent;

    const QColor link = in.blue.isValid() ? in.blue : p.accent;
    p.link = ensureContrast(link, bg, fg, kTextContrast, u"link"_s, &p.adjustments);
    const QColor red = in.red.isValid() ? in.red : QColor(0xd0, 0x3a, 0x3a);
    p.error = ensureContrast(red, bg, fg, kTextContrast, u"error"_s, &p.adjustments);

    QColor yellow = in.yellow.isValid() ? in.yellow : QColor(0xf2, 0xc9, 0x4c);
    for (double alpha = p.dark ? 0.40 : 0.55; alpha >= 0.10; alpha -= 0.05) {
        yellow.setAlphaF(alpha);
        if (contrastRatio(fg, composite(yellow, bg)) >= kTextContrast)
            break;
    }
    p.highlight = yellow;
    return p;
}

QString themeNameFor(const QString &dir)
{
    const QFileInfo info(dir);
    const QString nameFile = info.absolutePath() + u"/theme.name"_s;
    QFile file(nameFile);
    if (file.open(QIODevice::ReadOnly))
        return QString::fromUtf8(file.readAll()).trimmed();
    if (info.isSymLink())
        return QFileInfo(info.symLinkTarget()).fileName();
    return info.fileName();
}

} // namespace

double relativeLuminance(const QColor &color)
{
    auto channel = [](double c) {
        return c <= 0.03928 ? c / 12.92 : std::pow((c + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * channel(color.redF()) + 0.7152 * channel(color.greenF())
        + 0.0722 * channel(color.blueF());
}

double contrastRatio(const QColor &a, const QColor &b)
{
    const double la = relativeLuminance(a);
    const double lb = relativeLuminance(b);
    return (std::max(la, lb) + 0.05) / (std::min(la, lb) + 0.05);
}

QColor mix(const QColor &a, const QColor &b, double t)
{
    return QColor::fromRgbF(float(a.redF() * (1 - t) + b.redF() * t),
                            float(a.greenF() * (1 - t) + b.greenF() * t),
                            float(a.blueF() * (1 - t) + b.blueF() * t));
}

QColor composite(const QColor &over, const QColor &under)
{
    return mix(under, QColor::fromRgbF(over.redF(), over.greenF(), over.blueF()), over.alphaF());
}

QHash<QString, QString> parseSimpleToml(const QString &text)
{
    QHash<QString, QString> out;
    QString section;
    for (QString line : text.split(u'\n')) {
        line = line.trimmed();
        if (line.isEmpty() || line.startsWith(u'#'))
            continue;
        if (line.startsWith(u'[')) {
            const qsizetype end = line.indexOf(u']');
            if (end > 0)
                section = line.mid(1, end - 1).trimmed().remove(u'"');
            continue;
        }
        const qsizetype eq = line.indexOf(u'=');
        if (eq <= 0)
            continue;
        const QString key = line.left(eq).trimmed().remove(u'"');
        QString value = line.mid(eq + 1).trimmed();
        if (value.startsWith(u'"') || value.startsWith(u'\'')) {
            const QChar quote = value.front();
            const qsizetype close = value.indexOf(quote, 1);
            value = close > 0 ? value.mid(1, close - 1) : value.mid(1);
        } else {
            const qsizetype comment = value.indexOf(u'#');
            if (comment >= 0)
                value = value.left(comment).trimmed();
        }
        out.insert(section.isEmpty() ? key : section + u'.' + key, value);
    }
    return out;
}

ThemePalette ThemePalette::builtIn(bool dark)
{
    Inputs in;
    in.dark = dark;
    if (dark) {
        in.background = QColor(0x1a, 0x1b, 0x26);
        in.foreground = QColor(0xc0, 0xca, 0xf5);
        in.accent = QColor(0x7a, 0xa2, 0xf7);
        in.selection = QColor(0x33, 0x46, 0x7c);
        in.dimForeground = QColor(0x9a, 0xa5, 0xce);
        in.blue = QColor(0x7a, 0xa2, 0xf7);
        in.red = QColor(0xf7, 0x76, 0x8e);
        in.yellow = QColor(0xe0, 0xaf, 0x68);
    } else {
        in.background = QColor(0xfb, 0xfb, 0xfa);
        in.foreground = QColor(0x1f, 0x1f, 0x1f);
        in.accent = QColor(0x2f, 0x6f, 0xd0);
        in.selection = QColor(0xc9, 0xdc, 0xf7);
        in.dimForeground = QColor(0x5f, 0x5f, 0x5f);
        in.blue = QColor(0x2f, 0x6f, 0xd0);
        in.red = QColor(0xc0, 0x2b, 0x2b);
        in.yellow = QColor(0xf2, 0xc9, 0x4c);
    }
    return derive(in, dark ? u"Built-in dark"_s : u"Built-in light"_s, u"built-in"_s);
}

std::optional<ThemePalette> ThemePalette::fromColorsToml(const QString &path, QString *error)
{
    const QString text = readFile(path, error);
    if (text.isNull())
        return std::nullopt;
    const auto keys = parseSimpleToml(text);
    auto color = [&](const char *key) { return parseColor(keys.value(QLatin1StringView(key))); };

    Inputs in;
    in.background = color("background");
    in.foreground = color("foreground");
    if (!in.background.isValid() || !in.foreground.isValid()) {
        if (error)
            *error = u"colors.toml has no valid background/foreground"_s;
        return std::nullopt;
    }
    in.accent = color("accent");
    in.selection = color("selection");
    in.darkBackground = color("dark_background");
    in.dimForeground = color("dark_foreground");
    in.red = color("red");
    in.yellow = color("yellow");
    in.blue = color("blue");
    const QString mode = keys.value(u"mode"_s).toLower();
    if (mode == u"dark")
        in.dark = true;
    else if (mode == u"light")
        in.dark = false;
    return derive(in, themeNameFor(QFileInfo(path).absolutePath()), path);
}

std::optional<ThemePalette> ThemePalette::fromAlacrittyToml(const QString &path, QString *error)
{
    const QString text = readFile(path, error);
    if (text.isNull())
        return std::nullopt;
    const auto keys = parseSimpleToml(text);
    auto color = [&](const char *key) { return parseColor(keys.value(QLatin1StringView(key))); };

    Inputs in;
    in.background = color("colors.primary.background");
    in.foreground = color("colors.primary.foreground");
    if (!in.background.isValid() || !in.foreground.isValid()) {
        if (error)
            *error = u"alacritty.toml has no primary colors"_s;
        return std::nullopt;
    }
    in.selection = color("colors.selection.background");
    in.dimForeground = color("colors.primary.dim_foreground");
    in.red = color("colors.normal.red");
    in.yellow = color("colors.normal.yellow");
    in.blue = color("colors.normal.blue");
    in.accent = in.blue;
    return derive(in, themeNameFor(QFileInfo(path).absolutePath()), path);
}

QStringList ThemePalette::candidateDirectories()
{
    QStringList dirs;
    const QString override = qEnvironmentVariable("OMARCHY_NOTES_THEME_DIR");
    if (!override.isEmpty())
        dirs << override;
    QString state = qEnvironmentVariable("XDG_STATE_HOME");
    if (state.isEmpty())
        state = QDir::homePath() + u"/.local/state"_s;
    dirs << state + u"/omarchy/current/theme"_s;
    dirs << QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation)
            + u"/omarchy/current/theme"_s;
    return dirs;
}

ThemePalette ThemePalette::loadFromDirectory(const QString &dir)
{
    if (auto p = fromColorsToml(dir + u"/colors.toml"_s))
        return *p;
    if (auto p = fromAlacrittyToml(dir + u"/alacritty.toml"_s))
        return *p;
    return builtIn(true);
}

ThemePalette ThemePalette::loadActive()
{
    for (const QString &dir : candidateDirectories()) {
        if (auto p = fromColorsToml(dir + u"/colors.toml"_s))
            return *p;
        if (auto p = fromAlacrittyToml(dir + u"/alacritty.toml"_s))
            return *p;
    }
    return builtIn(true);
}

} // namespace onotes

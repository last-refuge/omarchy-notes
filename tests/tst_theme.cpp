#include "ThemePalette.h"

#include <QFile>
#include <QTemporaryDir>
#include <QTest>

using namespace onotes;
using namespace Qt::StringLiterals;

namespace {

void write(const QString &path, const QByteArray &content)
{
    QFile file(path);
    QVERIFY(file.open(QIODevice::WriteOnly));
    file.write(content);
}

void verifyContrast(const ThemePalette &p)
{
    QVERIFY2(contrastRatio(p.text, p.canvas) >= 4.5, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.secondaryText, p.canvas) >= 4.5, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.secondaryText, p.sidebar) >= 4.5, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.selectionText, p.selection) >= 4.5, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.link, p.canvas) >= 4.5, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.error, p.canvas) >= 4.5, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.accent, p.canvas) >= 3.0, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.accentText, p.accent) >= 3.0, qPrintable(p.name));
    QVERIFY2(contrastRatio(p.text, composite(p.highlight, p.canvas)) >= 4.5, qPrintable(p.name));
}

// Kanagawa as shipped with Omarchy 4.0 alpha: accent equals foreground and
// dark_foreground is below text contrast on the background.
const QByteArray kKanagawa = R"(mode = "dark"

accent = "#dcd7ba"
selection = "#363646"
muted = "#54546D"

background = "#1f1f28"
dark_background = "#17171e"
foreground = "#dcd7ba"
dark_foreground = "#727169"

red = "#c34043"
yellow = "#c0a36e"
blue = "#7e9cd8"
)";

const QByteArray kLight = R"(mode = "light"
accent = "#d7827e"
selection = "#dfdad9"
background = "#faf4ed"
foreground = "#575279"
red = "#b4637a"
yellow = "#ea9d34"
blue = "#286983"
)";

} // namespace

class TestTheme : public QObject
{
    Q_OBJECT

private slots:
    void parsesToml()
    {
        const auto keys = parseSimpleToml(
            u"# comment\nname = \"x # not a comment\"\n[colors.primary]\nbackground = '0x1f1f28' \n"
            "bare = 12 # trailing\n"_s);
        QCOMPARE(keys.value(u"name"_s), u"x # not a comment"_s);
        QCOMPARE(keys.value(u"colors.primary.background"_s), u"0x1f1f28"_s);
        QCOMPARE(keys.value(u"colors.primary.bare"_s), u"12"_s);
    }

    void kanagawa()
    {
        QTemporaryDir dir;
        write(dir.filePath(u"colors.toml"_s), kKanagawa);
        const auto p = ThemePalette::fromColorsToml(dir.filePath(u"colors.toml"_s));
        QVERIFY(p);
        QVERIFY(p->dark);
        QCOMPARE(p->canvas, QColor(u"#1f1f28"_s));
        QCOMPARE(p->sidebar, QColor(u"#17171e"_s));
        // accent == foreground, so the blue is used for selection states.
        QCOMPARE(p->accent, QColor(u"#7e9cd8"_s));
        verifyContrast(*p);
        QVERIFY(!p->adjustments.isEmpty()); // dark_foreground was corrected
    }

    void light()
    {
        QTemporaryDir dir;
        write(dir.filePath(u"colors.toml"_s), kLight);
        const auto p = ThemePalette::fromColorsToml(dir.filePath(u"colors.toml"_s));
        QVERIFY(p);
        QVERIFY(!p->dark);
        verifyContrast(*p);
    }

    void builtIns()
    {
        verifyContrast(ThemePalette::builtIn(true));
        verifyContrast(ThemePalette::builtIn(false));
    }

    void alacrittyFallback()
    {
        QTemporaryDir dir;
        write(dir.filePath(u"alacritty.toml"_s), R"([colors.primary]
background = "0x24273a"
foreground = "0xcad3f5"
[colors.normal]
red = "0xed8796"
yellow = "0xeed49f"
blue = "0x8aadf4"
)");
        const ThemePalette p = ThemePalette::loadFromDirectory(dir.path());
        QCOMPARE(p.source, dir.filePath(u"alacritty.toml"_s));
        QCOMPARE(p.canvas, QColor(u"#24273a"_s));
        verifyContrast(p);
    }

    void invalidFilesFallBack()
    {
        QTemporaryDir dir;
        write(dir.filePath(u"colors.toml"_s), "background = \"not a color\"\n");
        const ThemePalette p = ThemePalette::loadFromDirectory(dir.path());
        QCOMPARE(p.source, u"built-in"_s);
        verifyContrast(p);
        QCOMPARE(ThemePalette::loadFromDirectory(dir.filePath(u"missing"_s)).source, u"built-in"_s);
    }

    void unreadableContrastIsRepaired()
    {
        QTemporaryDir dir;
        write(dir.filePath(u"colors.toml"_s),
              "background = \"#808080\"\nforeground = \"#8a8a8a\"\nselection = \"#858585\"\n");
        const auto p = ThemePalette::fromColorsToml(dir.filePath(u"colors.toml"_s));
        QVERIFY(p);
        verifyContrast(*p);
    }
};

QTEST_MAIN(TestTheme)
#include "tst_theme.moc"

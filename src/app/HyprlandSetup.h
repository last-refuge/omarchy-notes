#pragma once

#include <QString>
#include <QStringList>

// Optional Omarchy/Hyprland integration, run only on request
// (omarchy-notes --setup-hyprland): a Quick Note key binding and a rule that
// floats the Quick Note window. Conflicting bindings are never overwritten.
struct HyprlandSetupResult {
    bool ok = false;
    QStringList messages; // what happened, one line each
};

HyprlandSetupResult setUpHyprland(const QString &keys = QStringLiteral("SUPER + ALT + N"));

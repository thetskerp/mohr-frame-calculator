#pragma once

#include <QApplication>
#include <QColor>
#include <QPalette>
#include <QStyleHints>

// 0 — системная, 1 — светлая, 2 — тёмная.
inline bool darkTheme(int choice) {
  return choice == 2 || (choice == 0 &&
    QApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark);
}

inline void applyTheme(int choice) {
  const bool dark = darkTheme(choice);
  QPalette p;
  const QColor window = dark ? QColor("#1c2026") : QColor("#f2f4f7");
  const QColor base = dark ? QColor("#161b22") : QColor("#ffffff");
  const QColor text = dark ? QColor("#e8edf3") : QColor("#202a35");
  const QColor button = dark ? QColor("#303843") : QColor("#e1e7ee");
  p.setColor(QPalette::Window, window);
  p.setColor(QPalette::WindowText, text);
  p.setColor(QPalette::Base, base);
  p.setColor(QPalette::AlternateBase, dark ? QColor("#252c35") : QColor("#edf1f5"));
  p.setColor(QPalette::Text, text);
  p.setColor(QPalette::Button, button);
  p.setColor(QPalette::ButtonText, text);
  p.setColor(QPalette::Highlight, QColor("#1976a6"));
  p.setColor(QPalette::HighlightedText, Qt::white);
  p.setColor(QPalette::ToolTipBase, button);
  p.setColor(QPalette::ToolTipText, text);
  QApplication::setPalette(p);
}

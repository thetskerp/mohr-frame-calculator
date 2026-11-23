#include "main_window.h"
#include "theme.h"

#include <QApplication>
#include <QStyleFactory>
#include <QTimer>
#include <QPalette>
#include <QFont>
#include <QFontDatabase>
#include <QAction>
#include <QSettings>

int main(int argc, char **argv) {
  QApplication app(argc, argv);
  app.setApplicationName("Mohr Frame");
  app.setOrganizationName("Mohr Frame");
  app.setApplicationVersion("0.9.0");
  QApplication::setStyle(QStyleFactory::create("Fusion"));
  app.setFont(QFont("Arial", 10));
  QFontDatabase::addApplicationFont(":/fonts/OpenGostTypeA-Regular.ttf");
  applyTheme(QSettings().value("display/theme", 2).toInt());
  MainWindow window;
  window.show();
  QString file;
  QString screenshot;
  bool smoke = false;
  bool geometrySmoke = false;
  bool propertySmoke = false;
  for (int i = 1; i < argc; ++i) {
    const QString arg = QString::fromLocal8Bit(argv[i]);
    if (arg == "--smoke-test") smoke = true;
    else if (arg == "--smoke-geometry") geometrySmoke = true;
    else if (arg == "--smoke-property") propertySmoke = true;
    else if (arg.startsWith("--screenshot=")) screenshot = arg.mid(13);
    else file = arg;
  }
  if (!file.isEmpty())
    QTimer::singleShot(0, &window, [&window, file] { window.openPath(file); });
  if (geometrySmoke || propertySmoke) QTimer::singleShot(100, &window, [&window, propertySmoke] {
    for (auto *action : window.findChildren<QAction*>()) {
      if (action->text().startsWith(propertySmoke ? "Табличное задание EI и α"
                                                : "Табличное задание точек и стержней")) {
        action->trigger();
        break;
      }
    }
  });
  if (smoke) QTimer::singleShot(geometrySmoke || propertySmoke ? 650 : 350, &app,
                               [&window,&app,screenshot,geometrySmoke,propertySmoke] {
    auto *widget = geometrySmoke || propertySmoke ? QApplication::activeModalWidget() : &window;
    if (!screenshot.isEmpty() && widget) widget->grab().save(screenshot);
    app.quit();
  });
  return app.exec();
}

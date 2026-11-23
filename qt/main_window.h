#pragma once

#include "frame_view.h"
#include "mohr-frame/src/bridge.rs.h"

#include <QMainWindow>
#include <QPointer>
#include <QHash>
#include <memory>

class QTreeWidget;
class QDockWidget;
class QFormLayout;
class QTabWidget;
class QTextEdit;
class QCheckBox;
class QComboBox;
class QSpinBox;
class QDoubleSpinBox;
class QLineEdit;
class QThread;

class MainWindow final : public QMainWindow {
  Q_OBJECT
public:
  explicit MainWindow(QWidget *parent = nullptr);
  ~MainWindow() override;
  void openPath(const QString &path);
private:
  void setupUi();
  void refresh();
  void refreshScene();
  void refreshInspector();
  void refreshResults();
  void selectObject(bool rod, int index);
  void newProject();
  void openProject();
  bool saveProject(bool choosePath);
  void exportWord(bool fem);
  void importProject(bool xlsx);
  void geometryEditor();
  void propertyEditor();
  void diagramEditor();
  void motionEditor();
  void settingsEditor();
  void reportOptions();
  void solveFem();
  void transferFem();
  void showError(const QString &message);
  QString pathForSave();
  QVector<QPointF> points() const;
  QStringList pointNames() const;
  QVector<QPair<int, int>> edges() const;
  QStringList edgeNames() const;
  void updateTitle();
  void restoreUiSettings();
  void saveUiSettings();

  rust::Box<mohr::Document> document_;
  QTabWidget *tabs_ = nullptr;
  FrameView *views_[5] = {};
  QTreeWidget *tree_ = nullptr;
  QWidget *inspector_ = nullptr;
  QFormLayout *inspectorForm_ = nullptr;
  QTextEdit *resultText_ = nullptr;
  QDockWidget *projectDock_ = nullptr;
  QDockWidget *propertiesDock_ = nullptr;
  QDockWidget *resultsDock_ = nullptr;
  bool selectedRod_ = false;
  int selectedIndex_ = -1;
  bool reactionActual_ = true;
  bool showValues_ = true;
  int displayDecimals_ = 2;
  int themeIndex_ = 2;
  ViewColors colors_;
  bool customColors_ = false;
  int reportFlags_ = 63;
  bool femSolved_ = false;
  int femField_ = 0;
  int femCase_ = 0;
  int manualFields_[3] = {0, 3, 3};
  double lastPrepareMs_ = 0.0;
  double lastSceneMs_ = 0.0;
  QHash<int, double> diagramHeights_;
  QPointer<QThread> worker_;
};

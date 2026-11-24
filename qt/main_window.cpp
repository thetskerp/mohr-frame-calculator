#include "main_window.h"
#include "geometry_editor.h"
#include "property_editor.h"
#include "theme.h"

#include <QActionGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QColorDialog>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QElapsedTimer>
#include <QDockWidget>
#include <QDoubleSpinBox>
#include <QEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHeaderView>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMenuBar>
#include <QMessageBox>
#include <QPushButton>
#include <QRegularExpression>
#include <QRegularExpressionValidator>
#include <QSettings>
#include <QStandardPaths>
#include <QScrollArea>
#include <QScrollBar>
#include <QSpinBox>
#include <QStatusBar>
#include <QTabWidget>
#include <QTableWidget>
#include <QTextEdit>
#include <QThread>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>

static QString qs(const rust::String &s) {
  return QString::fromUtf8(s.data(), static_cast<qsizetype>(s.size()));
}
static std::string utf8(const QString &s) {
  const QByteArray bytes = s.toUtf8();
  return std::string(bytes.constData(), static_cast<size_t>(bytes.size()));
}
// Щелчок по числу выделяет всё значение: новое число можно сразу напечатать.
class EditableNumber final : public QDoubleSpinBox {
public:
  EditableNumber() { lineEdit()->installEventFilter(this); }

protected:
  bool eventFilter(QObject *watched, QEvent *event) override {
    if (watched == lineEdit() && event->type() == QEvent::MouseButtonRelease) {
      // Выделяем после штатной обработки щелчка, которая устанавливает курсор.
      QTimer::singleShot(0, this, [this] { lineEdit()->selectAll(); });
    }
    return QDoubleSpinBox::eventFilter(watched, event);
  }
};

static QDoubleSpinBox *number(double value = 0, int decimals = 6) {
  auto *w = new EditableNumber;
  w->setRange(-1e15, 1e15); w->setDecimals(decimals);
  w->setValue(value); w->setSingleStep(0.1);
  return w;
}
static QComboBox *choice(const QStringList &items, int index = 0) {
  auto *w = new QComboBox;
  w->addItems(items);
  w->setCurrentIndex(std::clamp(index, 0, static_cast<int>(items.size())-1));
  return w;
}
static QString cell(QTableWidget *table, int row, int col) {
  auto *item = table->item(row, col);
  return item ? item->text().trimmed() : QString();
}
static void put(QTableWidget *table, int row, int col, const QString &value) {
  table->setItem(row, col, new QTableWidgetItem(value));
}
static QString fmt(double value, int decimals) {
  return QString::number(value, 'f', decimals);
}
static ViewColors themeColors(int choice) {
  ViewColors colors;
  if (darkTheme(choice)) return colors;
  colors.canvas = QColor("#ffffff");
  colors.frame = QColor("#25313e");
  colors.secondary = QColor("#506070");
  colors.dimensions = QColor("#69798a");
  colors.loads = QColor("#c62828");
  colors.reactions = QColor("#007f79");
  colors.diagrams = QColor("#a84f00");
  colors.supports = QColor("#0067b2");
  colors.motions = QColor("#008a79");
  colors.temperatures = QColor("#a85600");
  return colors;
}

class FemWorker final : public QThread {
public:
  explicit FemWorker(rust::Box<mohr::Document> snapshot, QObject *parent)
      : QThread(parent), result(std::move(snapshot)) {}
  rust::Box<mohr::Document> result;
  QString error;
  double solveMs = 0;
protected:
  void run() override {
    QElapsedTimer timer;
    timer.start();
    try { mohr::document_solve_fem(*result); }
    catch (const std::exception &ex) { error = QString::fromUtf8(ex.what()); }
    solveMs = timer.nsecsElapsed() / 1.0e6;
  }
};

MainWindow::MainWindow(QWidget *parent)
  : QMainWindow(parent), document_(mohr::new_document()) {
  setupUi();
  refresh();
  restoreUiSettings();
  connect(QApplication::styleHints(), &QStyleHints::colorSchemeChanged, this, [this] {
    if (themeIndex_ != 0) return;
    applyTheme(0);
    if (!customColors_) colors_ = themeColors(0);
    for (auto *view : views_) view->setColors(colors_);
    refreshScene();
  });
}
MainWindow::~MainWindow() {
  if (worker_) worker_->wait();
  saveUiSettings();
}
void MainWindow::showError(const QString &message) {
  QMessageBox::warning(this, tr("Mohr Frame"), message);
  statusBar()->showMessage(message, 10000);
}
void MainWindow::setupUi() {
  resize(1450, 900);
  setDockOptions(AllowNestedDocks | AllowTabbedDocks | AnimatedDocks);
  auto *projectMenu = menuBar()->addMenu(tr("Файл"));
  auto addAction = [this](QMenu *menu, const QString &label, auto handler,
                          const QKeySequence &shortcut = QKeySequence()) {
    QAction *a = menu->addAction(label);
    if (!shortcut.isEmpty()) a->setShortcut(shortcut);
    connect(a, &QAction::triggered, this, handler);
    return a;
  };
  addAction(projectMenu, tr("Новый"), [this] { newProject(); }, QKeySequence::New);
  addAction(projectMenu, tr("Открыть…"), [this] { openProject(); }, QKeySequence::Open);
  addAction(projectMenu, tr("Сохранить"), [this] { saveProject(false); }, QKeySequence::Save);
  addAction(projectMenu, tr("Сохранить как…"), [this] { saveProject(true); }, QKeySequence::SaveAs);
  projectMenu->addSeparator();
  addAction(projectMenu, tr("Импорт Excel…"), [this] { importProject(true); });
  addAction(projectMenu, tr("Импорт CSV (папка)…"), [this] { importProject(false); });
  projectMenu->addSeparator();
  addAction(projectMenu, tr("Отчёт Word…"), [this] { exportWord(false); });
  addAction(projectMenu, tr("Отчёт МКЭ Word…"), [this] { exportWord(true); });
  addAction(projectMenu, tr("Экспорт для v0.5.0"), [this] {
    try { statusBar()->showMessage(qs(mohr::document_export_legacy(*document_)), 10000); }
    catch (const std::exception &e) { showError(e.what()); }
  });
  projectMenu->addSeparator();
  addAction(projectMenu, tr("Настройки…"), [this] { settingsEditor(); });
  auto *editMenu = menuBar()->addMenu(tr("Редактирование"));
  addAction(editMenu, tr("Табличное задание точек и стержней…"), [this] { geometryEditor(); });
  addAction(editMenu, tr("Табличное задание EI и α…"), [this] { propertyEditor(); });
  addAction(editMenu, tr("Эпюра выбранного стержня…"), [this] { diagramEditor(); });
  addAction(editMenu, tr("Смещения опор…"), [this] { motionEditor(); });
  auto *computeMenu = menuBar()->addMenu(tr("Расчёт"));
  addAction(computeMenu, tr("Рассчитать МКЭ"), [this] { solveFem(); });
  addAction(computeMenu, tr("Передать эпюры и реакции в метод Мора"), [this] { transferFem(); });
  addAction(computeMenu, tr("Обновить перемещения"), [this] { refreshResults(); });
  auto *viewMenu = menuBar()->addMenu(tr("Вид"));
  addAction(viewMenu, tr("Вписать схему"), [this] {
    for (auto *view : views_) view->resetView();
  });
  auto *values = viewMenu->addAction(tr("Показывать ординаты"));
  values->setCheckable(true); values->setChecked(true);
  connect(values, &QAction::toggled, this, [this](bool v) {
    showValues_ = v; for (auto *view : views_) view->setShowValues(v);
  });

  tabs_ = new QTabWidget;
  setCentralWidget(tabs_);
  const QStringList tabNames = {tr("Схема и эпюры"), tr("Силовое перемещение"),
    tr("Температурное перемещение"), tr("Кинематическое перемещение"), tr("Расчёт рамы МКЭ")};
  for (int i = 0; i < 5; ++i) {
    views_[i] = new FrameView;
    views_[i]->setShowDimensions(i == 0 || i == 1 || i == 4);
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);
    if (i < 3 || i == 4) {
      auto *controls = new QHBoxLayout;
      controls->setContentsMargins(4, 0, 4, 0);
      if (i < 3) {
        auto *label = new QLabel(tr("Эпюра на холсте:"));
        auto *diagram = choice({tr("Расчётная схема без эпюры"),
          tr("Грузовая · изгибающий момент M"),
          tr("Грузовая · продольная сила N"), tr("Грузовая · поперечная сила Q"),
          tr("Единичная · момент mₓ (перемещение X)"),
          tr("Единичная · момент mᵧ (перемещение Y)"),
          tr("Единичная · момент mφ (поворот)"),
          tr("Единичная · продольная сила nₓ (перемещение X)"),
          tr("Единичная · продольная сила nᵧ (перемещение Y)"),
          tr("Единичная · продольная сила nφ (поворот)"),}, manualFields_[i] + 1);
        controls->addWidget(label);
        controls->addWidget(diagram, 1);
        connect(diagram, &QComboBox::currentIndexChanged, this, [this, i](int index) {
          diagramHeights_[i*100 + manualFields_[i]] = views_[i]->diagramHeight();
          manualFields_[i] = index - 1;
          refreshScene();
          views_[i]->setDiagramHeight(diagramHeights_.value(i*100 + manualFields_[i], 1.0));
        });
      } else {
        controls->addWidget(new QLabel(tr("Расчётная система:")));
        auto *femCase = choice({tr("Грузовая"), tr("Единичная сила X"),
          tr("Единичная сила Y"), tr("Единичный момент φ")});
        controls->addWidget(femCase, 1);
        controls->addWidget(new QLabel(tr("Эпюра:")));
        auto *femField = choice({tr("Изгибающий момент M"),
          tr("Продольная сила N"), tr("Поперечная сила Q")});
        controls->addWidget(femField, 1);
        auto *solveButton = new QPushButton(tr("Рассчитать МКЭ"));
        controls->addWidget(solveButton);
        connect(solveButton, &QPushButton::clicked, this, [this] { solveFem(); });
        auto *transferButton = new QPushButton(tr("Передать эпюры в метод Мора"));
        controls->addWidget(transferButton);
        connect(transferButton, &QPushButton::clicked, this, [this] { transferFem(); });
        connect(femCase, &QComboBox::currentIndexChanged, this, [this](int index) {
          diagramHeights_[400 + femCase_*10 + femField_] = views_[4]->diagramHeight();
          femCase_ = index; refreshScene();
          views_[4]->setDiagramHeight(diagramHeights_.value(400 + femCase_*10 + femField_, 1.0));
        });
        connect(femField, &QComboBox::currentIndexChanged, this, [this](int index) {
          diagramHeights_[400 + femCase_*10 + femField_] = views_[4]->diagramHeight();
          femField_ = index; refreshScene();
          views_[4]->setDiagramHeight(diagramHeights_.value(400 + femCase_*10 + femField_, 1.0));
        });
      }
      layout->addLayout(controls);
    }
    layout->addWidget(views_[i], 1);
    tabs_->addTab(page, tabNames[i]);
    connect(views_[i], &FrameView::objectSelected, this, [this](bool rod, int index) {
      selectObject(rod, index);
    });
  }
  connect(tabs_, &QTabWidget::currentChanged, this, [this] { refreshResults(); });

  projectDock_ = new QDockWidget(tr("Дерево проекта"), this);
  projectDock_->setObjectName("projectDock");
  tree_ = new QTreeWidget;
  tree_->setHeaderHidden(true);
  projectDock_->setWidget(tree_);
  addDockWidget(Qt::LeftDockWidgetArea, projectDock_);
  connect(tree_, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem *item) {
    if (item->data(0, Qt::UserRole).isValid())
      selectObject(item->data(0, Qt::UserRole).toBool(),
                   item->data(0, Qt::UserRole+1).toInt());
  });
  propertiesDock_ = new QDockWidget(tr("Свойства"), this);
  propertiesDock_->setObjectName("propertiesDock");
  auto *propertiesScroll = new QScrollArea;
  propertiesScroll->setWidgetResizable(true);
  inspector_ = new QWidget;
  inspectorForm_ = new QFormLayout(inspector_);
  propertiesScroll->setWidget(inspector_);
  propertiesDock_->setWidget(propertiesScroll);
  addDockWidget(Qt::RightDockWidgetArea, propertiesDock_);
  resultsDock_ = new QDockWidget(tr("Расчёт и сообщения"), this);
  resultsDock_->setObjectName("resultsDock");
  resultText_ = new QTextEdit;
  resultText_->setReadOnly(true);
  resultText_->setLineWrapMode(QTextEdit::NoWrap);
  resultsDock_->setWidget(resultText_);
  addDockWidget(Qt::BottomDockWidgetArea, resultsDock_);
  resizeDocks({projectDock_, propertiesDock_}, {230, 300}, Qt::Horizontal);
  resizeDocks({resultsDock_}, {200}, Qt::Vertical);
  statusBar()->showMessage(tr("Готово"));
}

QVector<QPointF> MainWindow::points() const {
  QVector<QPointF> out;
  for (const auto &p : mohr::document_nodes(*document_)) out.append(QPointF(p.x, p.y));
  return out;
}
QStringList MainWindow::pointNames() const {
  QStringList out;
  for (const auto &p : mohr::document_nodes(*document_)) out << qs(p.id);
  return out;
}
QVector<QPair<int, int>> MainWindow::edges() const {
  QVector<QPair<int, int>> out;
  for (const auto &r : mohr::document_rods(*document_))
    out.append({static_cast<int>(r.start), static_cast<int>(r.end)});
  return out;
}
QStringList MainWindow::edgeNames() const {
  QStringList out;
  for (const auto &r : mohr::document_rods(*document_)) out << qs(r.id);
  return out;
}
void MainWindow::refresh() {
  tree_->clear();
  auto *pointsRoot = new QTreeWidgetItem(tree_, {tr("Точки")});
  for (const auto &node : mohr::document_nodes(*document_)) {
    auto *item = new QTreeWidgetItem(pointsRoot, {qs(node.id)});
    item->setData(0, Qt::UserRole, false);
    item->setData(0, Qt::UserRole+1, static_cast<int>(node.index));
  }
  auto *rodsRoot = new QTreeWidgetItem(tree_, {tr("Стержни")});
  for (const auto &rod : mohr::document_rods(*document_)) {
    auto *item = new QTreeWidgetItem(rodsRoot, {qs(rod.id)});
    item->setData(0, Qt::UserRole, true);
    item->setData(0, Qt::UserRole+1, static_cast<int>(rod.index));
  }
  tree_->expandAll();
  refreshScene(); refreshInspector(); refreshResults(); updateTitle();
}
void MainWindow::refreshScene() {
  QElapsedTimer timer;
  timer.start();
  const auto pts = points();
  const auto names = pointNames();
  const auto rods = edges();
  const auto rodNames = edgeNames();
  QVector<mohr::CurvePointDto> manualCurves[3], femCurve;
  for (int i = 0; i < 3; ++i)
    if (manualFields_[i] >= 0)
      for (const auto &point : mohr::document_manual_curve(*document_, manualFields_[i]))
        manualCurves[i].append(point);
  if (femSolved_)
    for (const auto &point : mohr::document_fem_curve(*document_, femCase_, femField_))
      femCurve.append(point);
  QVector<mohr::NodeDto> loadedNodes;
  QVector<mohr::RodDto> loadedRods;
  for (const auto &n : mohr::document_nodes(*document_)) loadedNodes.append(n);
  for (const auto &r : mohr::document_rods(*document_)) loadedRods.append(r);
  QVector<mohr::PointLoadDto> pointLoads;
  for (const auto &load : mohr::document_point_loads(*document_)) pointLoads.append(load);
  QVector<mohr::MotionDto> motions;
  for (const auto &motion : mohr::document_motions(*document_)) motions.append(motion);
  QVector<mohr::ReactionDto> reactions;
  if (femSolved_) for (const auto &r : mohr::document_reactions(*document_, femCase_)) reactions.append(r);
  lastPrepareMs_ = timer.nsecsElapsed() / 1.0e6;
  timer.restart();
  for (auto *view : views_) view->beginUpdate();
  for (auto *view : views_) view->setModel(pts, names, rods, rodNames);
  for (int i = 0; i < 5; ++i) {
    views_[i]->setDiagram(i == 4 ? femCurve : i == 3 ? QVector<mohr::CurvePointDto>() : manualCurves[i],
      i == 4 ? "МКЭ" : "Эпюра", colors_.diagrams, displayDecimals_);
    views_[i]->setShowValues(showValues_);
  }
  views_[0]->setLoads(loadedNodes, loadedRods);
  views_[4]->setLoads(loadedNodes, loadedRods);
  views_[2]->setTemperatures(loadedRods);
  views_[4]->setPointLoads(pointLoads);
  views_[3]->setMotions(motions);
  views_[4]->setReactions(reactions, reactionActual_, displayDecimals_);
  for (auto *view : views_) view->endUpdate();
  lastSceneMs_ = timer.nsecsElapsed() / 1.0e6;
}
void MainWindow::refreshResults() {
  const int tab = tabs_->currentIndex();
  if (tab == 4) {
    resultText_->setPlainText(qs(mohr::document_fem_details(*document_)));
    return;
  }
  const auto settings = mohr::document_settings(*document_);
  try {
    const auto lines = mohr::document_results(*document_);
    QString text;
    const int effect = tab == 2 ? 1 : tab == 3 ? 2 : 0;
    const QStringList components = {tr("Горизонтальное"), tr("Вертикальное"), tr("Поворот")};
    for (const auto &line : lines) {
      if (line.effect != effect) continue;
      text += components.value(line.component) + ": " + qs(line.value) + "\n";
      if (!qs(line.details).isEmpty()) text += qs(line.details) + "\n";
      text += "\n";
    }
    text += tr("В итоговый отчёт включены: %1%2%3")
      .arg(settings.force ? tr("силовое ") : QString())
      .arg(settings.thermal ? tr("температурное ") : QString())
      .arg(settings.kinematic ? tr("кинематическое") : QString());
    resultText_->setPlainText(text);
  } catch (const std::exception &e) {
    resultText_->setPlainText(QString::fromUtf8(e.what()));
  }
}
void MainWindow::selectObject(bool rod, int index) {
  selectedRod_ = rod; selectedIndex_ = index;
  refreshInspector();
}
void MainWindow::refreshInspector() {
  while (inspectorForm_->rowCount()) inspectorForm_->removeRow(0);
  if (selectedIndex_ < 0) {
    inspectorForm_->addRow(new QLabel(tr("Выберите точку или стержень на схеме либо в дереве.")));
    return;
  }
  try {
    if (!selectedRod_) {
      const auto nodes = mohr::document_nodes(*document_);
      if (selectedIndex_ >= static_cast<int>(nodes.size())) { selectedIndex_ = -1; return; }
      const auto &node = nodes[selectedIndex_];
      auto *id = new QLineEdit(qs(node.id));
      auto *x = number(node.x), *y = number(node.y);
      auto *support = choice({tr("Нет"), tr("Заделка"), tr("Шарнирно неподвижная"),
                              tr("Шарнирно подвижная"), tr("Ползун")}, node.support);
      auto *normal = number(node.normal_deg, 2);
      auto *fx = number(node.fx), *fy = number(node.fy), *mz = number(node.mz);
      inspectorForm_->addRow(tr("Точка"), id);
      inspectorForm_->addRow("X", x); inspectorForm_->addRow("Y", y);
      inspectorForm_->addRow(tr("Опора"), support);
      inspectorForm_->addRow(tr("Нормаль опоры, °"), normal);
      inspectorForm_->addRow("Fx", fx); inspectorForm_->addRow("Fy", fy);
      inspectorForm_->addRow("Mz", mz);
      auto *target = new QPushButton(tr("Искомая точка"));
      inspectorForm_->addRow(target);
      connect(target, &QPushButton::clicked, this, [this] {
        try { mohr::document_set_target(*document_, selectedIndex_); refresh(); }
        catch (const std::exception &e) { showError(e.what()); }
      });
      auto *apply = new QPushButton(tr("Применить"));
      inspectorForm_->addRow(apply);
      connect(apply, &QPushButton::clicked, this, [this,id,x,y,support,normal,fx,fy,mz] {
        try {
          auto draft = mohr::clone_document(*document_);
          mohr::document_set_node(*draft, selectedIndex_, utf8(id->text()), x->value(), y->value());
          mohr::document_set_fem_node(*draft, selectedIndex_, support->currentIndex(),
                                      normal->value(), fx->value(), fy->value(), mz->value());
          document_ = std::move(draft);
          femSolved_ = false;
          refresh();
        } catch (const std::exception &e) { showError(e.what()); }
      });
      auto *remove = new QPushButton(tr("Удалить точку и связанные стержни"));
      inspectorForm_->addRow(remove);
      connect(remove, &QPushButton::clicked, this, [this] {
        if (QMessageBox::question(this, tr("Удалить точку"),
              tr("Удалить точку вместе с примыкающими стержнями?")) != QMessageBox::Yes) return;
        try {
          mohr::document_remove_node(*document_, selectedIndex_);
          selectedIndex_ = -1; femSolved_ = false; refresh();
        } catch (const std::exception &e) { showError(e.what()); }
      });
    } else {
      const auto rods = mohr::document_rods(*document_);
      if (selectedIndex_ >= static_cast<int>(rods.size())) { selectedIndex_ = -1; return; }
      const auto &rod = rods[selectedIndex_];
      auto *id = new QLineEdit(qs(rod.id));
      auto *from = choice(pointNames(), static_cast<int>(rod.start));
      auto *to = choice(pointNames(), static_cast<int>(rod.end));
      auto *h = number(rod.h);
      auto *eiPreset = choice({tr("По умолчанию")}), *alphaPreset = choice({tr("По умолчанию")});
      eiPreset->setItemData(0, -1); alphaPreset->setItemData(0, -1);
      for (const auto &p : mohr::document_properties(*document_, false)) {
        eiPreset->addItem(qs(p.name), static_cast<int>(p.id));
        if (static_cast<int>(p.id) == rod.ei_preset) eiPreset->setCurrentIndex(eiPreset->count()-1);
      }
      for (const auto &p : mohr::document_properties(*document_, true)) {
        alphaPreset->addItem(qs(p.name), static_cast<int>(p.id));
        if (static_cast<int>(p.id) == rod.alpha_preset) alphaPreset->setCurrentIndex(alphaPreset->count()-1);
      }
      auto *hs = new QCheckBox(tr("В начале")), *he = new QCheckBox(tr("В конце"));
      hs->setChecked(rod.hinge_start); he->setChecked(rod.hinge_end);
      auto *qx = number(rod.qx), *qy = number(rod.qy);
      auto *ea = number(rod.ea), *ei = number(rod.ei);
      auto *temp = new QCheckBox(tr("Температурное воздействие"));
      temp->setChecked(rod.has_temperature);
      auto *tu = number(rod.t_upper), *tl = number(rod.t_lower);
      inspectorForm_->addRow(tr("Стержень"), id);
      inspectorForm_->addRow(tr("Начало"), from); inspectorForm_->addRow(tr("Конец"), to);
      inspectorForm_->addRow("h", h);
      inspectorForm_->addRow("EI", eiPreset); inspectorForm_->addRow("α", alphaPreset);
      inspectorForm_->addRow(tr("Шарнир"), hs); inspectorForm_->addRow(QString(), he);
      inspectorForm_->addRow("qx", qx); inspectorForm_->addRow("qy", qy);
      inspectorForm_->addRow(tr("EA численно, 0 = аналитически"), ea);
      inspectorForm_->addRow(tr("EI численно, 0 = аналитически"), ei);
      inspectorForm_->addRow(temp);
      inspectorForm_->addRow(tr("Верх/лево, °C"), tu);
      inspectorForm_->addRow(tr("Низ/право, °C"), tl);
      auto *apply = new QPushButton(tr("Применить"));
      inspectorForm_->addRow(apply);
      connect(apply, &QPushButton::clicked, this,
        [this,id,from,to,h,eiPreset,alphaPreset,hs,he,qx,qy,ea,ei,temp,tu,tl] {
          try {
            auto draft = mohr::clone_document(*document_);
            mohr::document_set_rod(*draft, selectedIndex_, utf8(id->text()),
              from->currentIndex(), to->currentIndex(), h->value(),
              hs->isChecked(), he->isChecked(),
              eiPreset->currentData().toInt(), alphaPreset->currentData().toInt());
            mohr::document_set_fem_rod(*draft, selectedIndex_,
                                        qx->value(), qy->value(), ea->value(), ei->value());
            mohr::document_set_temperature(*draft, selectedIndex_, temp->isChecked(),
                                            tu->value(), tl->value());
            document_ = std::move(draft);
            femSolved_ = false;
            refresh();
          } catch (const std::exception &e) { showError(e.what()); }
        });
      auto *epure = new QPushButton(tr("Редактировать эпюру…"));
      inspectorForm_->addRow(epure);
      connect(epure, &QPushButton::clicked, this, [this] { diagramEditor(); });
      auto *pointLoad = new QPushButton(tr("Добавить сосредоточенную нагрузку…"));
      inspectorForm_->addRow(pointLoad);
      connect(pointLoad, &QPushButton::clicked, this, [this] {
        QDialog d(this); d.setWindowTitle(tr("Нагрузка на стержне"));
        QFormLayout form(&d);
        auto *xi = number(0.5), *fx = number(), *fy = number(), *mz = number();
        xi->setRange(0, 1);
        form.addRow(tr("Относительное положение 0…1"), xi);
        form.addRow("Fx", fx); form.addRow("Fy", fy); form.addRow("Mz", mz);
        QDialogButtonBox buttons(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
        form.addRow(&buttons);
        connect(&buttons, &QDialogButtonBox::accepted, &d, &QDialog::accept);
        connect(&buttons, &QDialogButtonBox::rejected, &d, &QDialog::reject);
        if (d.exec() == QDialog::Accepted) try {
          mohr::document_add_point_load(*document_, selectedIndex_, xi->value(), fx->value(),
                                        fy->value(), mz->value());
          refresh();
        } catch (const std::exception &e) { showError(e.what()); }
      });
      const auto loads = mohr::document_point_loads(*document_);
      for (const auto &load : loads) {
        if (static_cast<int>(load.rod) != selectedIndex_) continue;
        auto *removeLoad = new QPushButton(tr("Удалить нагрузку x/L=%1, Fx=%2, Fy=%3, M=%4")
          .arg(load.xi).arg(load.fx).arg(load.fy).arg(load.mz));
        inspectorForm_->addRow(removeLoad);
        connect(removeLoad, &QPushButton::clicked, this, [this,load] {
          try {
            mohr::document_remove_point_load(*document_, load.rod, load.index);
            femSolved_ = false;
            refresh();
          } catch (const std::exception &e) { showError(e.what()); }
        });
      }
      auto *remove = new QPushButton(tr("Удалить стержень"));
      inspectorForm_->addRow(remove);
      connect(remove, &QPushButton::clicked, this, [this] {
        if (QMessageBox::question(this, tr("Удалить стержень"),
              tr("Удалить выбранный стержень?")) != QMessageBox::Yes) return;
        try {
          mohr::document_remove_rod(*document_, selectedIndex_);
          selectedIndex_ = -1; femSolved_ = false; refresh();
        } catch (const std::exception &e) { showError(e.what()); }
      });
    }
  } catch (const std::exception &e) { showError(e.what()); }
}
void MainWindow::newProject() {
  if (worker_) return;
  document_ = mohr::new_document(); selectedIndex_ = -1; femSolved_ = false;
  for (auto *view : views_) view->resetView();
  refresh();
}
void MainWindow::openProject() {
  const QString path = QFileDialog::getOpenFileName(this, tr("Открыть проект"), {},
                                                     tr("Проект Mohr (*.mohr.ron)"));
  if (path.isEmpty()) return;
  openPath(path);
}
void MainWindow::openPath(const QString &path) {
  try {
    auto loaded = mohr::load_document(utf8(path));
    document_ = std::move(loaded);
    femSolved_ = false; selectedIndex_ = -1;
    refresh();
    for (auto *view : views_) view->resetView();
  } catch (const std::exception &e) { showError(e.what()); }
}
QString MainWindow::pathForSave() {
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Сохранить схему"));
  QFormLayout form(&dialog);
  auto *name = new QLineEdit(tr("Схема"));
  name->setValidator(new QRegularExpressionValidator(
    QRegularExpression("[\\p{L}\\p{N}_ -]+"), name));
  auto *folder = new QLineEdit(
    QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation));
  auto *choose = new QPushButton(tr("Выбрать папку…"));
  form.addRow(tr("Название"), name);
  form.addRow(tr("Сохранить в"), folder);
  form.addRow(QString(), choose);
  form.addRow(new QLabel(tr("Папка проекта будет создана автоматически. Файл: <название>.mohr.ron")));
  connect(choose, &QPushButton::clicked, &dialog, [this,folder] {
    const QString selected = QFileDialog::getExistingDirectory(this, tr("Расположение проектов"),
      folder->text());
    if (!selected.isEmpty()) folder->setText(selected);
  });
  auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  form.addRow(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (dialog.exec() != QDialog::Accepted || name->text().trimmed().isEmpty()) return {};
  const QString projectName = name->text().trimmed();
  return QDir(folder->text()).filePath(projectName + "/" + projectName + ".mohr.ron");
}
bool MainWindow::saveProject(bool choosePath) {
  try {
    const QString existing = qs(mohr::document_path(*document_));
    const QString path = choosePath || existing.isEmpty() ? pathForSave() : existing;
    if (path.isEmpty()) return false;
    const QString saved = qs(mohr::document_save(*document_, utf8(path)));
    statusBar()->showMessage(tr("Сохранено: ") + saved, 10000);
    updateTitle();
    return true;
  } catch (const std::exception &e) { showError(e.what()); return false; }
}
void MainWindow::exportWord(bool fem) {
  if (qs(mohr::document_path(*document_)).isEmpty() && !saveProject(false)) return;
  try {
    const QString path = fem ? qs(mohr::document_export_fem_word(*document_, reportFlags_))
                             : qs(mohr::document_export_word(*document_));
    statusBar()->showMessage(tr("Отчёт сохранён: ") + path, 15000);
  } catch (const std::exception &e) { showError(e.what()); }
}
void MainWindow::updateTitle() {
  const QString path = qs(mohr::document_path(*document_));
  setWindowTitle((path.isEmpty() ? tr("Новая схема") : QFileInfo(path).fileName()) +
                 " — Mohr Frame 0.9.1");
}
void MainWindow::restoreUiSettings() {
  QSettings s;
  restoreGeometry(s.value("window/geometry").toByteArray());
  restoreState(s.value("window/state").toByteArray());
  displayDecimals_ = s.value("display/decimals", 2).toInt();
  reactionActual_ = s.value("display/reactionActual", true).toBool();
  reportFlags_ = s.value("report/femSections", 63).toInt();
  showValues_ = s.value("display/ordinates", true).toBool();
  themeIndex_ = s.value("display/theme", 2).toInt();
  colors_ = themeColors(themeIndex_);
  customColors_ = s.value("colors/customized", false).toBool();
  auto readColor = [&s](const QString &key, QColor &target) {
    const QColor stored(s.value("colors/" + key).toString());
    if (stored.isValid()) target = stored;
  };
  if (customColors_) {
    readColor("frame", colors_.frame);
    readColor("loads", colors_.loads);
    readColor("reactions", colors_.reactions);
    readColor("diagrams", colors_.diagrams);
    readColor("supports", colors_.supports);
    readColor("motions", colors_.motions);
    readColor("temperatures", colors_.temperatures);
  }
  applyTheme(themeIndex_);
  const QVariantMap heights = s.value("canvas/heights").toMap();
  for (auto it = heights.begin(); it != heights.end(); ++it)
    diagramHeights_.insert(it.key().toInt(), it.value().toDouble());
  for (int i = 0; i < 5; ++i) {
    views_[i]->setZoom(s.value(QString("canvas/%1/zoom").arg(i), 1.0).toDouble());
    const int key = i == 4 ? 400 + femCase_*10 + femField_ : i*100 + (i < 3 ? manualFields_[i] : 0);
    views_[i]->setDiagramHeight(diagramHeights_.value(key, 1.0));
    views_[i]->horizontalScrollBar()->setValue(s.value(QString("canvas/%1/x").arg(i), 0).toInt());
    views_[i]->verticalScrollBar()->setValue(s.value(QString("canvas/%1/y").arg(i), 0).toInt());
    views_[i]->setShowValues(showValues_);
    views_[i]->setColors(colors_);
  }
}
void MainWindow::saveUiSettings() {
  QSettings s;
  s.setValue("window/geometry", saveGeometry());
  s.setValue("window/state", saveState());
  s.setValue("display/decimals", displayDecimals_);
  s.setValue("display/reactionActual", reactionActual_);
  s.setValue("report/femSections", reportFlags_);
  s.setValue("display/ordinates", showValues_);
  s.setValue("display/theme", themeIndex_);
  if (!customColors_) s.remove("colors");
  else {
    s.setValue("colors/customized", true);
    s.setValue("colors/frame", colors_.frame.name());
    s.setValue("colors/loads", colors_.loads.name());
    s.setValue("colors/reactions", colors_.reactions.name());
    s.setValue("colors/diagrams", colors_.diagrams.name());
    s.setValue("colors/supports", colors_.supports.name());
    s.setValue("colors/motions", colors_.motions.name());
    s.setValue("colors/temperatures", colors_.temperatures.name());
  }
  QVariantMap heights;
  for (auto it = diagramHeights_.begin(); it != diagramHeights_.end(); ++it)
    heights.insert(QString::number(it.key()), it.value());
  for (int i = 0; i < 5; ++i) {
    const int key = i == 4 ? 400 + femCase_*10 + femField_ : i*100 + (i < 3 ? manualFields_[i] : 0);
    heights.insert(QString::number(key), views_[i]->diagramHeight());
    s.setValue(QString("canvas/%1/zoom").arg(i), views_[i]->zoom());
    s.setValue(QString("canvas/%1/x").arg(i), views_[i]->horizontalScrollBar()->value());
    s.setValue(QString("canvas/%1/y").arg(i), views_[i]->verticalScrollBar()->value());
  }
  s.setValue("canvas/heights", heights);
}

void MainWindow::geometryEditor() {
  if (worker_) return;
  GeometryEditor dialog(*document_, this);
  if (dialog.exec() != QDialog::Accepted) return;
  selectedIndex_ = -1;
  femSolved_ = false;
  refresh();
  for (auto *view : views_) view->resetView();
}

void MainWindow::propertyEditor() {
  if (worker_) return;
  PropertyEditor dialog(*document_, this);
  if (dialog.exec() != QDialog::Accepted) return;
  femSolved_ = false;
  refresh();
}

void MainWindow::diagramEditor() {
  if (selectedIndex_ < 0 || !selectedRod_) {
    showError(tr("Сначала выберите стержень")); return;
  }
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Эпюра на стержне"));
  QFormLayout form(&dialog);
  auto *field = choice({"M грузовая", "N грузовая", "Q грузовая",
                        "mₓ", "mᵧ", "mφ", "nₓ", "nᵧ", "nφ"});
  auto *kind = choice({tr("Константа"), tr("Линейная"), tr("Парабола")});
  auto *left = number(), *right = number(), *q = number();
  auto *extremum = new QCheckBox(tr("Экстремум внутри стержня"));
  auto load = [this,field,kind,left,right,q,extremum] {
    try {
      const auto d = mohr::document_diagram(*document_, selectedIndex_, field->currentIndex());
      kind->setCurrentIndex(d.kind); left->setValue(d.left); right->setValue(d.right);
      q->setValue(d.q_signed); extremum->setChecked(d.extremum);
    } catch (const std::exception &e) { showError(e.what()); }
  };
  connect(field, &QComboBox::currentIndexChanged, &dialog, [load] { load(); });
  form.addRow(tr("Эпюра"), field); form.addRow(tr("Тип"), kind);
  form.addRow(tr("Начало со знаком"), left); form.addRow(tr("Конец со знаком"), right);
  form.addRow(tr("q с учётом выпуклости"), q); form.addRow(extremum);
  form.addRow(new QLabel(tr("Плюс: нижние/правые волокна. Минус: верхние/левые.\n"
                            "Для параболы проверяйте прогиб эпюры по правилу дождика.")));
  auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  form.addRow(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  load();
  if (dialog.exec() != QDialog::Accepted) return;
  try {
    mohr::document_set_diagram(*document_, selectedIndex_, field->currentIndex(),
      kind->currentIndex(), left->value(), right->value(), q->value(), extremum->isChecked());
    refresh();
  } catch (const std::exception &e) { showError(e.what()); }
}

void MainWindow::motionEditor() {
  QDialog dialog(this); dialog.setWindowTitle(tr("Заданные перемещения опор"));
  QFormLayout form(&dialog);
  auto *node = choice(pointNames());
  auto *direction = choice({"X", "Y", "φ", tr("Нормаль")});
  auto *normal = number(90), *displacement = number();
  auto *rx = number(), *ry = number(), *rphi = number();
  form.addRow(tr("Точка"), node); form.addRow(tr("Направление"), direction);
  form.addRow(tr("Угол нормали"), normal);
  form.addRow(tr("Заданное смещение c"), displacement);
  form.addRow(tr("R для единичной X"), rx);
  form.addRow(tr("R для единичной Y"), ry);
  form.addRow(tr("R для единичного φ"), rphi);
  auto *existing = new QComboBox;
  existing->addItem(tr("Новая связь"), -1);
  const auto motions = mohr::document_motions(*document_);
  for (int i = 0; i < static_cast<int>(motions.size()); ++i)
    existing->addItem(QString("%1 / %2").arg(pointNames().value(motions[i].node))
                      .arg(motions[i].direction), i);
  form.addRow(tr("Существующие"), existing);
  auto *removeMotion = new QPushButton(tr("Удалить выбранную связь"));
  form.addRow(removeMotion);
  connect(removeMotion, &QPushButton::clicked, &dialog, [this,existing,motions,&dialog] {
    const int i = existing->currentData().toInt();
    if (i < 0 || i >= static_cast<int>(motions.size())) return;
    try {
      mohr::document_remove_support_motion(*document_, motions[i].node, motions[i].direction);
      femSolved_ = false;
      refresh();
      dialog.reject();
    } catch (const std::exception &e) { showError(e.what()); }
  });
  connect(existing, &QComboBox::currentIndexChanged, &dialog, [=](int index) {
    const int i = existing->itemData(index).toInt();
    if (i < 0 || i >= static_cast<int>(motions.size())) return;
    const auto &m = motions[i];
    node->setCurrentIndex(m.node); direction->setCurrentIndex(m.direction);
    normal->setValue(m.normal_deg); displacement->setValue(m.displacement);
    rx->setValue(m.rx); ry->setValue(m.ry); rphi->setValue(m.rphi);
  });
  auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  form.addRow(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (dialog.exec() != QDialog::Accepted) return;
  try {
    auto draft = mohr::clone_document(*document_);
    const int oldIndex = existing->currentData().toInt();
    if (oldIndex >= 0 && oldIndex < static_cast<int>(motions.size()))
      mohr::document_remove_support_motion(*draft, motions[oldIndex].node,
                                           motions[oldIndex].direction);
    mohr::document_set_support_motion(*draft, node->currentIndex(),
      direction->currentIndex(), normal->value(), displacement->value(),
      rx->value(), ry->value(), rphi->value());
    document_ = std::move(draft);
    femSolved_ = false;
    refresh();
  } catch (const std::exception &e) { showError(e.what()); }
}

void MainWindow::settingsEditor() {
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Настройки"));
  dialog.resize(590, 510);
  QVBoxLayout layout(&dialog);
  auto *sections = new QTabWidget;
  layout.addWidget(sections);
  const auto current = mohr::document_settings(*document_);

  auto *appearance = new QWidget;
  QFormLayout appearanceForm(appearance);
  auto *theme = choice({tr("Системная"), tr("Светлая"), tr("Тёмная")}, themeIndex_);
  appearanceForm.addRow(tr("Тема"), theme);
  ViewColors chosenColors = colors_;
  bool chosenCustom = customColors_;
  QVector<QPair<QPushButton*, QColor*>> pickers;
  auto updatePicker = [](QPushButton *button, const QColor &color) {
    button->setText(color.name().toUpper());
    button->setStyleSheet(QString("background:%1; color:%2;")
      .arg(color.name(), color.lightness() < 130 ? "white" : "black"));
  };
  auto addColor = [&](const QString &name, QColor &value) {
    auto *button = new QPushButton;
    QColor *target = &value;
    updatePicker(button, value);
    appearanceForm.addRow(name, button);
    pickers.append({button, target});
    connect(button, &QPushButton::clicked, &dialog, [&, button, target] {
      const QColor selected = QColorDialog::getColor(*target, &dialog, tr("Выбор цвета"));
      if (selected.isValid()) {
        *target = selected;
        chosenCustom = true;
        updatePicker(button, selected);
      }
    });
  };
  addColor(tr("Стержни и точки"), chosenColors.frame);
  addColor(tr("Заданные силы и нагрузки"), chosenColors.loads);
  addColor(tr("Реакции опор"), chosenColors.reactions);
  addColor(tr("Эпюры и ординаты"), chosenColors.diagrams);
  addColor(tr("Опоры"), chosenColors.supports);
  addColor(tr("Смещения опор"), chosenColors.motions);
  addColor(tr("Температуры"), chosenColors.temperatures);
  connect(theme, &QComboBox::currentIndexChanged, &dialog, [&](int index) {
    chosenColors = themeColors(index);
    chosenCustom = false;
    for (const auto &picker : pickers) updatePicker(picker.first, *picker.second);
  });
  sections->addTab(appearance, tr("Оформление"));

  auto *calculation = new QWidget;
  QFormLayout calculationForm(calculation);
  auto *decimals = new QSpinBox;
  decimals->setRange(0, 12); decimals->setValue(displayDecimals_);
  auto *direction = choice({tr("По глобальным осям"), tr("Фактическая")},
                           reactionActual_ ? 1 : 0);
  auto *rounding = new QCheckBox(tr("Влияние округления на весь расчёт"));
  rounding->setChecked(current.rounding);
  calculationForm.addRow(tr("Знаков после запятой"), decimals);
  calculationForm.addRow(tr("Направление реакций"), direction);
  calculationForm.addRow(rounding);
  sections->addTab(calculation, tr("Расчёт"));

  auto *fem = new QWidget;
  QFormLayout femForm(fem);
  femForm.addRow(new QLabel(tr("Единичные системы для искомой точки:")));
  auto *unitX = new QCheckBox("X"), *unitY = new QCheckBox("Y"), *unitPhi = new QCheckBox("φ");
  unitX->setChecked(current.unit_x); unitY->setChecked(current.unit_y);
  unitPhi->setChecked(current.unit_phi);
  femForm.addRow(unitX); femForm.addRow(unitY); femForm.addRow(unitPhi);
  sections->addTab(fem, tr("МКЭ"));

  auto *report = new QWidget;
  QFormLayout reportForm(report);
  reportForm.addRow(new QLabel(tr("Включить в итоговый отчёт по перемещениям:")));
  auto *force = new QCheckBox(tr("Силовое")), *thermal = new QCheckBox(tr("Температурное")),
       *kinematic = new QCheckBox(tr("Кинематическое"));
  force->setChecked(current.force); thermal->setChecked(current.thermal);
  kinematic->setChecked(current.kinematic);
  reportForm.addRow(force); reportForm.addRow(thermal); reportForm.addRow(kinematic);
  reportForm.addRow(new QLabel(tr("Разделы отчёта МКЭ:")));
  const QStringList names = {tr("Геометрия"), tr("Нагрузки"), tr("Устойчивость"),
    tr("Реакции"), tr("Грузовые эпюры"), tr("Единичные эпюры")};
  QVector<QCheckBox*> reportChecks;
  for (int i = 0; i < names.size(); ++i) {
    auto *box = new QCheckBox(names[i]);
    box->setChecked(reportFlags_ & (1 << i));
    reportForm.addRow(box);
    reportChecks.append(box);
  }
  sections->addTab(report, tr("Отчёт"));

  auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  layout.addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (dialog.exec() != QDialog::Accepted) return;
  themeIndex_ = theme->currentIndex();
  colors_ = chosenColors;
  customColors_ = chosenCustom;
  applyTheme(themeIndex_);
  for (auto *view : views_) view->setColors(colors_);
  displayDecimals_ = decimals->value();
  reactionActual_ = direction->currentIndex() == 1;
  mohr::document_set_effects(*document_, force->isChecked(), thermal->isChecked(),
                              kinematic->isChecked());
  mohr::document_set_rounding(*document_, rounding->isChecked(), displayDecimals_);
  mohr::document_set_unit_selection(*document_, unitX->isChecked(), unitY->isChecked(),
                                     unitPhi->isChecked());
  reportFlags_ = 0;
  for (int i = 0; i < reportChecks.size(); ++i)
    if (reportChecks[i]->isChecked()) reportFlags_ |= (1 << i);
  femSolved_ = false;
  refresh();
  saveUiSettings();
}
void MainWindow::reportOptions() {
  QDialog dialog(this); dialog.setWindowTitle(tr("Разделы отчёта МКЭ"));
  QVBoxLayout layout(&dialog);
  const QStringList names = {tr("Геометрия"), tr("Нагрузки"), tr("Устойчивость"),
    tr("Реакции"), tr("Грузовые эпюры"), tr("Единичные эпюры")};
  QVector<QCheckBox*> checks;
  for (int i = 0; i < names.size(); ++i) {
    auto *box = new QCheckBox(names[i]); box->setChecked(reportFlags_ & (1 << i));
    layout.addWidget(box); checks.append(box);
  }
  QDialogButtonBox buttons(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  layout.addWidget(&buttons);
  connect(&buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(&buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (dialog.exec() == QDialog::Accepted) {
    reportFlags_ = 0;
    for (int i = 0; i < checks.size(); ++i) if (checks[i]->isChecked()) reportFlags_ |= 1 << i;
  }
}
void MainWindow::importProject(bool xlsx) {
  const QString path = xlsx
    ? QFileDialog::getOpenFileName(this, tr("Импорт Excel"), {}, tr("Excel (*.xlsx)"))
    : QFileDialog::getExistingDirectory(this, tr("Папка согласованных CSV"));
  if (path.isEmpty()) return;
  try {
    auto draft = xlsx ? mohr::import_xlsx_draft(utf8(path))
                      : mohr::import_csv_draft(utf8(path));
    QDialog preview(this);
    preview.setWindowTitle(tr("Предварительный просмотр импорта"));
    preview.resize(760, 520);
    QVBoxLayout layout(&preview);
    layout.addWidget(new QLabel(qs(mohr::import_draft_summary(*draft))));
    auto *tables = new QTabWidget;
    auto *table = new QTableWidget;
    table->setColumnCount(3); table->setHorizontalHeaderLabels({tr("Точка"), "X", "Y"});
    const auto points = mohr::import_draft_nodes(*draft);
    table->setRowCount(static_cast<int>(points.size()));
    for (int row = 0; row < static_cast<int>(points.size()); ++row) {
      put(table,row,0,qs(points[row].id)); put(table,row,1,QString::number(points[row].x));
      put(table,row,2,QString::number(points[row].y));
    }
    table->setEditTriggers(QAbstractItemView::NoEditTriggers);
    table->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    tables->addTab(table, tr("Точки"));
    auto *rodTable = new QTableWidget;
    rodTable->setColumnCount(4);
    rodTable->setHorizontalHeaderLabels({tr("Стержень"), tr("Начало"), tr("Конец"), "h"});
    const auto importedRods = mohr::import_draft_rods(*draft);
    rodTable->setRowCount(static_cast<int>(importedRods.size()));
    for (int row = 0; row < static_cast<int>(importedRods.size()); ++row) {
      const auto &rod = importedRods[row];
      put(rodTable,row,0,qs(rod.id));
      put(rodTable,row,1,qs(points.at(rod.start).id));
      put(rodTable,row,2,qs(points.at(rod.end).id));
      put(rodTable,row,3,QString::number(rod.h));
    }
    rodTable->setEditTriggers(QAbstractItemView::NoEditTriggers);
    rodTable->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    tables->addTab(rodTable, tr("Стержни"));
    layout.addWidget(tables);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    layout.addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &preview, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &preview, &QDialog::reject);
    if (preview.exec() == QDialog::Accepted) {
      mohr::document_apply_import(*document_, std::move(draft));
      femSolved_ = false; selectedIndex_ = -1;
      refresh();
      for (auto *view : views_) view->resetView();
    }
  } catch (const std::exception &e) {
    QDialog errors(this);
    errors.setWindowTitle(tr("Ошибки импорта"));
    errors.resize(760, 440);
    QVBoxLayout layout(&errors);
    layout.addWidget(new QLabel(tr("Проверьте указанные таблицы, строки и столбцы. Текущий проект не изменён.")));
    auto *details = new QTextEdit;
    details->setReadOnly(true);
    details->setPlainText(QString::fromUtf8(e.what()));
    layout.addWidget(details);
    auto *close = new QDialogButtonBox(QDialogButtonBox::Close);
    layout.addWidget(close);
    connect(close, &QDialogButtonBox::rejected, &errors, &QDialog::reject);
    errors.exec();
  }
}
void MainWindow::solveFem() {
  if (worker_) return;
  auto snapshot = mohr::clone_document(*document_);
  const auto revision = mohr::document_revision(*document_);
  auto *worker = new FemWorker(std::move(snapshot), this);
  worker_ = worker;
  statusBar()->showMessage(tr("МКЭ: расчёт выполняется…"));
  connect(worker, &QThread::finished, this, [this,worker,revision] {
    if (mohr::document_revision(*document_) == revision) {
      if (worker->error.isEmpty()) {
        document_ = std::move(worker->result);
        femSolved_ = true;
        refresh();
        statusBar()->showMessage(tr("МКЭ: %1 мс; данные сцены: %2 мс; отрисовка: %3 мс")
          .arg(worker->solveMs, 0, 'f', 1).arg(lastPrepareMs_, 0, 'f', 1)
          .arg(lastSceneMs_, 0, 'f', 1), 15000);
      } else showError(worker->error);
    } else statusBar()->showMessage(tr("Результат старой редакции отброшен"), 10000);
    worker_ = nullptr; worker->deleteLater();
  });
  worker->start();
}
void MainWindow::transferFem() {
  if (!femSolved_) { showError(tr("Сначала выполните расчёт МКЭ")); return; }
  try {
    mohr::document_transfer_fem(*document_);
    femSolved_ = false;
    refresh();
    tabs_->setCurrentIndex(1);
  } catch (const std::exception &e) { showError(e.what()); }
}


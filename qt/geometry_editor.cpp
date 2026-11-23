#include "geometry_editor.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QHeaderView>
#include <QHBoxLayout>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QSplitter>
#include <QTableWidget>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>

namespace {
QString text(const rust::String &value) {
  return QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size()));
}
std::string utf8(const QString &value) {
  const QByteArray bytes = value.toUtf8();
  return std::string(bytes.constData(), static_cast<size_t>(bytes.size()));
}
QString cell(QTableWidget *table, int row, int column) {
  const auto *item = table->item(row, column);
  return item ? item->text().trimmed() : QString();
}
void put(QTableWidget *table, int row, int column, const QString &value) {
  table->setItem(row, column, new QTableWidgetItem(value));
}
QString excelName(int index) {
  QString out;
  do {
    out.prepend(QChar('A' + index % 26));
    index = index / 26 - 1;
  } while (index >= 0);
  return out;
}
bool parseNumber(const QString &value, double &number) {
  bool valid = false;
  number = QString(value).replace(',', '.').toDouble(&valid);
  return valid && std::isfinite(number);
}
void checkCell(QTableWidget *table, int row, int column, bool checked) {
  auto *box = new QCheckBox;
  box->setChecked(checked);
  box->setText(checked ? QObject::tr("Да") : QObject::tr("Нет"));
  box->setStyleSheet("margin-left: 8px;");
  QObject::connect(box, &QCheckBox::toggled, box, [box](bool active) {
    box->setText(active ? QObject::tr("Да") : QObject::tr("Нет"));
  });
  table->setCellWidget(row, column, box);
}
}

GeometryEditor::GeometryEditor(mohr::Document &document, QWidget *parent)
    : QDialog(parent), document_(document) {
  setWindowTitle(tr("Табличное задание точек и стержней"));
  resize(1380, 760);
  auto *outer = new QVBoxLayout(this);
  outer->addWidget(new QLabel(tr("Задайте количество точек и стержней, затем измените их параметры. Предварительный вид обновляется сразу.")));
  auto *splitter = new QSplitter(Qt::Horizontal);
  outer->addWidget(splitter, 1);
  auto *scroll = new QScrollArea;
  scroll->setWidgetResizable(true);
  splitter->addWidget(scroll);
  auto *editor = new QWidget;
  auto *layout = new QVBoxLayout(editor);
  scroll->setWidget(editor);

  auto *pointGroup = new QGroupBox(tr("1. Точки"));
  auto *pointLayout = new QVBoxLayout(pointGroup);
  auto *pointRow = new QHBoxLayout;
  pointRow->addWidget(new QLabel(tr("Количество точек:")));
  pointCount_ = new QSpinBox;
  pointCount_->setRange(0, 200);
  pointRow->addWidget(pointCount_);
  pointRow->addStretch();
  pointLayout->addLayout(pointRow);
  points_ = new QTableWidget;
  points_->setColumnCount(3);
  points_->setHorizontalHeaderLabels({tr("Точка"), "X", "Y"});
  points_->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
  points_->setMinimumHeight(230);
  pointLayout->addWidget(points_);
  layout->addWidget(pointGroup);

  auto *rodGroup = new QGroupBox(tr("2. Стержни"));
  auto *rodLayout = new QVBoxLayout(rodGroup);
  auto *rodRow = new QHBoxLayout;
  rodRow->addWidget(new QLabel(tr("Количество стержней:")));
  rodCount_ = new QSpinBox;
  rodCount_->setRange(0, 300);
  rodRow->addWidget(rodCount_);
  rodRow->addStretch();
  rodLayout->addLayout(rodRow);
  rods_ = new QTableWidget;
  rods_->setColumnCount(6);
  rods_->setHorizontalHeaderLabels({tr("Стержень"), tr("Начало"), tr("Конец"),
    "h", tr("Шарнир в начале"), tr("Шарнир в конце")});
  rods_->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
  rods_->setMinimumHeight(250);
  rodLayout->addWidget(rods_);
  layout->addWidget(rodGroup);
  layout->addWidget(new QLabel(tr("Имена точек A…Z, AA… создаются автоматически. Имена стержней образуются по их концам; их можно изменить вручную.")));
  layout->addStretch();

  auto *previewBox = new QGroupBox(tr("Предварительный вид рамы"));
  auto *previewLayout = new QVBoxLayout(previewBox);
  preview_ = new FrameView;
  preview_->setShowDimensions(true);
  previewLayout->addWidget(preview_);
  splitter->addWidget(previewBox);
  splitter->setSizes({790, 570});

  message_ = new QLabel;
  message_->setStyleSheet("color: #ff7777;");
  outer->addWidget(message_);
  auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(tr("Применить"));
  outer->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, this, &GeometryEditor::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &GeometryEditor::reject);

  updating_ = true;
  const auto nodes = mohr::document_nodes(document_);
  const auto rods = mohr::document_rods(document_);
  originalPointCount_ = static_cast<int>(nodes.size());
  originalRodCount_ = static_cast<int>(rods.size());
  pointCount_->setValue(originalPointCount_);
  points_->setRowCount(originalPointCount_);
  for (int row = 0; row < originalPointCount_; ++row) {
    put(points_, row, 0, text(nodes[row].id));
    put(points_, row, 1, QString::number(nodes[row].x, 'g', 15));
    put(points_, row, 2, QString::number(nodes[row].y, 'g', 15));
  }
  rodCount_->setValue(originalRodCount_);
  rods_->setRowCount(originalRodCount_);
  for (int row = 0; row < originalRodCount_; ++row) {
    const auto &rod = rods[row];
    put(rods_, row, 0, text(rod.id));
    put(rods_, row, 3, QString::number(rod.h, 'g', 15));
    checkCell(rods_, row, 4, rod.hinge_start);
    checkCell(rods_, row, 5, rod.hinge_end);
    const QString expected = (pointName(rod.start).size() == 1 && pointName(rod.end).size() == 1)
      ? pointName(rod.start) + pointName(rod.end)
      : pointName(rod.start) + "-" + pointName(rod.end);
    generatedNames_.append(text(rod.id) == expected);
  }
  updateRodSelectors();
  for (int row = 0; row < originalRodCount_; ++row) {
    endpoint(row, 1)->setCurrentIndex(rods[row].start);
    endpoint(row, 2)->setCurrentIndex(rods[row].end);
    updateGeneratedName(row);
  }
  updating_ = false;
  for (int row = 0; row < originalRodCount_; ++row)
    for (int column : {4, 5})
      connect(qobject_cast<QCheckBox*>(rods_->cellWidget(row,column)), &QCheckBox::toggled,
              this, [this] { refreshPreview(); });
  connect(pointCount_, &QSpinBox::valueChanged, this, [this](int count) { resizePoints(count); });
  connect(rodCount_, &QSpinBox::valueChanged, this, [this](int count) { resizeRods(count); });
  connect(points_, &QTableWidget::itemChanged, this, [this](QTableWidgetItem *item) {
    if (updating_) return;
    if (item->column() == 0) updateRodSelectors();
    refreshPreview();
  });
  connect(rods_, &QTableWidget::itemChanged, this, [this](QTableWidgetItem *item) {
    if (updating_) return;
    if (item->column() == 0 && item->row() < generatedNames_.size())
      generatedNames_[item->row()] = false;
    refreshPreview();
  });
  refreshPreview();
}

QString GeometryEditor::pointName(int row) const {
  return row >= 0 && row < points_->rowCount() ? cell(points_, row, 0) : QString();
}
QComboBox *GeometryEditor::endpoint(int row, int column) const {
  return qobject_cast<QComboBox*>(rods_->cellWidget(row, column));
}
QString GeometryEditor::generatedRodName(int row) const {
  const auto *start = endpoint(row, 1);
  const auto *end = endpoint(row, 2);
  if (!start || !end || start->currentIndex() < 0 || end->currentIndex() < 0) return {};
  const QString a = pointName(start->currentIndex()), b = pointName(end->currentIndex());
  if (a.isEmpty() || b.isEmpty()) return {};
  return a.size() == 1 && b.size() == 1 ? a + b : a + "-" + b;
}
void GeometryEditor::updateGeneratedName(int row) {
  if (row >= generatedNames_.size() || !generatedNames_[row]) return;
  const QString name = generatedRodName(row);
  if (name.isEmpty()) return;
  const QSignalBlocker block(rods_);
  put(rods_, row, 0, name);
}
void GeometryEditor::updateRodSelectors() {
  const bool wasUpdating = updating_;
  updating_ = true;
  for (int row = 0; row < rods_->rowCount(); ++row) {
    for (int column : {1, 2}) {
      auto *combo = endpoint(row, column);
      const int selected = combo ? combo->currentIndex() : std::min(column-1, points_->rowCount()-1);
      if (!combo) {
        combo = new QComboBox;
        rods_->setCellWidget(row, column, combo);
        connect(combo, &QComboBox::currentIndexChanged, this, [this,row](int) {
          if (updating_) return;
          updateGeneratedName(row);
          refreshPreview();
        });
      }
      const QSignalBlocker block(combo);
      combo->clear();
      for (int i = 0; i < points_->rowCount(); ++i) combo->addItem(pointName(i));
      combo->setCurrentIndex(std::clamp(selected, -1, combo->count()-1));
    }
    updateGeneratedName(row);
  }
  updating_ = wasUpdating;
}
void GeometryEditor::resizePoints(int count) {
  if (updating_) return;
  const int old = points_->rowCount();
  if (count < old) {
    bool used = false;
    for (int row = 0; row < rods_->rowCount(); ++row)
      used |= endpoint(row,1)->currentIndex() >= count || endpoint(row,2)->currentIndex() >= count;
    const auto nodes = mohr::document_nodes(document_);
    for (int row = count; row < std::min(old, static_cast<int>(nodes.size())); ++row)
      used |= nodes[row].support != 0 || nodes[row].fx != 0 || nodes[row].fy != 0 || nodes[row].mz != 0;
    if (used) {
      message_->setText(tr("Сначала уберите стержни, опоры и нагрузки с удаляемых точек."));
      const QSignalBlocker block(pointCount_);
      pointCount_->setValue(old);
      return;
    }
  }
  updating_ = true;
  double maxX = 0;
  for (int row = 0; row < old; ++row) {
    double x = 0;
    if (parseNumber(cell(points_,row,1),x)) maxX = std::max(maxX,x);
  }
  points_->setRowCount(count);
  for (int row = old; row < count; ++row) {
    QString id;
    for (int candidate = 0;; ++candidate) {
      id = excelName(candidate);
      bool taken = false;
      for (int prior = 0; prior < row; ++prior)
        taken |= pointName(prior).compare(id, Qt::CaseInsensitive) == 0;
      if (!taken) break;
    }
    maxX += 2;
    put(points_,row,0,id);
    put(points_,row,1,QString::number(maxX));
    put(points_,row,2,"0");
  }
  updating_ = false;
  updateRodSelectors();
  message_->clear();
  refreshPreview();
}
void GeometryEditor::resizeRods(int count) {
  if (updating_) return;
  const int old = rods_->rowCount();
  if (count > old && points_->rowCount() < 2) {
    message_->setText(tr("Для стержня нужны хотя бы две точки."));
    const QSignalBlocker block(rodCount_);
    rodCount_->setValue(old);
    return;
  }
  updating_ = true;
  rods_->setRowCount(count);
  generatedNames_.resize(count);
  for (int row = old; row < count; ++row) {
    generatedNames_[row] = true;
    put(rods_,row,3,"0.5");
    checkCell(rods_,row,4,false);
    checkCell(rods_,row,5,false);
    for (int column : {4, 5})
      connect(qobject_cast<QCheckBox*>(rods_->cellWidget(row,column)), &QCheckBox::toggled,
              this, [this] { refreshPreview(); });
  }
  updateRodSelectors();
  for (int row = old; row < count; ++row) {
    int selectedStart = -1, selectedEnd = -1;
    for (int a = 0; a < points_->rowCount() && selectedStart < 0; ++a)
      for (int b = a+1; b < points_->rowCount(); ++b) {
        bool used = false;
        for (int previous = 0; previous < row; ++previous) {
          const int p = endpoint(previous,1)->currentIndex(), q = endpoint(previous,2)->currentIndex();
          used |= (p == a && q == b) || (p == b && q == a);
        }
        if (!used) { selectedStart = a; selectedEnd = b; break; }
      }
    if (selectedStart < 0) {
      rods_->setRowCount(row);
      generatedNames_.resize(row);
      const QSignalBlocker block(rodCount_);
      rodCount_->setValue(row);
      message_->setText(tr("Все пары точек уже соединены стержнями."));
      updating_ = false;
      refreshPreview();
      return;
    }
    endpoint(row,1)->setCurrentIndex(selectedStart);
    endpoint(row,2)->setCurrentIndex(selectedEnd);
    updateGeneratedName(row);
  }
  updating_ = false;
  message_->clear();
  refreshPreview();
}
void GeometryEditor::refreshPreview() {
  QVector<QPointF> positions;
  QStringList names, rodNames;
  QVector<QPair<int,int>> edges;
  QVector<mohr::RodDto> previewRods;
  for (int row = 0; row < points_->rowCount(); ++row) {
    double x = 0, y = 0;
    parseNumber(cell(points_,row,1),x);
    parseNumber(cell(points_,row,2),y);
    positions.append(QPointF(x,y));
    names.append(pointName(row));
  }
  for (int row = 0; row < rods_->rowCount(); ++row) {
    edges.append({endpoint(row,1)->currentIndex(), endpoint(row,2)->currentIndex()});
    rodNames.append(cell(rods_,row,0));
    mohr::RodDto item{};
    item.hinge_start = qobject_cast<QCheckBox*>(rods_->cellWidget(row,4))->isChecked();
    item.hinge_end = qobject_cast<QCheckBox*>(rods_->cellWidget(row,5))->isChecked();
    previewRods.append(item);
  }
  preview_->beginUpdate();
  preview_->setModel(positions,names,edges,rodNames);
  preview_->setLoads({},previewRods);
  preview_->endUpdate();
  preview_->resetView();
}
void GeometryEditor::accept() {
  if (points_->rowCount() < originalPointCount_ || rods_->rowCount() < originalRodCount_) {
    if (QMessageBox::question(this, tr("Удалить строки"),
      tr("Удаляемые точки, стержни и связанные с ними расчётные данные будут удалены. Продолжить?"))
        != QMessageBox::Yes) return;
  }
  rust::Vec<mohr::GeometryNodeDto> nodes;
  rust::Vec<mohr::GeometryRodDto> rods;
  for (int row = 0; row < points_->rowCount(); ++row) {
    double x = 0, y = 0;
    if (!parseNumber(cell(points_,row,1),x) || !parseNumber(cell(points_,row,2),y)) {
      message_->setText(tr("Точка, строка %1: координаты должны быть конечными числами.").arg(row+1));
      return;
    }
    mohr::GeometryNodeDto item;
    item.id = utf8(pointName(row)); item.x = x; item.y = y;
    nodes.push_back(std::move(item));
  }
  for (int row = 0; row < rods_->rowCount(); ++row) {
    double h = 0;
    if (!parseNumber(cell(rods_,row,3),h)) {
      message_->setText(tr("Стержень, строка %1: h должно быть конечным числом.").arg(row+1));
      return;
    }
    mohr::GeometryRodDto item;
    item.id = utf8(cell(rods_,row,0));
    item.start = endpoint(row,1)->currentIndex();
    item.end = endpoint(row,2)->currentIndex();
    item.h = h;
    item.hinge_start = qobject_cast<QCheckBox*>(rods_->cellWidget(row,4))->isChecked();
    item.hinge_end = qobject_cast<QCheckBox*>(rods_->cellWidget(row,5))->isChecked();
    rods.push_back(std::move(item));
  }
  try {
    mohr::document_replace_geometry(document_, std::move(nodes), std::move(rods));
    QDialog::accept();
  } catch (const std::exception &error) {
    message_->setText(QString::fromUtf8(error.what()));
  }
}

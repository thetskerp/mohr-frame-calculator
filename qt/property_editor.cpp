#include "property_editor.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QEvent>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>

#include <algorithm>
#include <cstdint>

namespace {
QString qs(const rust::String &s) {
  return QString::fromUtf8(s.data(), static_cast<qsizetype>(s.size()));
}

std::string utf8(const QString &s) {
  const QByteArray bytes = s.toUtf8();
  return std::string(bytes.constData(), static_cast<size_t>(bytes.size()));
}

class CompactNumber final : public QDoubleSpinBox {
public:
  CompactNumber() { lineEdit()->installEventFilter(this); }

protected:
  QString textFromValue(double value) const override {
    return locale().toString(value, 'g', 10);
  }
  bool eventFilter(QObject *watched, QEvent *event) override {
    if (watched == lineEdit() && event->type() == QEvent::MouseButtonRelease)
      QTimer::singleShot(0, this, [this] { lineEdit()->selectAll(); });
    return QDoubleSpinBox::eventFilter(watched, event);
  }
};

QDoubleSpinBox *decimal(const char *name, double value) {
  auto *input = new CompactNumber;
  input->setObjectName(name);
  input->setRange(-1e15, 1e15);
  input->setDecimals(10);
  input->setValue(value);
  input->setMinimumWidth(80);
  input->setKeyboardTracking(false);
  return input;
}

QTableWidgetItem *item(const QString &text, bool editable = true) {
  auto *result = new QTableWidgetItem(text);
  if (!editable) result->setFlags(result->flags() & ~Qt::ItemIsEditable);
  return result;
}

QComboBox *assignment(QTableWidget *table, int row, int column) {
  return qobject_cast<QComboBox *>(table->cellWidget(row, column));
}
} // namespace

PropertyEditor::PropertyEditor(mohr::Document &document, QWidget *parent)
    : QDialog(parent), document_(document) {
  setWindowTitle(tr("Табличное задание свойств стержней"));
  resize(1130, 710);
  setMinimumSize(850, 540);
  auto *root = new QVBoxLayout(this);
  auto *hint = new QLabel(tr("Без галочки задаётся коэффициент перед EI или α. С галочкой — численное значение: мантисса × 10ⁿ."));
  hint->setWordWrap(true);
  root->addWidget(hint);

  auto *panels = new QHBoxLayout;
  for (int kind = 0; kind < 2; ++kind) {
    const bool alpha = kind == 1;
    auto *group = new QGroupBox(alpha ? tr("Коэффициенты α") : tr("Жёсткости EI"));
    auto *box = new QVBoxLayout(group);
    auto *countLine = new QHBoxLayout;
    countLine->addWidget(new QLabel(tr("Количество строк:")));
    counts_[kind] = new QSpinBox;
    counts_[kind]->setRange(1, 100);
    countLine->addWidget(counts_[kind]);
    countLine->addStretch();
    box->addLayout(countLine);

    auto *table = new QTableWidget;
    tables_[kind] = table;
    table->setColumnCount(4);
    table->setHorizontalHeaderLabels({tr("№"), tr("Название (опционально)"),
                                       tr("Значение"), tr("Задано численно")});
    table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
    table->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Stretch);
    table->horizontalHeader()->setSectionResizeMode(2, QHeaderView::Stretch);
    table->horizontalHeader()->setSectionResizeMode(3, QHeaderView::ResizeToContents);
    table->verticalHeader()->setVisible(false);
    table->setAlternatingRowColors(true);
    table->setSelectionBehavior(QAbstractItemView::SelectRows);
    box->addWidget(table);
    panels->addWidget(group, 1);

    const auto presets = mohr::document_properties(document_, alpha);
    for (const auto &preset : presets) addRow(kind, preset);
    if (table->rowCount() == 0) {
      mohr::PropertyDto initial;
      initial.id = 1;
      initial.name = alpha ? "Расширение 1" : "Жёсткость 1";
      initial.numeric = false;
      initial.factor = initial.mantissa = 1.0;
      initial.exponent = alpha ? -5 : 0;
      addRow(kind, initial);
    }
    counts_[kind]->setValue(table->rowCount());
    connect(counts_[kind], qOverload<int>(&QSpinBox::valueChanged), this,
            [this, kind](int value) { setCount(kind, value); });
    connect(table, &QTableWidget::itemChanged, this, [this, kind](QTableWidgetItem *changed) {
      if (changed && changed->column() == 1) updateAssignmentName(kind, changed->row());
    });
  }
  root->addLayout(panels, 3);

  auto *assignLabel = new QLabel(tr("Назначение стержням"));
  QFont font = assignLabel->font();
  font.setBold(true);
  assignLabel->setFont(font);
  root->addWidget(assignLabel);
  assignments_ = new QTableWidget;
  assignments_->setColumnCount(3);
  assignments_->setHorizontalHeaderLabels({tr("Стержень"), tr("Жёсткость EI"), tr("Коэффициент α")});
  assignments_->horizontalHeader()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
  assignments_->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Stretch);
  assignments_->horizontalHeader()->setSectionResizeMode(2, QHeaderView::Stretch);
  assignments_->verticalHeader()->setVisible(false);
  assignments_->setAlternatingRowColors(true);
  const auto rods = mohr::document_rods(document_);
  assignments_->setRowCount(static_cast<int>(rods.size()));
  for (int row = 0; row < static_cast<int>(rods.size()); ++row) {
    assignments_->setItem(row, 0, item(qs(rods[row].id), false));
    auto *ei = new QComboBox;
    ei->setProperty("presetId", rods[row].ei_preset);
    assignments_->setCellWidget(row, 1, ei);
    auto *alpha = new QComboBox;
    alpha->setProperty("presetId", rods[row].alpha_preset);
    assignments_->setCellWidget(row, 2, alpha);
  }
  refreshAssignments();
  root->addWidget(assignments_, 2);

  auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(tr("Применить"));
  buttons->button(QDialogButtonBox::Cancel)->setText(tr("Отмена"));
  connect(buttons, &QDialogButtonBox::accepted, this, &PropertyEditor::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &PropertyEditor::reject);
  root->addWidget(buttons);
}

void PropertyEditor::addRow(int kind, const mohr::PropertyDto &preset) {
  auto *table = tables_[kind];
  const int row = table->rowCount();
  table->insertRow(row);
  table->setRowHeight(row, 46);
  table->setItem(row, 0, item(QString::number(preset.id), false));
  table->setItem(row, 1, item(qs(preset.name)));

  auto *stack = new QStackedWidget;
  auto *symbolic = new QWidget;
  auto *symbolicLayout = new QHBoxLayout(symbolic);
  symbolicLayout->setContentsMargins(3, 1, 3, 1);
  symbolicLayout->addWidget(decimal("factor", preset.factor), 1);
  symbolicLayout->addWidget(new QLabel(kind == 0 ? tr("· EI") : tr("· α")));
  stack->addWidget(symbolic);
  auto *numeric = new QWidget;
  auto *numericLayout = new QHBoxLayout(numeric);
  numericLayout->setContentsMargins(3, 1, 3, 1);
  numericLayout->addWidget(decimal("mantissa", preset.mantissa), 1);
  numericLayout->addWidget(new QLabel(tr("× 10^")));
  auto *exponent = new QSpinBox;
  exponent->setObjectName("exponent");
  exponent->setRange(-30, 30);
  exponent->setValue(preset.exponent);
  exponent->setFixedWidth(65);
  numericLayout->addWidget(exponent);
  stack->addWidget(numeric);
  stack->setCurrentIndex(preset.numeric ? 1 : 0);
  table->setCellWidget(row, 2, stack);

  auto *checkContainer = new QWidget;
  auto *checkLayout = new QHBoxLayout(checkContainer);
  checkLayout->setContentsMargins(3, 0, 3, 0);
  auto *check = new QCheckBox;
  check->setObjectName("numeric");
  check->setChecked(preset.numeric);
  checkLayout->addWidget(check, 0, Qt::AlignCenter);
  table->setCellWidget(row, 3, checkContainer);
  connect(check, &QCheckBox::toggled, stack, [stack](bool enabled) {
    stack->setCurrentIndex(enabled ? 1 : 0);
    if (enabled) {
      auto *mantissa = stack->findChild<QDoubleSpinBox *>("mantissa");
      mantissa->setFocus();
      mantissa->selectAll();
    }
  });
}

void PropertyEditor::setCount(int kind, int count) {
  auto *table = tables_[kind];
  while (table->rowCount() > count) {
    const int last = table->rowCount() - 1;
    const int id = table->item(last, 0)->text().toInt();
    bool used = false;
    for (int row = 0; row < assignments_->rowCount(); ++row) {
      const auto *box = assignment(assignments_, row, kind + 1);
      used |= box && box->currentData().toInt() == id;
    }
    if (used) {
      QSignalBlocker block(counts_[kind]);
      counts_[kind]->setValue(table->rowCount());
      QMessageBox::information(this, tr("Набор назначен"),
        tr("Сначала назначьте стержням другой набор, затем уменьшите количество строк."));
      break;
    }
    table->removeRow(last);
  }
  while (table->rowCount() < count) {
    unsigned nextId = 1;
    for (int row = 0; row < table->rowCount(); ++row)
      nextId = std::max(nextId, table->item(row, 0)->text().toUInt() + 1);
    mohr::PropertyDto preset;
    preset.id = nextId;
    preset.name = utf8(kind == 0 ? tr("Жёсткость %1").arg(nextId)
                                : tr("Расширение %1").arg(nextId));
    preset.numeric = false;
    preset.factor = preset.mantissa = 1.0;
    preset.exponent = kind == 0 ? 0 : -5;
    addRow(kind, preset);
  }
  refreshAssignments();
}

void PropertyEditor::refreshAssignments() {
  if (!assignments_) return;
  for (int row = 0; row < assignments_->rowCount(); ++row) {
    for (int kind = 0; kind < 2; ++kind) {
      auto *box = assignment(assignments_, row, kind + 1);
      const int selected = box->count() ? box->currentData().toInt()
                                        : box->property("presetId").toInt();
      QSignalBlocker block(box);
      box->clear();
      box->addItem(tr("— не назначено —"), -1);
      auto *table = tables_[kind];
      for (int presetRow = 0; presetRow < table->rowCount(); ++presetRow) {
        const int id = table->item(presetRow, 0)->text().toInt();
        const QString name = table->item(presetRow, 1)->text().trimmed();
        box->addItem(name.isEmpty() ? QString("№ %1").arg(id) : name, id);
      }
      box->setCurrentIndex(std::max(0, box->findData(selected)));
    }
  }
}

void PropertyEditor::updateAssignmentName(int kind, int row) {
  auto *table = tables_[kind];
  const int id = table->item(row, 0)->text().toInt();
  const QString name = table->item(row, 1)->text().trimmed();
  for (int i = 0; i < assignments_->rowCount(); ++i) {
    auto *box = assignment(assignments_, i, kind + 1);
    const int index = box->findData(id);
    if (index >= 0) box->setItemText(index, name.isEmpty() ? QString("№ %1").arg(id) : name);
  }
}

rust::Vec<mohr::PropertyDto> PropertyEditor::collect(int kind) const {
  rust::Vec<mohr::PropertyDto> result;
  auto *table = tables_[kind];
  for (int row = 0; row < table->rowCount(); ++row) {
    mohr::PropertyDto preset;
    preset.id = table->item(row, 0)->text().toUInt();
    QString name = table->item(row, 1)->text().trimmed();
    if (name.isEmpty()) name = kind == 0 ? tr("Жёсткость %1").arg(preset.id)
                                         : tr("Расширение %1").arg(preset.id);
    preset.name = utf8(name);
    const auto *check = table->cellWidget(row, 3)->findChild<QCheckBox *>("numeric");
    const auto *stack = qobject_cast<QStackedWidget *>(table->cellWidget(row, 2));
    preset.numeric = check->isChecked();
    preset.factor = stack->findChild<QDoubleSpinBox *>("factor")->value();
    preset.mantissa = stack->findChild<QDoubleSpinBox *>("mantissa")->value();
    preset.exponent = stack->findChild<QSpinBox *>("exponent")->value();
    result.push_back(std::move(preset));
  }
  return result;
}

void PropertyEditor::accept() {
  rust::Vec<std::int32_t> eiAssignments;
  rust::Vec<std::int32_t> alphaAssignments;
  for (int row = 0; row < assignments_->rowCount(); ++row) {
    eiAssignments.push_back(assignment(assignments_, row, 1)->currentData().toInt());
    alphaAssignments.push_back(assignment(assignments_, row, 2)->currentData().toInt());
  }
  try {
    mohr::document_replace_properties(document_, collect(0), collect(1),
                                      std::move(eiAssignments), std::move(alphaAssignments));
    QDialog::accept();
  } catch (const std::exception &error) {
    QMessageBox::warning(this, tr("Ошибка свойств"), QString::fromUtf8(error.what()));
  }
}

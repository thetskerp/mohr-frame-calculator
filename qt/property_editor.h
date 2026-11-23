#pragma once

#include "mohr-frame/src/bridge.rs.h"

#include <QDialog>

class QSpinBox;
class QTableWidget;

// Единый табличный редактор наборов EI, α и назначений стержням.
class PropertyEditor final : public QDialog {
public:
  explicit PropertyEditor(mohr::Document &document, QWidget *parent = nullptr);

protected:
  void accept() override;

private:
  void addRow(int kind, const mohr::PropertyDto &preset);
  void setCount(int kind, int count);
  void refreshAssignments();
  void updateAssignmentName(int kind, int row);
  rust::Vec<mohr::PropertyDto> collect(int kind) const;

  mohr::Document &document_;
  QTableWidget *tables_[2] = {};
  QSpinBox *counts_[2] = {};
  QTableWidget *assignments_ = nullptr;
};

#pragma once

#include "frame_view.h"
#include "mohr-frame/src/bridge.rs.h"

#include <QDialog>
#include <QVector>

class QTableWidget;
class QSpinBox;
class QLabel;
class QComboBox;

// Единый табличный редактор геометрии, как в версии с egui.
class GeometryEditor final : public QDialog {
public:
  GeometryEditor(mohr::Document &document, QWidget *parent = nullptr);

protected:
  void accept() override;

private:
  void resizePoints(int count);
  void resizeRods(int count);
  void updateRodSelectors();
  void updateGeneratedName(int row);
  void refreshPreview();
  QString pointName(int row) const;
  QString generatedRodName(int row) const;
  QComboBox *endpoint(int row, int column) const;

  mohr::Document &document_;
  QTableWidget *points_ = nullptr;
  QTableWidget *rods_ = nullptr;
  QSpinBox *pointCount_ = nullptr;
  QSpinBox *rodCount_ = nullptr;
  QLabel *message_ = nullptr;
  FrameView *preview_ = nullptr;
  QVector<bool> generatedNames_;
  int originalPointCount_ = 0;
  int originalRodCount_ = 0;
  bool updating_ = false;
};

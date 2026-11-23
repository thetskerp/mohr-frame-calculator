#pragma once

#include <QGraphicsView>
#include <QGraphicsScene>
#include <QStringList>
#include <QColor>
#include <QVector>
#include <QString>

namespace mohr {
struct NodeDto;
struct RodDto;
struct CurvePointDto;
struct ReactionDto;
struct PointLoadDto;
struct MotionDto;
}

struct ViewColors {
  QColor canvas = QColor("#101923");
  QColor frame = QColor("#e7edf5");
  QColor secondary = QColor("#aeb9c6");
  QColor dimensions = QColor("#9ca7b3");
  QColor loads = QColor("#ef6b6b");
  QColor reactions = QColor("#46e4d6");
  QColor diagrams = QColor("#ff9d54");
  QColor supports = QColor("#66c7ff");
  QColor motions = QColor("#44d9d0");
  QColor temperatures = QColor("#f69c56");
};

class FrameView final : public QGraphicsView {
  Q_OBJECT
public:
  explicit FrameView(QWidget *parent = nullptr);
  void setModel(const QVector<QPointF> &nodes, const QStringList &nodeNames,
                const QVector<QPair<int, int>> &rods, const QStringList &rodNames);
  void setDiagram(const QVector<mohr::CurvePointDto> &points, const QString &title,
                  const QColor &color, int decimals);
  void setReactions(const QVector<mohr::ReactionDto> &reactions, bool actual,
                    int decimals);
  void setLoads(const QVector<mohr::NodeDto> &nodes,
                const QVector<mohr::RodDto> &rods);
  void setPointLoads(const QVector<mohr::PointLoadDto> &loads);
  void setMotions(const QVector<mohr::MotionDto> &motions);
  void setTemperatures(const QVector<mohr::RodDto> &rods);
  void setDiagramHeight(double height);
  void setShowValues(bool visible);
  void setShowDimensions(bool visible);
  void beginUpdate() { batching_ = true; }
  void endUpdate() { batching_ = false; redraw(); }
  void resetView();
  double zoom() const { return transform().m11(); }
  double diagramHeight() const { return diagramHeight_; }
  double lastRedrawMs() const { return lastRedrawMs_; }
  void setZoom(double value);
  void setColors(const ViewColors &colors);
signals:
  void objectSelected(bool rod, int index);
protected:
  void wheelEvent(QWheelEvent *event) override;
  void mouseDoubleClickEvent(QMouseEvent *event) override;
private:
  void redraw();
  QPointF screenPoint(int node) const;
  void addLabel(const QPointF &point, const QString &text, const QColor &color,
                const QPointF &offset = QPointF(8, -22));
  void addDiagramLabel(const QPointF &point, const QPointF &outward,
                       const QString &text);
  void addArrow(const QPointF &start, const QPointF &end, const QColor &color,
                const QString &label);
  void addMoment(const QPointF &point, double value, const QColor &color,
                 const QString &name, bool magnitudeOnly = false);
  QGraphicsScene scene_;
  QVector<QPointF> nodes_;
  QStringList nodeNames_;
  QVector<QPair<int, int>> rods_;
  QStringList rodNames_;
  QVector<mohr::CurvePointDto> diagram_;
  QVector<mohr::ReactionDto> reactions_;
  QVector<mohr::NodeDto> loadsNodes_;
  QVector<mohr::RodDto> loadsRods_;
  QVector<mohr::PointLoadDto> pointLoads_;
  QVector<mohr::MotionDto> motions_;
  QVector<mohr::RodDto> temperatures_;
  QString diagramTitle_;
  QColor diagramColor_ = QColor("#ff9a48");
  ViewColors colors_;
  int decimals_ = 2;
  bool actualReactions_ = true;
  bool showValues_ = true;
  bool showDimensions_ = false;
  bool fitPending_ = true;
  bool batching_ = false;
  double diagramHeight_ = 1.0;
  double lastRedrawMs_ = 0.0;
};

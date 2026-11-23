#include "frame_view.h"
#include "mohr-frame/src/bridge.rs.h"

#include <QGraphicsEllipseItem>
#include <QGraphicsItemGroup>
#include <QGraphicsLineItem>
#include <QGraphicsPathItem>
#include <QGraphicsTextItem>
#include <QWheelEvent>
#include <QElapsedTimer>
#include <QFontMetricsF>
#include <QPainter>
#include <algorithm>
#include <cmath>

static QPointF normalized(QPointF p) {
  const double length = std::hypot(p.x(), p.y());
  return length > 1e-12 ? p / length : QPointF(0, 0);
}

// Ордината хранит координату точки эпюры как позицию самого элемента.
// Смещение текста находится внутри элемента и всегда измеряется в пикселях.
class DiagramValueItem final : public QGraphicsItem {
public:
  DiagramValueItem(const QPointF &point, const QString &text, const QColor &color,
                   const QPointF &outward)
      : text_(text), color_(color) {
    setPos(point);
    setFlag(ItemIgnoresTransformations);
    setZValue(30);
    const QFontMetricsF metrics(font_);
    baseline_ = outward * 42 + QPointF(-metrics.horizontalAdvance(text_) / 2,
                                        metrics.ascent() / 2);
    bounds_ = metrics.boundingRect(text_).translated(baseline_).adjusted(-2, -2, 2, 2);
  }
  QRectF boundingRect() const override { return bounds_; }
  void paint(QPainter *painter, const QStyleOptionGraphicsItem *, QWidget *) override {
    painter->setFont(font_);
    painter->setPen(color_);
    painter->drawText(baseline_, text_);
  }
private:
  QString text_;
  QColor color_;
  QFont font_ = QFont("Arial", 10);
  QPointF baseline_;
  QRectF bounds_;
};

FrameView::FrameView(QWidget *parent) : QGraphicsView(parent) {
  setScene(&scene_);
  setRenderHints(QPainter::Antialiasing | QPainter::TextAntialiasing);
  setBackgroundBrush(colors_.canvas);
  setDragMode(QGraphicsView::ScrollHandDrag);
  setTransformationAnchor(QGraphicsView::AnchorUnderMouse);
  connect(&scene_, &QGraphicsScene::selectionChanged, this, [this] {
    auto items = scene_.selectedItems();
    if (!items.isEmpty())
      emit objectSelected(items.first()->data(0).toBool(), items.first()->data(1).toInt());
  });
}

void FrameView::setModel(const QVector<QPointF> &nodes, const QStringList &names,
                         const QVector<QPair<int, int>> &rods, const QStringList &rodNames) {
  if (nodes_.isEmpty() && !nodes.isEmpty()) fitPending_ = true;
  nodes_ = nodes; nodeNames_ = names; rods_ = rods; rodNames_ = rodNames;
  if (!batching_) redraw();
}
void FrameView::setDiagram(const QVector<mohr::CurvePointDto> &points, const QString &title,
                            const QColor &color, int decimals) {
  diagram_ = points; diagramTitle_ = title; diagramColor_ = color; decimals_ = decimals;
  if (!batching_) redraw();
}
void FrameView::setReactions(const QVector<mohr::ReactionDto> &reactions, bool actual, int decimals) {
  reactions_ = reactions; actualReactions_ = actual; decimals_ = decimals;
  if (!batching_) redraw();
}
void FrameView::setLoads(const QVector<mohr::NodeDto> &nodes,
                         const QVector<mohr::RodDto> &rods) {
  loadsNodes_ = nodes; loadsRods_ = rods; if (!batching_) redraw();
}
void FrameView::setPointLoads(const QVector<mohr::PointLoadDto> &loads) {
  pointLoads_ = loads; if (!batching_) redraw();
}
void FrameView::setMotions(const QVector<mohr::MotionDto> &motions) {
  motions_ = motions; if (!batching_) redraw();
}
void FrameView::setTemperatures(const QVector<mohr::RodDto> &rods) {
  temperatures_ = rods; if (!batching_) redraw();
}
void FrameView::setDiagramHeight(double height) { diagramHeight_ = height; if (!batching_) redraw(); }
void FrameView::setShowValues(bool visible) { showValues_ = visible; if (!batching_) redraw(); }
void FrameView::setShowDimensions(bool visible) { showDimensions_ = visible; if (!batching_) redraw(); }
void FrameView::setZoom(double value) {
  if (!std::isfinite(value) || value < 0.05 || value > 40.0) return;
  resetTransform(); scale(value, value);
}
void FrameView::setColors(const ViewColors &colors) {
  colors_ = colors;
  diagramColor_ = colors.diagrams;
  setBackgroundBrush(colors.canvas);
  if (!batching_) redraw();
}
void FrameView::resetView() {
  fitPending_ = true; redraw();
}
void FrameView::wheelEvent(QWheelEvent *event) {
  const double factor = std::pow(1.0015, event->angleDelta().y());
  if (event->modifiers() & Qt::ShiftModifier) {
    diagramHeight_ = std::clamp(diagramHeight_ * factor, 0.05, 20.0);
    redraw();
  } else {
    if (zoom() * factor >= 0.05 && zoom() * factor <= 40.0)
      scale(factor, factor);
  }
  event->accept();
}
void FrameView::mouseDoubleClickEvent(QMouseEvent *event) {
  resetView();
  event->accept();
}
QPointF FrameView::screenPoint(int node) const {
  return node >= 0 && node < nodes_.size() ? QPointF(nodes_[node].x() * 110, -nodes_[node].y() * 110)
                                           : QPointF();
}
void FrameView::addLabel(const QPointF &point, const QString &text, const QColor &color,
                         const QPointF &offset) {
  auto *item = scene_.addText(text);
  item->setDefaultTextColor(color);
  item->setFlag(QGraphicsItem::ItemIgnoresTransformations);
  item->setZValue(30);
  for (int attempt = 0; attempt < 9; ++attempt) {
    const int row = (attempt + 1) / 2;
    const int shift = attempt == 0 ? 0 : (attempt % 2 ? -1 : 1) * row * 20;
    item->setPos(point + offset + QPointF(0, shift));
    const QRectF box = item->sceneBoundingRect().adjusted(-3, -2, 3, 2);
    bool overlapsText = false;
    for (auto *other : scene_.items(box)) {
      if (other != item && other->type() == QGraphicsTextItem::Type) {
        overlapsText = true;
        break;
      }
    }
    if (!overlapsText) break;
  }
}
void FrameView::addDiagramLabel(const QPointF &point, const QPointF &outward,
                                const QString &text) {
  scene_.addItem(new DiagramValueItem(point, text, diagramColor_, outward));
}
void FrameView::addArrow(const QPointF &start, const QPointF &end,
                         const QColor &color, const QString &label) {
  QPen pen(color, 2.5);
  pen.setCosmetic(true);
  scene_.addLine(QLineF(start, end), pen);
  const QPointF u = normalized(end - start);
  const QPointF n(-u.y(), u.x());
  scene_.addLine(QLineF(end, end - u * 11 + n * 5), pen);
  scene_.addLine(QLineF(end, end - u * 11 - n * 5), pen);
  if (!label.isEmpty())
    scene_.addItem(new DiagramValueItem(start, label, color, normalized(start-end)));
}

void FrameView::addMoment(const QPointF &point, double value, const QColor &color,
                          const QString &name, bool magnitudeOnly) {
  auto *anchor = new QGraphicsItemGroup;
  anchor->setFlag(QGraphicsItem::ItemIgnoresTransformations);
  anchor->setPos(point);
  anchor->setZValue(20);
  scene_.addItem(anchor);
  constexpr double radius = 20.0;
  constexpr double pi = 3.141592653589793;
  const double begin = value > 0 ? -120.0 : 120.0;
  const double sweep = value > 0 ? 240.0 : -240.0;
  auto at = [&](double degrees) {
    const double radians = degrees * pi / 180.0;
    return QPointF(radius * std::cos(radians), -radius * std::sin(radians));
  };
  QPainterPath path(at(begin));
  for (int step = 1; step <= 24; ++step)
    path.lineTo(at(begin + sweep * step / 24.0));
  auto *arc = new QGraphicsPathItem(path, anchor);
  arc->setPen(QPen(color, 2.5));
  const QPointF tip = at(begin + sweep);
  const QPointF tangent = normalized(tip - at(begin + sweep * 23.0 / 24.0));
  const QPointF side(-tangent.y(), tangent.x());
  auto *arrowA = new QGraphicsLineItem(QLineF(tip, tip - tangent*11 + side*5), anchor);
  auto *arrowB = new QGraphicsLineItem(QLineF(tip, tip - tangent*11 - side*5), anchor);
  arrowA->setPen(QPen(color, 2.5));
  arrowB->setPen(QPen(color, 2.5));
  const QString number = QString::number(magnitudeOnly ? std::abs(value) : value, 'f', decimals_);
  auto *label = new QGraphicsTextItem(QString("%1=%2").arg(name, number), anchor);
  label->setDefaultTextColor(color);
  label->setPos(27, -36);
}

void FrameView::redraw() {
  QElapsedTimer elapsed;
  elapsed.start();
  scene_.clear();
  const QColor loadColor = colors_.loads;
  QPointF center;
  for (const auto &node : nodes_) center += QPointF(node.x()*110, -node.y()*110);
  if (!nodes_.isEmpty()) center /= nodes_.size();
  const QPen memberPen(colors_.frame, 2.5);
  for (int i = 0; i < rods_.size(); ++i) {
    const auto [a, b] = rods_[i];
    if (a < 0 || b < 0 || a >= nodes_.size() || b >= nodes_.size()) continue;
    const QPointF start = screenPoint(a), end = screenPoint(b);
    auto *item = scene_.addLine(QLineF(start, end), memberPen);
    item->setFlag(QGraphicsItem::ItemIsSelectable);
    item->setData(0, true); item->setData(1, i);
    item->setZValue(3);
    addLabel((start + end) / 2, rodNames_.value(i), colors_.secondary, QPointF(5, -27));
    if (showDimensions_) {
      const QPointF unit = normalized(end-start);
      QPointF normal(-unit.y(), unit.x());
      const QPointF middle = (start+end)/2;
      if (QPointF::dotProduct(normal, middle-center) < 0) normal = -normal;
      const QColor gray = colors_.dimensions;
      const QPen dimensionPen(gray, 1);
      const QPointF ds = start+normal*36, de = end+normal*36;
      scene_.addLine(QLineF(start+normal*8, start+normal*44), dimensionPen);
      scene_.addLine(QLineF(end+normal*8, end+normal*44), dimensionPen);
      scene_.addLine(QLineF(ds,de), dimensionPen);
      const QPointF tick = normalized(unit+normal)*7;
      scene_.addLine(QLineF(ds-tick,ds+tick), dimensionPen);
      scene_.addLine(QLineF(de-tick,de+tick), dimensionPen);
      auto *label = scene_.addText(QString::number(std::hypot(
        nodes_[b].x()-nodes_[a].x(), nodes_[b].y()-nodes_[a].y()), 'f', 2));
      label->setDefaultTextColor(gray);
      label->setFont(QFont("OpenGost Type A", 11));
      label->setFlag(QGraphicsItem::ItemIgnoresTransformations);
      label->setPos(middle+normal*42-QPointF(10,10));
    }
    if (i < loadsRods_.size()) {
      const auto &r = loadsRods_[i];
      for (const auto &entry : {std::pair{r.hinge_start,start}, std::pair{r.hinge_end,end}}) {
        if (entry.first)
          scene_.addEllipse(entry.second.x()-8, entry.second.y()-8, 16, 16,
            QPen(colors_.frame, 2), QBrush(colors_.canvas))->setZValue(8);
      }
      if (std::abs(r.qx) > 1e-10 || std::abs(r.qy) > 1e-10) {
        const QPointF load(r.qx, -r.qy), direction = normalized(load);
        QPen spine(loadColor, 2.5);
        spine.setCosmetic(true);
        scene_.addLine(QLineF(start-direction*35, end-direction*35), spine);
        const int count = std::clamp(static_cast<int>(QLineF(start,end).length()/45), 2, 12);
        for (int j = 0; j <= count; ++j) {
          const QPointF p = start + (end-start)*(static_cast<double>(j)/count);
          addArrow(p - direction*35, p, loadColor, QString());
        }
        QString qLabel;
        if (std::abs(r.qx) > 1e-10) qLabel = QString("qX=%1").arg(r.qx);
        if (std::abs(r.qy) > 1e-10) {
          if (!qLabel.isEmpty()) qLabel += ", ";
          qLabel += QString("qY=%1").arg(r.qy);
        }
        scene_.addItem(new DiagramValueItem((start+end)/2 - direction*35,
          qLabel, loadColor, QPointF(0, -1)));
      }
    }
  }
  for (int i = 0; i < nodes_.size(); ++i) {
    const QPointF p = screenPoint(i);
    auto *dot = scene_.addEllipse(p.x() - 5, p.y() - 5, 10, 10,
                                   QPen(colors_.frame, 2), QBrush(colors_.canvas));
    dot->setFlag(QGraphicsItem::ItemIsSelectable);
    dot->setData(0, false); dot->setData(1, i);
    dot->setZValue(10);
    addLabel(p, nodeNames_.value(i), colors_.frame);
  }
  // Ординаты выходят по нормали к экранному направлению стержня,
  // как и в сохранённом рендерере egui. Знаки берутся только из Rust.
  if (!diagram_.isEmpty()) {
    double peak = 0;
    for (const auto &p : diagram_) peak = std::max(peak, std::abs(p.value));
    const double factor = peak > 1e-12 ? 80 * diagramHeight_ / peak : 0;
    int cursor = 0;
    while (cursor < diagram_.size()) {
      const int rod = static_cast<int>(diagram_[cursor].rod_index);
      if (rod < 0 || rod >= rods_.size()) { ++cursor; continue; }
      const auto [a, b] = rods_[rod];
      if (a < 0 || b < 0 || a >= nodes_.size() || b >= nodes_.size()) { ++cursor; continue; }
      const QPointF start = screenPoint(a), end = screenPoint(b);
      const QPointF u = normalized(end - start), normal(-u.y(), u.x());
      QPainterPath path;
      bool begun = false;
      QPointF first, last;
      while (cursor < diagram_.size() && static_cast<int>(diagram_[cursor].rod_index) == rod) {
        const auto &value = diagram_[cursor++];
        const QPointF base = start + (end - start) * value.t;
        const QPointF p = base + normal * value.value * factor;
        if (!begun) { path.moveTo(p); first = p; begun = true; }
        else path.lineTo(p);
        last = p;
        const bool characteristic = std::abs(value.t) < 1e-9 ||
          std::abs(value.t - 0.5) < 1e-9 || std::abs(value.t - 1.0) < 1e-9;
        if (showValues_ && characteristic) {
          scene_.addEllipse(p.x()-2, p.y()-2, 4, 4, QPen(diagramColor_), QBrush(diagramColor_));
          const QPointF outward = value.value < 0 ? -normal : normal;
          const double rounded = std::round(value.value * std::pow(10.0, decimals_)) /
                                 std::pow(10.0, decimals_);
          addDiagramLabel(p, outward, QString::number(rounded, 'f', decimals_));
        }
      }
      if (begun) {
        auto *line = scene_.addPath(path, QPen(diagramColor_, 2));
        line->setZValue(5);
        scene_.addLine(QLineF(start, first), QPen(diagramColor_, 1.2));
        scene_.addLine(QLineF(end, last), QPen(diagramColor_, 1.2));
      }
    }
  }
  for (const auto &node : loadsNodes_) {
    const int index = static_cast<int>(node.index);
    if (index < 0 || index >= nodes_.size()) continue;
    const QPointF p = screenPoint(index);
    if (node.support != 0) {
      const QColor blue = colors_.supports;
      if (node.support == 1) {
        scene_.addLine(QLineF(p + QPointF(-4,-19), p + QPointF(-4,19)), QPen(blue,3));
        for (int h = 0; h < 4; ++h)
          scene_.addLine(QLineF(p + QPointF(-4,-15+h*10), p + QPointF(-13,-22+h*10)), QPen(blue));
      } else {
        scene_.addPolygon(QPolygonF({p + QPointF(0, 8), p + QPointF(-13, 28),
                                     p + QPointF(13, 28)}), QPen(blue, 2));
        if (node.support == 3) {
          for (double x : {-7.0, 7.0})
            scene_.addEllipse(p.x()+x-3, p.y()+28, 6, 6, QPen(blue));
        }
        if (node.support == 4)
          scene_.addLine(QLineF(p + QPointF(-17, 34), p + QPointF(17, 34)), QPen(blue,2));
      }
    }
    if (std::abs(node.fx) > 1e-10)
      addArrow(p - QPointF(node.fx > 0 ? 55 : -55, 0), p,
               loadColor, QString("Fx=%1").arg(node.fx));
    if (std::abs(node.fy) > 1e-10)
      addArrow(p + QPointF(0, node.fy > 0 ? 55 : -55), p,
               loadColor, QString("Fy=%1").arg(node.fy));
    if (std::abs(node.mz) > 1e-10)
      addMoment(p, node.mz, loadColor, "Mz");
  }
  for (const auto &load : pointLoads_) {
    const int rod = static_cast<int>(load.rod);
    if (rod < 0 || rod >= rods_.size()) continue;
    const auto [a,b] = rods_[rod];
    const QPointF p = screenPoint(a) + (screenPoint(b)-screenPoint(a))*load.xi;
    const QPointF direction = normalized(QPointF(load.fx, -load.fy));
    if (std::hypot(load.fx, load.fy) > 1e-10)
      addArrow(p-direction*40, p, loadColor,
               QString("F=(%1;%2)").arg(load.fx).arg(load.fy));
    if (std::abs(load.mz) > 1e-10)
      addMoment(p, load.mz, loadColor, "Mz");
  }
  for (const auto &motion : motions_) {
    if (static_cast<int>(motion.node) >= nodes_.size()) continue;
    const QPointF p = screenPoint(motion.node);
    QPointF axis;
    if (motion.direction == 0) axis = QPointF(1,0);
    if (motion.direction == 1) axis = QPointF(0,-1);
    if (motion.direction == 3) {
      const double radians = motion.normal_deg * 3.141592653589793 / 180.0;
      axis = QPointF(std::cos(radians), -std::sin(radians));
    }
    if (motion.direction == 2)
      addLabel(p, QString("cφ=%1 %2").arg(motion.displacement)
        .arg(motion.displacement >= 0 ? "↺" : "↻"), colors_.motions);
    else
      addArrow(p, p+axis*(motion.displacement >= 0 ? 34 : -34),
        colors_.motions, QString("c=%1").arg(motion.displacement));
  }
  for (const auto &r : temperatures_) {
    if (!r.has_temperature || static_cast<int>(r.index) >= rods_.size()) continue;
    const auto [a,b] = rods_[r.index];
    const QPointF start = screenPoint(a), end = screenPoint(b);
    const QPointF u = normalized(end-start), normal(-u.y(),u.x());
    const QPointF middle = (start+end)/2;
    addLabel(middle-normal*28, QString("t₁=%1").arg(r.t_upper),
      colors_.temperatures, QPointF(0,-12));
    addLabel(middle+normal*28, QString("t₂=%1").arg(r.t_lower),
      colors_.temperatures, QPointF(0,-12));
    addLabel(middle, QString("Δt=%1, t₀=%2")
      .arg(std::abs(r.t_upper-r.t_lower)).arg((r.t_upper+r.t_lower)/2),
      colors_.temperatures, QPointF(18,20));
  }
  for (const auto &r : reactions_) {
    if (static_cast<int>(r.node_index) >= nodes_.size()) continue;
    const QPointF p = screenPoint(static_cast<int>(r.node_index));
    const auto arrow = [this, p](double signedValue, QPointF axis, const QString &name) {
      if (std::abs(signedValue) < 1e-10) return;
      const double direction = actualReactions_ ? (signedValue > 0 ? 1 : -1) : 1;
      const double labelValue = actualReactions_ ? std::abs(signedValue) : signedValue;
      const QPointF start = p + axis * direction * 18;
      const QPointF end = p + axis * direction * 76;
      addArrow(start, end, colors_.reactions, QString());
      const QPointF labelOffset = std::abs(axis.x()) > 0.5
        ? QPointF(direction > 0 ? 3 : -82, -30)
        : QPointF(12, direction > 0 ? -14 : -12);
      addLabel(end, QString("%1=%2").arg(name).arg(labelValue, 0, 'f', decimals_),
               colors_.reactions, labelOffset);
    };
    arrow(r.x, QPointF(1, 0), "Rx");
    arrow(r.y, QPointF(0, -1), "Ry");
    if (std::abs(r.mz) > 1e-10)
      addMoment(p, r.mz, colors_.reactions, "M", actualReactions_);
  }
  QRectF bounds = scene_.itemsBoundingRect().adjusted(-90, -90, 90, 90);
  if (bounds.width() < 100) bounds.adjust(-50, 0, 50, 0);
  if (bounds.height() < 100) bounds.adjust(0, -50, 0, 50);
  scene_.setSceneRect(bounds);
  if (fitPending_) {
    fitInView(bounds, Qt::KeepAspectRatio);
    fitPending_ = false;
  }
  lastRedrawMs_ = elapsed.nsecsElapsed() / 1.0e6;
}

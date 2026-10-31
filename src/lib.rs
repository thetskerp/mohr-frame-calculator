//! Расчет перемещений плоской стержневой рамы методом Мора.
//!
//! Знак момента задается стороной растянутых волокон:
//! верхние/левые волокна имеют знак «минус», нижние/правые — «плюс».

use std::fmt;

use serde::{Deserialize, Serialize};

const EPS: f64 = 1.0e-9;

fn default_rigidity_factor() -> f64 {
    1.0
}

fn default_section_height() -> f64 {
    0.5
}

fn default_thermal_expansion_factor() -> f64 {
    1.0
}

/// Точка в глобальной декартовой системе координат.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Геометрическое расположение стержня.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentOrientation {
    Horizontal,
    Vertical,
    Inclined,
}

impl fmt::Display for SegmentOrientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Horizontal => "горизонтальный",
            Self::Vertical => "вертикальный",
            Self::Inclined => "наклонный",
        };
        write!(f, "{name}")
    }
}

/// Сторона растянутых волокон и соответствующий ей знак ординаты.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FiberSide {
    /// Верхние волокна горизонтального или левые волокна вертикального стержня.
    UpperOrLeft,
    /// Нижние волокна горизонтального или правые волокна вертикального стержня.
    LowerOrRight,
    /// Ордината лежит на базовой линии (применяется только для нуля).
    OnAxis,
}

impl FiberSide {
    pub fn sign(self) -> f64 {
        match self {
            Self::UpperOrLeft => -1.0,
            Self::LowerOrRight => 1.0,
            Self::OnAxis => 0.0,
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::UpperOrLeft => "верх/лево (−)",
            Self::LowerOrRight => "низ/право (+)",
            Self::OnAxis => "ось (0)",
        }
    }
}

/// Ордината эпюры: неотрицательный модуль и сторона растянутых волокон.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ordinate {
    pub magnitude: f64,
    pub fiber: FiberSide,
}

impl Ordinate {
    pub fn new(magnitude: f64, fiber: FiberSide) -> Result<Self, String> {
        if !magnitude.is_finite() {
            return Err("ордината должна быть конечным числом".into());
        }
        if magnitude < 0.0 {
            return Err("модуль ординаты не может быть отрицательным".into());
        }
        if magnitude > EPS && fiber == FiberSide::OnAxis {
            return Err("ненулевая ордината не может лежать на базовой линии".into());
        }
        Ok(Self { magnitude, fiber })
    }

    /// Создает ординату из алгебраического значения.
    pub fn from_signed(value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("ордината должна быть конечным числом".into());
        }
        let fiber = if value > EPS {
            FiberSide::LowerOrRight
        } else if value < -EPS {
            FiberSide::UpperOrLeft
        } else {
            FiberSide::OnAxis
        };
        Ok(Self {
            magnitude: value.abs(),
            fiber,
        })
    }

    pub fn signed_value(self) -> f64 {
        self.magnitude * self.fiber.sign()
    }
}

/// Вид функции эпюры на участке.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagramKind {
    Constant,
    Linear,
    Parabolic,
}

impl fmt::Display for DiagramKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Constant => "константа",
            Self::Linear => "линейная",
            Self::Parabolic => "параболическая",
        };
        write!(f, "{name}")
    }
}

/// Эпюра изгибающих моментов на одном стержне.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagram {
    pub kind: DiagramKind,
    pub left: Ordinate,
    pub right: Ordinate,
    /// Модуль интенсивности распределенной нагрузки для параболы.
    pub q: Option<f64>,
    /// Сторона, в которую направлена выпуклость по действию q.
    pub q_fiber: Option<FiberSide>,
    pub has_extremum: bool,
}

impl Default for Diagram {
    fn default() -> Self {
        let zero = Ordinate {
            magnitude: 0.0,
            fiber: FiberSide::OnAxis,
        };
        Self {
            kind: DiagramKind::Linear,
            left: zero,
            right: zero,
            q: None,
            q_fiber: None,
            has_extremum: false,
        }
    }
}

impl Diagram {
    pub fn new(
        kind: DiagramKind,
        left: Ordinate,
        right: Ordinate,
        q: Option<f64>,
        q_fiber: Option<FiberSide>,
        has_extremum: bool,
    ) -> Result<Self, String> {
        let diagram = Self {
            kind,
            left,
            right,
            q,
            q_fiber,
            has_extremum,
        };
        diagram.validate()?;
        Ok(diagram)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.kind == DiagramKind::Constant
            && (self.left.signed_value() - self.right.signed_value()).abs() > EPS
        {
            return Err("у постоянной эпюры левая и правая ординаты должны совпадать".into());
        }

        match self.kind {
            DiagramKind::Parabolic => {
                let q = self
                    .q
                    .ok_or_else(|| "для параболической эпюры требуется q".to_string())?;
                if !q.is_finite() || q < 0.0 {
                    return Err("q должен быть конечным неотрицательным числом".into());
                }
                match self.q_fiber {
                    Some(FiberSide::UpperOrLeft | FiberSide::LowerOrRight) => {}
                    _ => return Err("для параболы требуется ненулевая сторона действия q".into()),
                }
            }
            _ => {
                if self.q.is_some() || self.q_fiber.is_some() || self.has_extremum {
                    return Err("q и флаг экстремума допустимы только для параболы".into());
                }
            }
        }
        Ok(())
    }

    pub fn left_value(&self) -> f64 {
        self.left.signed_value()
    }

    pub fn right_value(&self) -> f64 {
        self.right.signed_value()
    }

    /// Ордината в середине участка.
    ///
    /// Для параболы поправка qL²/8 получает знак стороны, в которую
    /// направлено действие нагрузки. Для линейной эпюры это полусумма концов.
    pub fn midpoint_value(&self, length: f64) -> f64 {
        self.value_at(length, 0.5)
    }

    /// Алгебраическая ордината эпюры в относительной координате `t` (0…1).
    /// Используется как расчетным ядром, так и графическим интерфейсом.
    pub fn value_at(&self, length: f64, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let chord = self.left_value() * (1.0 - t) + self.right_value() * t;
        if self.kind == DiagramKind::Parabolic {
            let q = self.q.unwrap_or(0.0);
            let sign = self.q_fiber.map_or(0.0, FiberSide::sign);
            chord + sign * q * length * length * t * (1.0 - t) / 2.0
        } else {
            chord
        }
    }

    /// Точная алгебраическая площадь эпюры на участке.
    ///
    /// Для параболы интегрируется заданная квадратичная функция, поэтому
    /// численный метод Симпсона здесь не используется.
    pub fn signed_area(&self, length: f64) -> f64 {
        let chord_area = length * (self.left_value() + self.right_value()) / 2.0;
        if self.kind == DiagramKind::Parabolic {
            let chord_mid = (self.left_value() + self.right_value()) / 2.0;
            let parabolic_ordinate = self.midpoint_value(length) - chord_mid;
            chord_area + 2.0 * length * parabolic_ordinate / 3.0
        } else {
            chord_area
        }
    }
}

/// Стержень рамы и две заданные на нем эпюры.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub length: f64,
    pub orientation: SegmentOrientation,
    pub start: Point,
    pub end: Point,
    /// Коэффициент жесткости участка: EI участка = rigidity_factor * EI базы.
    #[serde(default = "default_rigidity_factor")]
    pub rigidity_factor: f64,
    pub load_diagram: Diagram,
    pub unit_diagram: Diagram,
}

impl Segment {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("ID участка не может быть пустым".into());
        }
        if !self.length.is_finite() || self.length <= 0.0 {
            return Err("длина участка должна быть положительным конечным числом".into());
        }
        if !self.rigidity_factor.is_finite() || self.rigidity_factor <= 0.0 {
            return Err(
                "коэффициент жесткости EI должен быть положительным конечным числом".into(),
            );
        }
        for coordinate in [self.start.x, self.start.y, self.end.x, self.end.y] {
            if !coordinate.is_finite() {
                return Err("координаты должны быть конечными числами".into());
            }
        }
        if (self.start.x - self.end.x).abs() < EPS && (self.start.y - self.end.y).abs() < EPS {
            return Err("начало и конец участка не могут совпадать".into());
        }
        self.load_diagram.validate()?;
        self.unit_diagram.validate()?;
        Ok(())
    }

    /// Возвращает предупреждения о несогласованности введенной геометрии.
    pub fn geometry_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        let geometric_length = dx.hypot(dy);
        let tolerance = 1.0e-6 * self.length.max(1.0);

        if (geometric_length - self.length).abs() > tolerance {
            warnings.push(format!(
                "участок {}: L={} не совпадает с длиной по координатам {}",
                self.id,
                format_number(self.length),
                format_number(geometric_length)
            ));
        }
        match self.orientation {
            SegmentOrientation::Horizontal if dy.abs() > tolerance => warnings.push(format!(
                "участок {} помечен горизонтальным, но y начала и конца различаются",
                self.id
            )),
            SegmentOrientation::Vertical if dx.abs() > tolerance => warnings.push(format!(
                "участок {} помечен вертикальным, но x начала и конца различаются",
                self.id
            )),
            SegmentOrientation::Inclined if dx.abs() <= tolerance || dy.abs() <= tolerance => {
                warnings.push(format!(
                    "участок {} помечен наклонным, хотя координаты задают осевой стержень",
                    self.id
                ))
            }
            _ => {}
        }
        warnings
    }
}

/// Плоская стержневая рама.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub segments: Vec<Segment>,
}

impl Frame {
    pub fn validate(&self) -> Result<(), String> {
        if self.segments.is_empty() {
            return Err("рама должна содержать хотя бы один участок".into());
        }
        for (index, segment) in self.segments.iter().enumerate() {
            segment
                .validate()
                .map_err(|error| format!("участок {}: {error}", index + 1))?;
            if self.segments[..index]
                .iter()
                .any(|other| other.id == segment.id)
            {
                return Err(format!("ID участка '{}' повторяется", segment.id));
            }
        }
        Ok(())
    }
}

/// Узел рамы. Стержни ссылаются на узлы по индексам в `StructuralModel::nodes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub position: Point,
}

/// Стержень между двумя узлами с грузовой и тремя единичными эпюрами.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rod {
    pub id: String,
    pub start_node: usize,
    pub end_node: usize,
    /// Коэффициент жесткости стержня относительно базового EI.
    #[serde(default = "default_rigidity_factor")]
    pub rigidity_factor: f64,
    /// Высота поперечного сечения h для температурного изгиба.
    #[serde(default = "default_section_height")]
    pub section_height: f64,
    /// Коэффициент материала: α стержня = thermal_expansion_factor · α базы.
    #[serde(default = "default_thermal_expansion_factor")]
    pub thermal_expansion_factor: f64,
    /// Температуры на сторонах стержня. `None` означает отсутствие воздействия.
    #[serde(default)]
    pub temperature: Option<TemperatureLoad>,
    /// Эпюра продольных сил N для отображения и будущего отчета.
    #[serde(default)]
    pub normal_diagram: Diagram,
    /// Эпюра поперечных сил Q для отображения и будущего отчета.
    #[serde(default)]
    pub shear_diagram: Diagram,
    pub load_diagram: Diagram,
    /// Эпюра от единичной горизонтальной силы в выбранном узле.
    pub unit_x_diagram: Diagram,
    /// Эпюра от единичной вертикальной силы в выбранном узле.
    pub unit_y_diagram: Diagram,
    /// Эпюра от единичного положительного момента в выбранном узле.
    #[serde(default)]
    pub unit_rotation_diagram: Diagram,
    /// Единичные эпюры продольной силы для температурного слагаемого.
    #[serde(default)]
    pub unit_normal_x_diagram: Diagram,
    #[serde(default)]
    pub unit_normal_y_diagram: Diagram,
    #[serde(default)]
    pub unit_normal_rotation_diagram: Diagram,
}

/// Постоянные по длине стержня изменения температуры относительно исходного
/// состояния. Первая сторона — верхняя для горизонтального стержня и левая
/// для вертикального; вторая — нижняя/правая.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TemperatureLoad {
    pub upper_or_left: f64,
    pub lower_or_right: f64,
}

impl Default for TemperatureLoad {
    fn default() -> Self {
        Self {
            upper_or_left: 0.0,
            lower_or_right: 0.0,
        }
    }
}

impl TemperatureLoad {
    pub fn difference(self) -> f64 {
        (self.upper_or_left - self.lower_or_right).abs()
    }

    /// Знак температурного изгиба: плюс, когда горячее нижняя/правая сторона.
    pub fn signed_gradient(self) -> f64 {
        self.lower_or_right - self.upper_or_left
    }

    pub fn mean(self) -> f64 {
        (self.upper_or_left + self.lower_or_right) / 2.0
    }
}

/// Узловая модель рамы и точка, перемещения которой определяются.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StructuralModel {
    pub nodes: Vec<Node>,
    pub rods: Vec<Rod>,
    pub target_node: usize,
}

/// Компонента искомого перемещения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplacementDirection {
    Horizontal,
    Vertical,
    Rotation,
}

impl StructuralModel {
    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.is_empty() {
            return Err("модель должна содержать хотя бы одну точку".into());
        }
        if self.rods.is_empty() {
            return Err("модель должна содержать хотя бы один стержень".into());
        }
        if self.target_node >= self.nodes.len() {
            return Err("не выбрана корректная точка для определения перемещений".into());
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.id.trim().is_empty() {
                return Err(format!("точка {}: ID не может быть пустым", index + 1));
            }
            if !node.position.x.is_finite() || !node.position.y.is_finite() {
                return Err(format!(
                    "точка {}: координаты должны быть конечными",
                    node.id
                ));
            }
            if self.nodes[..index].iter().any(|other| other.id == node.id) {
                return Err(format!("ID точки '{}' повторяется", node.id));
            }
        }
        for (index, rod) in self.rods.iter().enumerate() {
            if rod.id.trim().is_empty() {
                return Err(format!("стержень {}: ID не может быть пустым", index + 1));
            }
            if self.rods[..index].iter().any(|other| other.id == rod.id) {
                return Err(format!("ID стержня '{}' повторяется", rod.id));
            }
            if rod.start_node >= self.nodes.len() || rod.end_node >= self.nodes.len() {
                return Err(format!(
                    "стержень {} ссылается на отсутствующую точку",
                    rod.id
                ));
            }
            if rod.start_node == rod.end_node {
                return Err(format!("стержень {} соединяет точку саму с собой", rod.id));
            }
            if self.rod_length(rod)? <= EPS {
                return Err(format!("стержень {} имеет нулевую длину", rod.id));
            }
            if !rod.rigidity_factor.is_finite() || rod.rigidity_factor <= 0.0 {
                return Err(format!(
                    "стержень {}: коэффициент жесткости EI должен быть положительным конечным числом",
                    rod.id
                ));
            }
            if !rod.section_height.is_finite() || rod.section_height <= 0.0 {
                return Err(format!(
                    "стержень {}: высота сечения h должна быть положительным конечным числом",
                    rod.id
                ));
            }
            if !rod.thermal_expansion_factor.is_finite() || rod.thermal_expansion_factor <= 0.0 {
                return Err(format!(
                    "стержень {}: коэффициент α должен быть положительным конечным числом",
                    rod.id
                ));
            }
            if let Some(temperature) = rod.temperature {
                if !temperature.upper_or_left.is_finite() || !temperature.lower_or_right.is_finite()
                {
                    return Err(format!(
                        "стержень {}: температуры должны быть конечными числами",
                        rod.id
                    ));
                }
            }
            rod.load_diagram.validate()?;
            rod.normal_diagram.validate()?;
            rod.shear_diagram.validate()?;
            rod.unit_x_diagram.validate()?;
            rod.unit_y_diagram.validate()?;
            rod.unit_rotation_diagram.validate()?;
            rod.unit_normal_x_diagram.validate()?;
            rod.unit_normal_y_diagram.validate()?;
            rod.unit_normal_rotation_diagram.validate()?;
        }
        Ok(())
    }

    pub fn rod_length(&self, rod: &Rod) -> Result<f64, String> {
        let start = self
            .nodes
            .get(rod.start_node)
            .ok_or_else(|| format!("стержень {}: нет начальной точки", rod.id))?
            .position;
        let end = self
            .nodes
            .get(rod.end_node)
            .ok_or_else(|| format!("стержень {}: нет конечной точки", rod.id))?
            .position;
        Ok((end.x - start.x).hypot(end.y - start.y))
    }

    pub fn rod_orientation(&self, rod: &Rod) -> Result<SegmentOrientation, String> {
        let start = self
            .nodes
            .get(rod.start_node)
            .ok_or_else(|| format!("стержень {}: нет начальной точки", rod.id))?
            .position;
        let end = self
            .nodes
            .get(rod.end_node)
            .ok_or_else(|| format!("стержень {}: нет конечной точки", rod.id))?
            .position;
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let tolerance = 1.0e-9 * dx.abs().max(dy.abs()).max(1.0);
        Ok(if dy.abs() <= tolerance {
            SegmentOrientation::Horizontal
        } else if dx.abs() <= tolerance {
            SegmentOrientation::Vertical
        } else {
            SegmentOrientation::Inclined
        })
    }

    /// Создает расчетную раму для выбранной компоненты перемещения.
    pub fn frame_for(&self, direction: DisplacementDirection) -> Result<Frame, String> {
        self.validate()?;
        let mut segments = Vec::with_capacity(self.rods.len());
        for rod in &self.rods {
            let start = self.nodes[rod.start_node].position;
            let end = self.nodes[rod.end_node].position;
            let unit_diagram = match direction {
                DisplacementDirection::Horizontal => rod.unit_x_diagram.clone(),
                DisplacementDirection::Vertical => rod.unit_y_diagram.clone(),
                DisplacementDirection::Rotation => rod.unit_rotation_diagram.clone(),
            };
            segments.push(Segment {
                id: rod.id.clone(),
                length: self.rod_length(rod)?,
                orientation: self.rod_orientation(rod)?,
                start,
                end,
                rigidity_factor: rod.rigidity_factor,
                load_diagram: rod.load_diagram.clone(),
                unit_diagram,
            });
        }
        Ok(Frame { segments })
    }
}

/// Жесткость одинакова для всех участков рамы.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Rigidity {
    Numeric(f64),
    Symbolic,
}

/// Коэффициент линейного температурного расширения α.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum ThermalExpansion {
    Numeric(f64),
    #[default]
    Symbolic,
}

impl ThermalExpansion {
    pub fn numeric(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value <= 0.0 {
            return Err("α должна быть положительным конечным числом".into());
        }
        Ok(Self::Numeric(value))
    }
}

/// Какие воздействия входят в итоговое перемещение и отчёт.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectsSelection {
    #[serde(default = "default_true")]
    pub force: bool,
    #[serde(default)]
    pub thermal: bool,
    #[serde(default)]
    pub kinematic: bool,
}

fn default_true() -> bool {
    true
}

impl Default for EffectsSelection {
    fn default() -> Self {
        Self {
            force: true,
            thermal: false,
            kinematic: false,
        }
    }
}

impl Rigidity {
    pub fn numeric(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value <= 0.0 {
            return Err("EI должна быть положительным конечным числом".into());
        }
        Ok(Self::Numeric(value))
    }
}

/// Способ интегрирования вклада участка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationMethod {
    TrapezoidalProduct,
    SimpsonThreePoint,
}

impl fmt::Display for IntegrationMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrapezoidalProduct => write!(f, "формула перемножения трапеций"),
            Self::SimpsonThreePoint => write!(f, "Симпсон по 3 точкам"),
        }
    }
}

/// Вклад одного участка в числитель интеграла Мора.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentContribution {
    pub segment_id: String,
    /// Интеграл эпюр до учета различия жесткостей участков.
    pub raw_integral: f64,
    /// Коэффициент k в выражении EI участка = k * EI базы.
    pub rigidity_factor: f64,
    /// Эквивалентный вклад в числитель: raw_integral / rigidity_factor.
    pub integral: f64,
    pub method: IntegrationMethod,
    pub load_values: [f64; 3],
    pub unit_values: [f64; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalculationResult {
    pub contributions: Vec<SegmentContribution>,
    /// Сумма интегралов ∫Mp·m1 dx до деления на EI.
    pub numerator: f64,
    pub rigidity: Rigidity,
}

/// Два линейных перемещения и поворот выбранной точки.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplacementComponents {
    pub horizontal: CalculationResult,
    pub vertical: CalculationResult,
    pub rotation: CalculationResult,
}

/// Вклад одного стержня в температурное перемещение до умножения на α.
#[derive(Debug, Clone, PartialEq)]
pub struct ThermalRodContribution {
    pub rod_id: String,
    pub active: bool,
    pub length: f64,
    pub section_height: f64,
    pub expansion_factor: f64,
    pub upper_or_left: f64,
    pub lower_or_right: f64,
    pub temperature_difference: f64,
    pub signed_gradient: f64,
    pub mean_temperature: f64,
    pub unit_moment_area: f64,
    pub unit_normal_area: f64,
    pub bending_term: f64,
    pub axial_term: f64,
    pub coefficient: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermalCalculationResult {
    pub contributions: Vec<ThermalRodContribution>,
    /// Коэффициент при α либо величина до умножения на числовое α.
    pub coefficient: f64,
    pub expansion: ThermalExpansion,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermalDisplacementComponents {
    pub horizontal: ThermalCalculationResult,
    pub vertical: ThermalCalculationResult,
    pub rotation: ThermalCalculationResult,
}

impl ThermalCalculationResult {
    pub fn numeric_displacement(&self) -> Option<f64> {
        match self.expansion {
            ThermalExpansion::Numeric(alpha) => Some(alpha * self.coefficient),
            ThermalExpansion::Symbolic => None,
        }
    }

    pub fn formatted_displacement(&self) -> String {
        match self.numeric_displacement() {
            Some(value) => format_number(value),
            None if self.coefficient.abs() <= EPS => "0".into(),
            None => format!("α · {}", format_number(self.coefficient)),
        }
    }
}

impl CalculationResult {
    pub fn numeric_displacement(&self) -> Option<f64> {
        match self.rigidity {
            Rigidity::Numeric(ei) => Some(self.numerator / ei),
            Rigidity::Symbolic => None,
        }
    }

    pub fn formatted_displacement(&self) -> String {
        match self.numeric_displacement() {
            Some(value) => format_number(value),
            None => format_symbolic_displacement(self.numerator),
        }
    }

    /// Десятичное представление результата для одновременного показа с дробью.
    pub fn formatted_decimal_displacement(&self) -> String {
        match self.numeric_displacement() {
            Some(value) => format_number(value),
            None => format!("{} / EI", format_number(self.numerator)),
        }
    }
}

/// Форматирует аналитический результат. Дроби, естественно возникающие из
/// формул трапеций и Симпсона, сохраняются в шестых вместо десятичной записи.
fn format_symbolic_displacement(value: f64) -> String {
    if let Some(integer) = nearly_integer(value) {
        return format!("{integer} / EI");
    }
    if let Some(sixths) = nearly_integer(value * 6.0) {
        return format!("{sixths} / 6EI");
    }
    format!("{} / EI", format_number(value))
}

fn nearly_integer(value: f64) -> Option<String> {
    let rounded = value.round();
    let tolerance = 1.0e-9 * value.abs().max(1.0);
    ((value - rounded).abs() <= tolerance).then(|| format!("{rounded:.0}"))
}

/// Вычисляет перемещение как сумму ∫Mp·m1/(EI) dx по всем участкам.
pub fn calculate_displacement(
    frame: &Frame,
    rigidity: Rigidity,
) -> Result<CalculationResult, String> {
    frame.validate()?;
    if let Rigidity::Numeric(value) = rigidity {
        Rigidity::numeric(value)?;
    }

    let mut contributions = Vec::with_capacity(frame.segments.len());
    for segment in &frame.segments {
        let mp_left = segment.load_diagram.left_value();
        let mp_mid = segment.load_diagram.midpoint_value(segment.length);
        let mp_right = segment.load_diagram.right_value();
        let m1_left = segment.unit_diagram.left_value();
        let m1_mid = segment.unit_diagram.midpoint_value(segment.length);
        let m1_right = segment.unit_diagram.right_value();

        let has_parabola = segment.load_diagram.kind == DiagramKind::Parabolic
            || segment.unit_diagram.kind == DiagramKind::Parabolic;
        let (raw_integral, method) = if has_parabola {
            (
                segment.length / 6.0
                    * (mp_left * m1_left + 4.0 * mp_mid * m1_mid + mp_right * m1_right),
                IntegrationMethod::SimpsonThreePoint,
            )
        } else {
            (
                segment.length / 6.0
                    * (mp_left * (2.0 * m1_left + m1_right)
                        + mp_right * (m1_left + 2.0 * m1_right)),
                IntegrationMethod::TrapezoidalProduct,
            )
        };
        let integral = raw_integral / segment.rigidity_factor;

        contributions.push(SegmentContribution {
            segment_id: segment.id.clone(),
            raw_integral,
            rigidity_factor: segment.rigidity_factor,
            integral,
            method,
            load_values: [mp_left, mp_mid, mp_right],
            unit_values: [m1_left, m1_mid, m1_right],
        });
    }

    let numerator = contributions.iter().map(|item| item.integral).sum();
    Ok(CalculationResult {
        contributions,
        numerator,
        rigidity,
    })
}

/// Вычисляет горизонтальное, вертикальное перемещения и поворот выбранной точки.
pub fn calculate_displacement_components(
    model: &StructuralModel,
    rigidity: Rigidity,
) -> Result<DisplacementComponents, String> {
    let horizontal_frame = model.frame_for(DisplacementDirection::Horizontal)?;
    let vertical_frame = model.frame_for(DisplacementDirection::Vertical)?;
    let rotation_frame = model.frame_for(DisplacementDirection::Rotation)?;
    Ok(DisplacementComponents {
        horizontal: calculate_displacement(&horizontal_frame, rigidity)?,
        vertical: calculate_displacement(&vertical_frame, rigidity)?,
        rotation: calculate_displacement(&rotation_frame, rigidity)?,
    })
}

/// Вычисляет температурные составляющие перемещения по формуле
/// α·Σkα·[∫(t′/h)·m ds + ∫t₀·n ds]. Температура постоянна по длине стержня,
/// поэтому требуются точные площади единичных эпюр m и n.
pub fn calculate_thermal_displacement_components(
    model: &StructuralModel,
    expansion: ThermalExpansion,
) -> Result<ThermalDisplacementComponents, String> {
    model.validate()?;
    if let ThermalExpansion::Numeric(value) = expansion {
        ThermalExpansion::numeric(value)?;
    }
    Ok(ThermalDisplacementComponents {
        horizontal: calculate_thermal_direction(
            model,
            expansion,
            DisplacementDirection::Horizontal,
        )?,
        vertical: calculate_thermal_direction(model, expansion, DisplacementDirection::Vertical)?,
        rotation: calculate_thermal_direction(model, expansion, DisplacementDirection::Rotation)?,
    })
}

fn calculate_thermal_direction(
    model: &StructuralModel,
    expansion: ThermalExpansion,
    direction: DisplacementDirection,
) -> Result<ThermalCalculationResult, String> {
    let mut contributions = Vec::with_capacity(model.rods.len());
    for rod in &model.rods {
        let length = model.rod_length(rod)?;
        let moment = match direction {
            DisplacementDirection::Horizontal => &rod.unit_x_diagram,
            DisplacementDirection::Vertical => &rod.unit_y_diagram,
            DisplacementDirection::Rotation => &rod.unit_rotation_diagram,
        };
        let normal = match direction {
            DisplacementDirection::Horizontal => &rod.unit_normal_x_diagram,
            DisplacementDirection::Vertical => &rod.unit_normal_y_diagram,
            DisplacementDirection::Rotation => &rod.unit_normal_rotation_diagram,
        };
        let active = rod.temperature.is_some();
        let temperature = rod.temperature.unwrap_or_default();
        let difference = temperature.difference();
        let signed_gradient = temperature.signed_gradient();
        let mean = temperature.mean();
        let moment_area = moment.signed_area(length);
        let normal_area = normal.signed_area(length);
        let bending_term = if active {
            signed_gradient / rod.section_height * moment_area
        } else {
            0.0
        };
        let axial_term = if active { mean * normal_area } else { 0.0 };
        contributions.push(ThermalRodContribution {
            rod_id: rod.id.clone(),
            active,
            length,
            section_height: rod.section_height,
            expansion_factor: rod.thermal_expansion_factor,
            upper_or_left: temperature.upper_or_left,
            lower_or_right: temperature.lower_or_right,
            temperature_difference: difference,
            signed_gradient,
            mean_temperature: mean,
            unit_moment_area: moment_area,
            unit_normal_area: normal_area,
            bending_term,
            axial_term,
            coefficient: rod.thermal_expansion_factor * (bending_term + axial_term),
        });
    }
    let coefficient = contributions.iter().map(|item| item.coefficient).sum();
    Ok(ThermalCalculationResult {
        contributions,
        coefficient,
        expansion,
    })
}

/// Текстовая схема рамы, пригодная как временная визуализация до появления GUI.
pub fn draw_frame_ascii(frame: &Frame) -> String {
    let mut output = String::new();
    output.push_str("\nСХЕМА РАМЫ И ЭПЮР (ординаты: лево / середина / право)\n");
    output.push_str("Правило знаков: верх/лево = −, низ/право = +\n");

    for segment in &frame.segments {
        let symbol = match segment.orientation {
            SegmentOrientation::Horizontal => "────────",
            SegmentOrientation::Vertical => "││││││││",
            SegmentOrientation::Inclined => "╱╱╱╱╱╱╱╱",
        };
        let mp = [
            segment.load_diagram.left_value(),
            segment.load_diagram.midpoint_value(segment.length),
            segment.load_diagram.right_value(),
        ];
        let m1 = [
            segment.unit_diagram.left_value(),
            segment.unit_diagram.midpoint_value(segment.length),
            segment.unit_diagram.right_value(),
        ];
        output.push_str(&format!(
            "\n[{}] ({}, {}) {} ({}, {}), L={}, EI={}*EI_base\n",
            segment.id,
            format_number(segment.start.x),
            format_number(segment.start.y),
            symbol,
            format_number(segment.end.x),
            format_number(segment.end.y),
            format_number(segment.length),
            format_number(segment.rigidity_factor)
        ));
        output.push_str(&format!(
            "  Mp [{}]: {} / {} / {}{}\n",
            segment.load_diagram.kind,
            format_number(mp[0]),
            format_number(mp[1]),
            format_number(mp[2]),
            extremum_mark(&segment.load_diagram)
        ));
        output.push_str(&format!(
            "  m1 [{}]: {} / {} / {}{}\n",
            segment.unit_diagram.kind,
            format_number(m1[0]),
            format_number(m1[1]),
            format_number(m1[2]),
            extremum_mark(&segment.unit_diagram)
        ));
    }
    output
}

fn extremum_mark(diagram: &Diagram) -> &'static str {
    if diagram.kind == DiagramKind::Parabolic && diagram.has_extremum {
        "; внутренний экстремум: да"
    } else {
        ""
    }
}

/// Компактное представление числа без незначащих нулей.
pub fn format_number(value: f64) -> String {
    let normalized = if value.abs() < 5.0e-13 { 0.0 } else { value };
    let text = format!("{normalized:.10}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinate(value: f64) -> Ordinate {
        Ordinate::from_signed(value).unwrap()
    }

    fn linear(left: f64, right: f64) -> Diagram {
        Diagram::new(
            DiagramKind::Linear,
            ordinate(left),
            ordinate(right),
            None,
            None,
            false,
        )
        .unwrap()
    }

    fn parabolic(left: f64, right: f64, q: f64, fiber: FiberSide) -> Diagram {
        Diagram::new(
            DiagramKind::Parabolic,
            ordinate(left),
            ordinate(right),
            Some(q),
            Some(fiber),
            true,
        )
        .unwrap()
    }

    fn segment(load_diagram: Diagram, unit_diagram: Diagram) -> Segment {
        Segment {
            id: "S1".into(),
            length: 2.0,
            orientation: SegmentOrientation::Horizontal,
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 2.0, y: 0.0 },
            rigidity_factor: 1.0,
            load_diagram,
            unit_diagram,
        }
    }

    #[test]
    fn fiber_side_defines_ordinate_sign() {
        assert_eq!(ordinate(-12.0).signed_value(), -12.0);
        assert_eq!(ordinate(7.0).signed_value(), 7.0);
        assert_eq!(ordinate(0.0).signed_value(), 0.0);
    }

    #[test]
    fn linear_diagrams_use_required_formula() {
        let frame = Frame {
            segments: vec![segment(linear(2.0, 4.0), linear(3.0, 5.0))],
        };
        let result = calculate_displacement(&frame, Rigidity::Symbolic).unwrap();
        let expected = 2.0 / 6.0 * (2.0 * (2.0 * 3.0 + 5.0) + 4.0 * (3.0 + 2.0 * 5.0));
        assert!((result.numerator - expected).abs() < EPS);
        assert_eq!(
            result.contributions[0].method,
            IntegrationMethod::TrapezoidalProduct
        );
    }

    #[test]
    fn parabolic_diagram_uses_signed_q_and_simpson() {
        let load = parabolic(0.0, 0.0, 8.0, FiberSide::UpperOrLeft);
        assert!((load.midpoint_value(2.0) + 4.0).abs() < EPS);
        let frame = Frame {
            segments: vec![segment(load, linear(1.0, 1.0))],
        };
        let result = calculate_displacement(&frame, Rigidity::Symbolic).unwrap();
        let expected = 2.0 / 6.0 * (0.0 + 4.0 * -4.0 * 1.0 + 0.0);
        assert!((result.numerator - expected).abs() < EPS);
        assert_eq!(
            result.contributions[0].method,
            IntegrationMethod::SimpsonThreePoint
        );
    }

    #[test]
    fn numeric_ei_is_applied_after_sum() {
        let frame = Frame {
            segments: vec![segment(linear(2.0, 2.0), linear(3.0, 3.0))],
        };
        let result = calculate_displacement(&frame, Rigidity::numeric(4.0).unwrap()).unwrap();
        assert!((result.numeric_displacement().unwrap() - 3.0).abs() < EPS);
        assert_eq!(result.formatted_displacement(), "3");
    }

    #[test]
    fn symbolic_ei_has_requested_format() {
        let frame = Frame {
            segments: vec![segment(linear(-2.0, -2.0), linear(1.0, 1.0))],
        };
        let result = calculate_displacement(&frame, Rigidity::Symbolic).unwrap();
        assert_eq!(result.formatted_displacement(), "-4 / EI");
    }

    #[test]
    fn symbolic_sixths_are_not_converted_to_a_repeating_decimal() {
        let result = CalculationResult {
            contributions: Vec::new(),
            numerator: 310.0 / 6.0,
            rigidity: Rigidity::Symbolic,
        };
        assert_eq!(result.formatted_displacement(), "310 / 6EI");
        assert_eq!(
            result.formatted_decimal_displacement(),
            "51.6666666667 / EI"
        );
    }

    #[test]
    fn constant_diagram_rejects_different_ends() {
        let result = Diagram::new(
            DiagramKind::Constant,
            ordinate(1.0),
            ordinate(2.0),
            None,
            None,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn node_model_derives_length_and_three_components() {
        let model = StructuralModel {
            nodes: vec![
                Node {
                    id: "A".into(),
                    position: Point { x: 0.0, y: 0.0 },
                },
                Node {
                    id: "B".into(),
                    position: Point { x: 2.0, y: 0.0 },
                },
            ],
            rods: vec![Rod {
                id: "AB".into(),
                start_node: 0,
                end_node: 1,
                rigidity_factor: 1.0,
                section_height: 0.5,
                thermal_expansion_factor: 1.0,
                temperature: None,
                normal_diagram: Diagram::default(),
                shear_diagram: Diagram::default(),
                load_diagram: linear(2.0, 2.0),
                unit_x_diagram: linear(3.0, 3.0),
                unit_y_diagram: linear(-1.0, -1.0),
                unit_rotation_diagram: linear(0.5, 0.5),
                unit_normal_x_diagram: Diagram::default(),
                unit_normal_y_diagram: Diagram::default(),
                unit_normal_rotation_diagram: Diagram::default(),
            }],
            target_node: 1,
        };
        assert!((model.rod_length(&model.rods[0]).unwrap() - 2.0).abs() < EPS);
        let result = calculate_displacement_components(&model, Rigidity::Symbolic).unwrap();
        assert!((result.horizontal.numerator - 12.0).abs() < EPS);
        assert!((result.vertical.numerator + 4.0).abs() < EPS);
        assert!((result.rotation.numerator - 2.0).abs() < EPS);
    }

    #[test]
    fn different_segment_rigidities_are_applied_before_summing() {
        let mut first = segment(linear(2.0, 2.0), linear(3.0, 3.0));
        first.id = "EI".into();
        let mut second = first.clone();
        second.id = "2EI".into();
        second.rigidity_factor = 2.0;
        let frame = Frame {
            segments: vec![first, second],
        };

        let result = calculate_displacement(&frame, Rigidity::Symbolic).unwrap();
        assert!((result.contributions[0].raw_integral - 12.0).abs() < EPS);
        assert!((result.contributions[0].integral - 12.0).abs() < EPS);
        assert!((result.contributions[1].raw_integral - 12.0).abs() < EPS);
        assert!((result.contributions[1].integral - 6.0).abs() < EPS);
        assert!((result.numerator - 18.0).abs() < EPS);
        assert_eq!(result.formatted_displacement(), "18 / EI");
    }

    #[test]
    fn parabolic_area_is_integrated_analytically() {
        let diagram = Diagram::new(
            DiagramKind::Parabolic,
            Ordinate::from_signed(0.0).unwrap(),
            Ordinate::from_signed(0.0).unwrap(),
            Some(8.0),
            Some(FiberSide::LowerOrRight),
            true,
        )
        .unwrap();
        assert!((diagram.signed_area(2.0) - 16.0 / 3.0).abs() < EPS);
    }

    #[test]
    fn thermal_displacement_contains_bending_and_axial_terms() {
        let mut model = StructuralModel {
            nodes: vec![
                Node {
                    id: "A".into(),
                    position: Point { x: 0.0, y: 0.0 },
                },
                Node {
                    id: "B".into(),
                    position: Point { x: 2.0, y: 0.0 },
                },
            ],
            rods: vec![Rod {
                id: "AB".into(),
                start_node: 0,
                end_node: 1,
                rigidity_factor: 1.0,
                section_height: 0.5,
                thermal_expansion_factor: 2.0,
                temperature: Some(TemperatureLoad {
                    upper_or_left: 10.0,
                    lower_or_right: 30.0,
                }),
                normal_diagram: Diagram::default(),
                shear_diagram: Diagram::default(),
                load_diagram: Diagram::default(),
                unit_x_diagram: linear(2.0, 2.0),
                unit_y_diagram: Diagram::default(),
                unit_rotation_diagram: Diagram::default(),
                unit_normal_x_diagram: linear(3.0, 3.0),
                unit_normal_y_diagram: Diagram::default(),
                unit_normal_rotation_diagram: Diagram::default(),
            }],
            target_node: 1,
        };
        let result =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Symbolic).unwrap();
        assert!((result.horizontal.contributions[0].bending_term - 160.0).abs() < EPS);
        assert!((result.horizontal.contributions[0].axial_term - 120.0).abs() < EPS);
        assert!((result.horizontal.coefficient - 560.0).abs() < EPS);
        assert_eq!(result.horizontal.formatted_displacement(), "α · 560");

        model.rods[0].temperature = None;
        let disabled =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Numeric(0.000012))
                .unwrap();
        assert!(disabled.horizontal.coefficient.abs() < EPS);
        assert_eq!(disabled.horizontal.formatted_displacement(), "0");
    }
}

//! Расчет перемещений плоской стержневой рамы методом Мора.
//!
//! Знак момента задается стороной растянутых волокон:
//! верхние/левые волокна имеют знак «минус», нижние/правые — «плюс».

use std::fmt;

use serde::{Deserialize, Serialize};

pub mod fem;

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

/// Значение строки справочника: коэффициент перед символом либо полное число.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PropertyValue {
    pub numeric: bool,
    pub symbolic_factor: f64,
    pub mantissa: f64,
    pub exponent: i32,
}

impl Default for PropertyValue {
    fn default() -> Self {
        Self {
            numeric: false,
            symbolic_factor: 1.0,
            mantissa: 1.0,
            exponent: 0,
        }
    }
}

impl PropertyValue {
    pub fn from_number(value: f64) -> Self {
        if !value.is_finite() || value <= 0.0 {
            return Self {
                numeric: true,
                mantissa: value,
                ..Self::default()
            };
        }
        let exponent = value.log10().floor() as i32;
        let scale = 10_f64.powi(exponent);
        let (mantissa, exponent) = if scale.is_finite() && scale > 0.0 {
            (value / scale, exponent)
        } else {
            (value, 0)
        };
        Self {
            numeric: true,
            mantissa,
            exponent,
            ..Self::default()
        }
    }

    pub fn numeric_value(self) -> Option<f64> {
        let value = self.mantissa * 10_f64.powi(self.exponent);
        (value.is_finite() && value > 0.0).then_some(value)
    }

    pub fn validate(self) -> Result<(), String> {
        if !self.numeric && (!self.symbolic_factor.is_finite() || self.symbolic_factor <= 0.0) {
            return Err("аналитический коэффициент должен быть положительным числом".into());
        }
        if self.numeric && self.numeric_value().is_none() {
            return Err("числовое значение должно быть положительным конечным числом".into());
        }
        Ok(())
    }

    pub fn formatted(self, symbol: &str) -> String {
        if self.numeric {
            format!("{} × 10^({})", format_number(self.mantissa), self.exponent)
        } else {
            format!("{} · {symbol}", format_number(self.symbolic_factor))
        }
    }
}

/// Идентификатор не меняется при переименовании строки справочника.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyPreset {
    pub id: u32,
    pub name: String,
    pub value: PropertyValue,
}

fn preset_id_for_value(items: &mut Vec<PropertyPreset>, value: PropertyValue, prefix: &str) -> u32 {
    if let Some(existing) = items.iter().find(|item| item.value == value) {
        return existing.id;
    }
    let id = items.iter().map(|item| item.id).max().unwrap_or(0) + 1;
    items.push(PropertyPreset {
        id,
        name: format!("{prefix} {id}"),
        value,
    });
    id
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

/// Обозначение точки в порядке Excel: A…Z, AA…AZ, BA…
pub fn alphabetical_node_id(mut index: usize) -> String {
    let mut label = String::new();
    loop {
        label.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    label
}

pub fn next_node_id(nodes: &[Node]) -> String {
    (0..)
        .map(alphabetical_node_id)
        .find(|candidate| {
            nodes
                .iter()
                .all(|node| !node.id.trim().eq_ignore_ascii_case(candidate))
        })
        .expect("в последовательности имён всегда найдётся свободное")
}

/// Имя по направлению стержня. Дефис устраняет неоднозначность длинных имён.
pub fn rod_id_from_nodes(nodes: &[Node], start: usize, end: usize) -> Option<String> {
    let start = nodes.get(start)?.id.trim();
    let end = nodes.get(end)?.id.trim();
    if start.is_empty() || end.is_empty() {
        return None;
    }
    let separator = if start.chars().count() == 1 && end.chars().count() == 1 {
        ""
    } else {
        "-"
    };
    Some(format!("{start}{separator}{end}"))
}

fn same_position(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS
}

/// Стержень между двумя узлами с грузовой и тремя единичными эпюрами.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rod {
    pub id: String,
    pub start_node: usize,
    pub end_node: usize,
    /// Освобождение изгибающего момента на начальном конце стержня.
    #[serde(default)]
    pub release_start_moment: bool,
    /// Освобождение изгибающего момента на конечном конце стержня.
    #[serde(default)]
    pub release_end_moment: bool,
    #[serde(default)]
    pub rigidity_preset_id: Option<u32>,
    #[serde(default)]
    pub thermal_preset_id: Option<u32>,
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

impl Rod {
    /// Новый стержень без заданных эпюр и температурного воздействия.
    pub fn blank(id: String, start_node: usize, end_node: usize) -> Self {
        Self {
            id,
            start_node,
            end_node,
            release_start_moment: false,
            release_end_moment: false,
            rigidity_preset_id: None,
            thermal_preset_id: None,
            rigidity_factor: 1.0,
            section_height: 0.5,
            thermal_expansion_factor: 1.0,
            temperature: None,
            normal_diagram: Diagram::default(),
            shear_diagram: Diagram::default(),
            load_diagram: Diagram::default(),
            unit_x_diagram: Diagram::default(),
            unit_y_diagram: Diagram::default(),
            unit_rotation_diagram: Diagram::default(),
            unit_normal_x_diagram: Diagram::default(),
            unit_normal_y_diagram: Diagram::default(),
            unit_normal_rotation_diagram: Diagram::default(),
        }
    }
}

/// Постоянные по длине стержня изменения температуры относительно исходного
/// состояния. Первая сторона — верхняя для горизонтального стержня и левая
/// для вертикального; вторая — нижняя/правая.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TemperatureLoad {
    pub upper_or_left: f64,
    pub lower_or_right: f64,
}

/// Закреплённое направление опоры и заданное перемещение связи.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportDirection {
    Horizontal,
    Vertical,
    Rotation,
    Normal,
}

impl SupportDirection {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Horizontal => "X",
            Self::Vertical => "Y",
            Self::Rotation => "φ",
            Self::Normal => "N",
        }
    }
}

/// Реакции берутся из трёх соответствующих единичных систем.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupportMotion {
    pub node: usize,
    pub direction: SupportDirection,
    /// Угол нормали для наклонной подвижной или скользящей опоры.
    #[serde(default = "default_support_normal_deg")]
    pub normal_deg: f64,
    pub displacement: f64,
    pub unit_x_reaction: f64,
    pub unit_y_reaction: f64,
    pub unit_rotation_reaction: f64,
}

pub fn default_support_normal_deg() -> f64 {
    90.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct KinematicContribution {
    pub node_id: String,
    pub direction: SupportDirection,
    pub normal_deg: f64,
    pub displacement: f64,
    pub unit_reaction: f64,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KinematicResult {
    pub contributions: Vec<KinematicContribution>,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KinematicDisplacementComponents {
    pub horizontal: KinematicResult,
    pub vertical: KinematicResult,
    pub rotation: KinematicResult,
}

/// Для каждой опорной связи вычисляет −Rᵢ·cᵢ, сохраняя и нулевые слагаемые.
pub fn calculate_kinematic_displacement_components(
    model: &StructuralModel,
) -> Result<KinematicDisplacementComponents, String> {
    model.validate()?;
    let calculate = |select: fn(&SupportMotion) -> f64| {
        let contributions = model
            .supports
            .iter()
            .map(|support| {
                let reaction = select(support);
                KinematicContribution {
                    node_id: model.nodes[support.node].id.clone(),
                    direction: support.direction,
                    normal_deg: support.normal_deg,
                    displacement: support.displacement,
                    unit_reaction: reaction,
                    value: -reaction * support.displacement,
                }
            })
            .collect::<Vec<_>>();
        let total = contributions.iter().map(|item| item.value).sum();
        KinematicResult {
            contributions,
            total,
        }
    };
    Ok(KinematicDisplacementComponents {
        horizontal: calculate(|s| s.unit_x_reaction),
        vertical: calculate(|s| s.unit_y_reaction),
        rotation: calculate(|s| s.unit_rotation_reaction),
    })
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
    /// Модуль перепада температур между сторонами сечения.
    pub fn difference(self) -> f64 {
        (self.upper_or_left - self.lower_or_right).abs()
    }

    /// Сторона наиболее нагретых волокон. При равных температурах
    /// температурного изгиба нет, поэтому сторона не определяется.
    pub fn hotter_side(self) -> Option<FiberSide> {
        if self.upper_or_left > self.lower_or_right {
            Some(FiberSide::UpperOrLeft)
        } else if self.lower_or_right > self.upper_or_left {
            Some(FiberSide::LowerOrRight)
        } else {
            None
        }
    }

    /// Знак стороны наиболее нагретых волокон по принятому правилу эпюр:
    /// верх/лево = −1, низ/право = +1.
    pub fn hotter_side_sign(self) -> f64 {
        self.hotter_side().map_or(0.0, FiberSide::sign)
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
    #[serde(default)]
    pub rigidity_presets: Vec<PropertyPreset>,
    #[serde(default)]
    pub thermal_presets: Vec<PropertyPreset>,
    #[serde(default)]
    pub supports: Vec<SupportMotion>,
}

/// Компонента искомого перемещения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplacementDirection {
    Horizontal,
    Vertical,
    Rotation,
}

impl StructuralModel {
    /// Узел внутреннего шарнира: один из двух примыкающих стержней
    /// не передаёт изгибающий момент.
    pub fn is_internal_hinge_node(&self, node: usize) -> bool {
        self.rods.iter().any(|rod| {
            (rod.start_node == node && rod.release_start_moment)
                || (rod.end_node == node && rod.release_end_moment)
        })
    }

    pub fn validate_hinges(&self) -> Result<(), String> {
        for (index, node) in self.nodes.iter().enumerate() {
            let incident = self
                .rods
                .iter()
                .filter(|rod| rod.start_node == index || rod.end_node == index)
                .count();
            if !self.is_internal_hinge_node(index) {
                continue;
            }
            let releases = self
                .rods
                .iter()
                .filter(|rod| {
                    (rod.start_node == index && rod.release_start_moment)
                        || (rod.end_node == index && rod.release_end_moment)
                })
                .count();
            if incident != 2 || releases != 1 {
                return Err(format!(
                    "в точке {} внутренний шарнир требует ровно два стержня и освобождение момента только у одного из них",
                    node.id
                ));
            }
            if self.supports.iter().any(|support| {
                support.node == index && support.direction == SupportDirection::Rotation
            }) {
                return Err(format!(
                    "в точке {} нельзя одновременно задать внутренний шарнир и опору по повороту",
                    node.id
                ));
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_for_loading()?;
        self.validate_unique_geometry()
    }

    /// Проверка новых ограничений независимо от полноты черновика таблицы.
    pub fn validate_unique_geometry(&self) -> Result<(), String> {
        for (index, node) in self.nodes.iter().enumerate() {
            for earlier in &self.nodes[..index] {
                if node.id.trim().to_lowercase() == earlier.id.trim().to_lowercase() {
                    return Err(format!(
                        "названия точек '{}' и '{}' совпадают",
                        earlier.id, node.id
                    ));
                }
                if same_position(node.position, earlier.position) {
                    return Err(format!(
                        "точки {} и {} имеют одинаковые координаты",
                        earlier.id, node.id
                    ));
                }
            }
        }
        for (index, rod) in self.rods.iter().enumerate() {
            for earlier in &self.rods[..index] {
                if rod.id.trim().to_lowercase() == earlier.id.trim().to_lowercase() {
                    return Err(format!(
                        "названия стержней '{}' и '{}' совпадают",
                        earlier.id, rod.id
                    ));
                }
                let Some((a, b)) = self
                    .nodes
                    .get(rod.start_node)
                    .zip(self.nodes.get(rod.end_node))
                else {
                    continue;
                };
                let Some((c, d)) = self
                    .nodes
                    .get(earlier.start_node)
                    .zip(self.nodes.get(earlier.end_node))
                else {
                    continue;
                };
                if (same_position(a.position, c.position) && same_position(b.position, d.position))
                    || (same_position(a.position, d.position)
                        && same_position(b.position, c.position))
                {
                    return Err(format!(
                        "стержни {} и {} совпадают по концам",
                        earlier.id, rod.id
                    ));
                }
            }
        }
        Ok(())
    }

    /// Проверки старого формата, без новых ограничений геометрии и написания имён.
    /// Позволяет открыть проект с конфликтом для его исправления.
    pub fn validate_for_loading(&self) -> Result<(), String> {
        if self.nodes.is_empty() {
            return Err("модель должна содержать хотя бы одну точку".into());
        }
        if self.rods.is_empty() {
            return Err("модель должна содержать хотя бы один стержень".into());
        }
        if self.target_node >= self.nodes.len() {
            return Err("не выбрана корректная точка для определения перемещений".into());
        }
        for (kind, presets) in [
            ("жёсткости", &self.rigidity_presets),
            ("коэффициента α", &self.thermal_presets),
        ] {
            for (index, preset) in presets.iter().enumerate() {
                if preset.name.trim().is_empty() {
                    return Err(format!("строка {kind} {}: задайте название", index + 1));
                }
                if presets[..index].iter().any(|previous| {
                    previous.id == preset.id
                        || previous
                            .name
                            .trim()
                            .eq_ignore_ascii_case(preset.name.trim())
                }) {
                    return Err(format!(
                        "строка {kind} '{}': повторяется ID или название",
                        preset.name
                    ));
                }
                preset
                    .value
                    .validate()
                    .map_err(|error| format!("строка {kind} '{}': {error}", preset.name))?;
            }
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
        for (index, support) in self.supports.iter().enumerate() {
            if support.node >= self.nodes.len() {
                return Err(format!(
                    "опора {} ссылается на отсутствующую точку",
                    index + 1
                ));
            }
            if self.supports[..index]
                .iter()
                .any(|other| other.node == support.node && other.direction == support.direction)
            {
                return Err(format!(
                    "для точки {} направление {} задано дважды",
                    self.nodes[support.node].id,
                    support.direction.symbol()
                ));
            }
            if [
                support.normal_deg,
                support.displacement,
                support.unit_x_reaction,
                support.unit_y_reaction,
                support.unit_rotation_reaction,
            ]
            .iter()
            .any(|value| !value.is_finite())
            {
                return Err(format!(
                    "опора {}: значения должны быть конечными",
                    index + 1
                ));
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
            if rod
                .rigidity_preset_id
                .is_some_and(|id| !self.rigidity_presets.iter().any(|item| item.id == id))
            {
                return Err(format!(
                    "стержень {}: выбранная жёсткость не найдена",
                    rod.id
                ));
            }
            if rod
                .thermal_preset_id
                .is_some_and(|id| !self.thermal_presets.iter().any(|item| item.id == id))
            {
                return Err(format!(
                    "стержень {}: выбранный коэффициент α не найден",
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
        self.validate_hinges()?;
        Ok(())
    }

    pub fn next_free_rod_endpoints(&self) -> Option<(usize, usize)> {
        let count = self.nodes.len();
        let pairs = (0..count.saturating_sub(1))
            .map(|start| (start, start + 1))
            .chain((0..count).flat_map(|start| ((start + 1)..count).map(move |end| (start, end))));
        pairs.into_iter().find(|(start, end)| {
            !self.rods.iter().any(|rod| {
                (rod.start_node == *start && rod.end_node == *end)
                    || (rod.start_node == *end && rod.end_node == *start)
            })
        })
    }

    pub fn unique_rod_id(&self, start: usize, end: usize) -> Option<String> {
        let base = rod_id_from_nodes(&self.nodes, start, end)?;
        if self
            .rods
            .iter()
            .all(|rod| !rod.id.trim().eq_ignore_ascii_case(&base))
        {
            return Some(base);
        }
        (2..)
            .map(|number| format!("{base}-{number}"))
            .find(|candidate| {
                self.rods
                    .iter()
                    .all(|rod| !rod.id.trim().eq_ignore_ascii_case(candidate))
            })
    }

    /// Обновляет только имена, совпадавшие с прежним автоматическим именем.
    pub fn refresh_generated_rod_ids(
        &mut self,
        previous_nodes: &[Node],
        previous_rods: &[(String, usize, usize)],
    ) {
        for (rod, (old_id, old_start, old_end)) in self.rods.iter_mut().zip(previous_rods.iter()) {
            if rod.id != *old_id {
                continue;
            }
            let Some(old_base) = rod_id_from_nodes(previous_nodes, *old_start, *old_end) else {
                continue;
            };
            let suffix = if old_id == &old_base {
                Some(None)
            } else {
                old_id
                    .strip_prefix(&format!("{old_base}-"))
                    .filter(|part| part.parse::<usize>().is_ok_and(|number| number >= 2))
                    .map(Some)
            };
            if let (Some(suffix), Some(new_base)) = (
                suffix,
                rod_id_from_nodes(&self.nodes, rod.start_node, rod.end_node),
            ) {
                rod.id = suffix.map_or(new_base.clone(), |part| format!("{new_base}-{part}"));
            }
        }
    }

    pub fn ensure_property_presets(&mut self, rigidity: Rigidity, expansion: ThermalExpansion) {
        for rod in &mut self.rods {
            if rod.rigidity_preset_id.is_none() {
                let value = match rigidity {
                    Rigidity::Symbolic => PropertyValue {
                        symbolic_factor: rod.rigidity_factor,
                        ..PropertyValue::default()
                    },
                    Rigidity::Numeric(base) => {
                        PropertyValue::from_number(base * rod.rigidity_factor)
                    }
                };
                let id = preset_id_for_value(&mut self.rigidity_presets, value, "Жёсткость");
                rod.rigidity_preset_id = Some(id);
            }
            if rod.thermal_preset_id.is_none() {
                let value = match expansion {
                    ThermalExpansion::Symbolic => PropertyValue {
                        symbolic_factor: rod.thermal_expansion_factor,
                        ..PropertyValue::default()
                    },
                    ThermalExpansion::Numeric(base) => {
                        PropertyValue::from_number(base * rod.thermal_expansion_factor)
                    }
                };
                let id = preset_id_for_value(&mut self.thermal_presets, value, "Расширение");
                rod.thermal_preset_id = Some(id);
            }
        }
        if self.rigidity_presets.is_empty() {
            self.rigidity_presets.push(PropertyPreset {
                id: 1,
                name: "Жёсткость 1".into(),
                value: PropertyValue::default(),
            });
        }
        if self.thermal_presets.is_empty() {
            self.thermal_presets.push(PropertyPreset {
                id: 1,
                name: "Расширение 1".into(),
                value: PropertyValue::default(),
            });
        }
    }

    pub fn rigidity_value(&self, rod: &Rod, legacy: Rigidity) -> Result<PropertyValue, String> {
        if let Some(id) = rod.rigidity_preset_id {
            return self
                .rigidity_presets
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.value)
                .ok_or_else(|| format!("стержень {}: жёсткость не найдена", rod.id));
        }
        Ok(match legacy {
            Rigidity::Symbolic => PropertyValue {
                symbolic_factor: rod.rigidity_factor,
                ..PropertyValue::default()
            },
            Rigidity::Numeric(base) => PropertyValue::from_number(base * rod.rigidity_factor),
        })
    }

    pub fn thermal_value(
        &self,
        rod: &Rod,
        legacy: ThermalExpansion,
    ) -> Result<PropertyValue, String> {
        if let Some(id) = rod.thermal_preset_id {
            return self
                .thermal_presets
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.value)
                .ok_or_else(|| format!("стержень {}: коэффициент α не найден", rod.id));
        }
        Ok(match legacy {
            ThermalExpansion::Symbolic => PropertyValue {
                symbolic_factor: rod.thermal_expansion_factor,
                ..PropertyValue::default()
            },
            ThermalExpansion::Numeric(base) => {
                PropertyValue::from_number(base * rod.thermal_expansion_factor)
            }
        })
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
    pub property: Option<PropertyValue>,
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
    pub numeric_part: f64,
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
    pub property: Option<PropertyValue>,
    pub upper_or_left: f64,
    pub lower_or_right: f64,
    pub temperature_difference: f64,
    pub hotter_side: Option<FiberSide>,
    pub hotter_side_sign: f64,
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
    pub numeric_part: f64,
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
            ThermalExpansion::Numeric(alpha) => Some(alpha * self.coefficient + self.numeric_part),
            ThermalExpansion::Symbolic if self.coefficient.abs() <= EPS => Some(self.numeric_part),
            ThermalExpansion::Symbolic => None,
        }
    }

    pub fn formatted_displacement(&self) -> String {
        match self.numeric_displacement() {
            Some(value) => format_number(value),
            None if self.coefficient.abs() <= EPS => "0".into(),
            None => join_expression(
                format!("α · {}", format_number(self.coefficient)),
                self.numeric_part,
            ),
        }
    }
}

impl CalculationResult {
    pub fn numeric_displacement(&self) -> Option<f64> {
        match self.rigidity {
            Rigidity::Numeric(ei) => Some(self.numerator / ei + self.numeric_part),
            Rigidity::Symbolic if self.numerator.abs() <= EPS => Some(self.numeric_part),
            Rigidity::Symbolic => None,
        }
    }

    pub fn formatted_displacement(&self) -> String {
        match self.numeric_displacement() {
            Some(value) => format_number(value),
            None => join_expression(
                format_symbolic_displacement(self.numerator),
                self.numeric_part,
            ),
        }
    }

    /// Десятичное представление результата для одновременного показа с дробью.
    pub fn formatted_decimal_displacement(&self) -> String {
        match self.numeric_displacement() {
            Some(value) => format_number(value),
            None => join_expression(
                format!("{} / EI", format_number(self.numerator)),
                self.numeric_part,
            ),
        }
    }
}

fn join_expression(symbolic: String, numeric: f64) -> String {
    if numeric.abs() <= EPS {
        symbolic
    } else if numeric < 0.0 {
        format!("{symbolic} - {}", format_number(-numeric))
    } else {
        format!("{symbolic} + {}", format_number(numeric))
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

pub fn format_symbolic_force(value: f64) -> String {
    format_symbolic_displacement(value)
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
            property: None,
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
        numeric_part: 0.0,
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
    let calculate = |frame: &Frame| -> Result<CalculationResult, String> {
        let mut result = calculate_displacement(frame, rigidity)?;
        if model
            .rods
            .iter()
            .any(|rod| rod.rigidity_preset_id.is_some())
        {
            result.rigidity = Rigidity::Symbolic;
            result.numerator = 0.0;
            for (item, rod) in result.contributions.iter_mut().zip(&model.rods) {
                let property = model.rigidity_value(rod, rigidity)?;
                item.property = Some(property);
                if property.numeric {
                    item.integral = item.raw_integral
                        / property
                            .numeric_value()
                            .ok_or("некорректная числовая жёсткость")?;
                    result.numeric_part += item.integral;
                } else {
                    item.integral = item.raw_integral / property.symbolic_factor;
                    result.numerator += item.integral;
                }
            }
        }
        Ok(result)
    };
    Ok(DisplacementComponents {
        horizontal: calculate(&horizontal_frame)?,
        vertical: calculate(&vertical_frame)?,
        rotation: calculate(&rotation_frame)?,
    })
}

/// Вычисляет температурные составляющие перемещения по формуле
/// α·Σkα·[sₜ·t′/h·∫m ds + t₀·∫n ds], где t′ = |t₁−t₂|, а sₜ — знак
/// стороны наиболее нагретых волокон. Температура постоянна по длине стержня,
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
    let has_presets = model.rods.iter().any(|rod| rod.thermal_preset_id.is_some());
    let mut symbolic_coefficient = 0.0;
    let mut numeric_part = 0.0;
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
        let hotter_side = temperature.hotter_side();
        let hotter_side_sign = temperature.hotter_side_sign();
        let mean = temperature.mean();
        let moment_area = moment.signed_area(length);
        let normal_area = normal.signed_area(length);
        let bending_term = if active {
            hotter_side_sign * difference / rod.section_height * moment_area
        } else {
            0.0
        };
        let axial_term = if active { mean * normal_area } else { 0.0 };
        let base_term = bending_term + axial_term;
        let property = if has_presets {
            Some(model.thermal_value(rod, expansion)?)
        } else {
            None
        };
        let contribution = if let Some(property) = property {
            if property.numeric {
                let value = base_term
                    * property
                        .numeric_value()
                        .ok_or("некорректный числовой коэффициент α")?;
                numeric_part += value;
                value
            } else {
                let value = base_term * property.symbolic_factor;
                symbolic_coefficient += value;
                value
            }
        } else {
            rod.thermal_expansion_factor * base_term
        };
        contributions.push(ThermalRodContribution {
            rod_id: rod.id.clone(),
            active,
            length,
            section_height: rod.section_height,
            expansion_factor: rod.thermal_expansion_factor,
            property,
            upper_or_left: temperature.upper_or_left,
            lower_or_right: temperature.lower_or_right,
            temperature_difference: difference,
            hotter_side,
            hotter_side_sign,
            mean_temperature: mean,
            unit_moment_area: moment_area,
            unit_normal_area: normal_area,
            bending_term,
            axial_term,
            coefficient: contribution,
        });
    }
    let coefficient = if has_presets {
        symbolic_coefficient
    } else {
        contributions.iter().map(|item| item.coefficient).sum()
    };
    Ok(ThermalCalculationResult {
        contributions,
        coefficient,
        numeric_part,
        expansion: if has_presets {
            ThermalExpansion::Symbolic
        } else {
            expansion
        },
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
    fn force_sign_is_positive_on_same_side_and_negative_on_opposite_sides() {
        let same_side = Frame {
            segments: vec![segment(linear(-2.0, -2.0), linear(-3.0, -3.0))],
        };
        let opposite_sides = Frame {
            segments: vec![segment(linear(-2.0, -2.0), linear(3.0, 3.0))],
        };
        let positive = calculate_displacement(&same_side, Rigidity::Symbolic).unwrap();
        let negative = calculate_displacement(&opposite_sides, Rigidity::Symbolic).unwrap();
        assert!((positive.numerator - 12.0).abs() < EPS);
        assert!((negative.numerator + 12.0).abs() < EPS);
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
            numeric_part: 0.0,
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
                release_start_moment: false,
                release_end_moment: false,
                rigidity_preset_id: None,
                thermal_preset_id: None,
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
            rigidity_presets: Vec::new(),
            thermal_presets: Vec::new(),
            supports: Vec::new(),
        };
        assert!((model.rod_length(&model.rods[0]).unwrap() - 2.0).abs() < EPS);
        let result = calculate_displacement_components(&model, Rigidity::Symbolic).unwrap();
        assert!((result.horizontal.numerator - 12.0).abs() < EPS);
        assert!((result.vertical.numerator + 4.0).abs() < EPS);
        assert!((result.rotation.numerator - 2.0).abs() < EPS);
    }

    #[test]
    fn kinematic_displacement_uses_unit_reactions_and_keeps_zero_terms() {
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
            rods: vec![Rod::blank("AB".into(), 0, 1)],
            target_node: 1,
            rigidity_presets: Vec::new(),
            thermal_presets: Vec::new(),
            supports: vec![
                SupportMotion {
                    node: 0,
                    direction: SupportDirection::Horizontal,
                    normal_deg: 90.0,
                    displacement: 0.5,
                    unit_x_reaction: -2.0,
                    unit_y_reaction: 3.0,
                    unit_rotation_reaction: 4.0,
                },
                SupportMotion {
                    node: 0,
                    direction: SupportDirection::Vertical,
                    normal_deg: 90.0,
                    displacement: 0.0,
                    unit_x_reaction: 10.0,
                    unit_y_reaction: 10.0,
                    unit_rotation_reaction: 10.0,
                },
            ],
        };
        let result = calculate_kinematic_displacement_components(&model).unwrap();
        assert_eq!(result.horizontal.total, 1.0);
        assert_eq!(result.vertical.total, -1.5);
        assert_eq!(result.rotation.total, -2.0);
        assert_eq!(result.horizontal.contributions.len(), 2);
        assert_eq!(result.horizontal.contributions[1].value, 0.0);
        let mut duplicate = model;
        duplicate.supports[1].direction = SupportDirection::Horizontal;
        assert!(duplicate.validate().unwrap_err().contains("задано дважды"));
    }

    #[test]
    fn legacy_support_motion_without_normal_angle_still_opens() {
        let text = "(node:0,direction:Vertical,displacement:0.02,unit_x_reaction:0.0,unit_y_reaction:-1.0,unit_rotation_reaction:0.0)";
        let motion: SupportMotion = ron::from_str(text).unwrap();
        assert_eq!(motion.normal_deg, 90.0);
        assert_eq!(motion.direction, SupportDirection::Vertical);
    }

    #[test]
    fn point_and_rod_names_follow_endpoints() {
        assert_eq!(alphabetical_node_id(0), "A");
        assert_eq!(alphabetical_node_id(25), "Z");
        assert_eq!(alphabetical_node_id(26), "AA");
        assert_eq!(alphabetical_node_id(51), "AZ");
        assert_eq!(alphabetical_node_id(52), "BA");
        let nodes = vec![
            Node {
                id: "AA".into(),
                position: Point { x: 0.0, y: 0.0 },
            },
            Node {
                id: "B".into(),
                position: Point { x: 1.0, y: 0.0 },
            },
        ];
        assert_eq!(next_node_id(&nodes), "A");
        assert_eq!(rod_id_from_nodes(&nodes, 0, 1).as_deref(), Some("AA-B"));
        let mut model = StructuralModel {
            nodes: nodes.clone(),
            rods: vec![Rod::blank("AA-B".into(), 0, 1)],
            target_node: 0,
            rigidity_presets: Vec::new(),
            thermal_presets: Vec::new(),
            supports: Vec::new(),
        };
        let old = vec![("AA-B".into(), 0, 1)];
        model.nodes[0].id = "C".into();
        model.refresh_generated_rod_ids(&nodes, &old);
        assert_eq!(model.rods[0].id, "CB");
        model.nodes.push(Node {
            id: "D".into(),
            position: Point { x: 2.0, y: 0.0 },
        });
        let old_nodes = model.nodes.clone();
        model.rods[0].id = "CB-2".into();
        let previous = vec![("CB-2".into(), 0, 1)];
        model.rods[0].end_node = 2;
        model.refresh_generated_rod_ids(&old_nodes, &previous);
        assert_eq!(model.rods[0].id, "CD-2");
        model.rods[0].id = "Ригель".into();
        model.nodes[0].id = "E".into();
        let previous = vec![("Ригель".into(), 0, 1)];
        model.refresh_generated_rod_ids(&model.nodes.clone(), &previous);
        assert_eq!(model.rods[0].id, "Ригель");
    }

    #[test]
    fn duplicate_positions_and_reversed_rods_are_rejected() {
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
                Node {
                    id: "C".into(),
                    position: Point { x: 0.0, y: 1.0e-10 },
                },
            ],
            rods: vec![Rod::blank("AB".into(), 0, 1)],
            target_node: 1,
            rigidity_presets: Vec::new(),
            thermal_presets: Vec::new(),
            supports: Vec::new(),
        };
        assert!(model
            .validate()
            .unwrap_err()
            .contains("одинаковые координаты"));
        assert!(model.validate_for_loading().is_ok());
        model.nodes[2].position.y = 1.0;
        model.rods.push(Rod::blank("BA".into(), 1, 0));
        assert!(model
            .validate()
            .unwrap_err()
            .contains("совпадают по концам"));
        model.rods.pop();
        model.nodes[2].id = " a ".into();
        assert!(model.validate().unwrap_err().contains("названия точек"));
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
                release_start_moment: false,
                release_end_moment: false,
                rigidity_preset_id: None,
                thermal_preset_id: None,
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
            rigidity_presets: Vec::new(),
            thermal_presets: Vec::new(),
            supports: Vec::new(),
        };
        let result =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Symbolic).unwrap();
        assert!((result.horizontal.contributions[0].bending_term - 160.0).abs() < EPS);
        assert!((result.horizontal.contributions[0].axial_term - 120.0).abs() < EPS);
        assert!((result.horizontal.coefficient - 560.0).abs() < EPS);
        assert_eq!(result.horizontal.formatted_displacement(), "α · 560");
        assert_eq!(
            result.horizontal.contributions[0].temperature_difference,
            20.0
        );
        assert_eq!(
            result.horizontal.contributions[0].hotter_side,
            Some(FiberSide::LowerOrRight)
        );

        // Нижняя/правая сторона горячее: эпюра с противоположной стороны даёт минус.
        model.rods[0].unit_x_diagram = linear(-2.0, -2.0);
        let opposite =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Symbolic).unwrap();
        assert!((opposite.horizontal.contributions[0].bending_term + 160.0).abs() < EPS);

        // Верхняя/левая сторона горячее: эпюра на этой же стороне снова даёт плюс.
        model.rods[0].temperature = Some(TemperatureLoad {
            upper_or_left: 30.0,
            lower_or_right: 10.0,
        });
        let upper_same =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Symbolic).unwrap();
        assert!((upper_same.horizontal.contributions[0].bending_term - 160.0).abs() < EPS);
        assert_eq!(
            upper_same.horizontal.contributions[0].temperature_difference,
            20.0
        );
        assert_eq!(
            upper_same.horizontal.contributions[0].hotter_side,
            Some(FiberSide::UpperOrLeft)
        );

        // При горячей верхней/левой стороне эпюра снизу/справа даёт минус.
        model.rods[0].unit_x_diagram = linear(2.0, 2.0);
        let upper_opposite =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Symbolic).unwrap();
        assert!((upper_opposite.horizontal.contributions[0].bending_term + 160.0).abs() < EPS);

        model.rods[0].temperature = None;
        let disabled =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Numeric(0.000012))
                .unwrap();
        assert!(disabled.horizontal.coefficient.abs() < EPS);
        assert_eq!(disabled.horizontal.formatted_displacement(), "0");
    }

    #[test]
    fn independent_presets_mix_symbolic_and_numeric_force_and_temperature() {
        let nodes = vec![
            Node {
                id: "A".into(),
                position: Point { x: 0.0, y: 0.0 },
            },
            Node {
                id: "B".into(),
                position: Point { x: 2.0, y: 0.0 },
            },
            Node {
                id: "C".into(),
                position: Point { x: 4.0, y: 0.0 },
            },
        ];
        let mut first = Rod::blank("AB".into(), 0, 1);
        let mut second = Rod::blank("BC".into(), 1, 2);
        for rod in [&mut first, &mut second] {
            rod.load_diagram = linear(2.0, 2.0);
            rod.unit_x_diagram = linear(3.0, 3.0);
            rod.unit_normal_x_diagram = linear(1.0, 1.0);
            rod.temperature = Some(TemperatureLoad {
                upper_or_left: 10.0,
                lower_or_right: 10.0,
            });
        }
        first.rigidity_preset_id = Some(1);
        first.thermal_preset_id = Some(1);
        second.rigidity_preset_id = Some(2);
        second.thermal_preset_id = Some(2);
        let model = StructuralModel {
            nodes,
            rods: vec![first, second],
            target_node: 2,
            supports: vec![],
            rigidity_presets: vec![
                PropertyPreset {
                    id: 1,
                    name: "Первая".into(),
                    value: PropertyValue {
                        symbolic_factor: 6.0,
                        ..PropertyValue::default()
                    },
                },
                PropertyPreset {
                    id: 2,
                    name: "Вторая".into(),
                    value: PropertyValue {
                        numeric: true,
                        mantissa: 2.0,
                        exponent: 5,
                        ..PropertyValue::default()
                    },
                },
            ],
            thermal_presets: vec![
                PropertyPreset {
                    id: 1,
                    name: "Первый".into(),
                    value: PropertyValue {
                        symbolic_factor: 2.0,
                        ..PropertyValue::default()
                    },
                },
                PropertyPreset {
                    id: 2,
                    name: "Второй".into(),
                    value: PropertyValue {
                        numeric: true,
                        mantissa: 1.0,
                        exponent: -5,
                        ..PropertyValue::default()
                    },
                },
            ],
        };
        let force = calculate_displacement_components(&model, Rigidity::Symbolic).unwrap();
        assert!((force.horizontal.numerator - 2.0).abs() < EPS);
        assert!((force.horizontal.numeric_part - 0.00006).abs() < EPS);
        assert_eq!(
            force.horizontal.formatted_displacement(),
            "2 / EI + 0.00006"
        );
        let thermal =
            calculate_thermal_displacement_components(&model, ThermalExpansion::Symbolic).unwrap();
        assert!((thermal.horizontal.coefficient - 40.0).abs() < EPS);
        assert!((thermal.horizontal.numeric_part - 0.0002).abs() < EPS);
        assert_eq!(
            thermal.horizontal.formatted_displacement(),
            "α · 40 + 0.0002"
        );
    }
}

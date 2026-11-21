//! Линейный МКЭ плоской рамы: элемент Эйлера — Бернулли, 3 СС в узле.

use crate::{
    Diagram, DiagramKind, FiberSide, Ordinate, PropertyPreset, PropertyValue, StructuralModel,
    SupportDirection, SupportMotion,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SupportKind {
    #[default]
    Free,
    Fixed,
    Pin,
    Roller,
    Guided,
}

impl SupportKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "Нет",
            Self::Fixed => "Жёсткая заделка",
            Self::Pin => "Шарнирно неподвижная",
            Self::Roller => "Шарнирно подвижная",
            Self::Guided => "Скользящая заделка",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FemNode {
    pub id: String,
    pub support: SupportKind,
    /// Угол нормали, по которой запрещено смещение (для ролика и направляющей).
    pub normal_deg: f64,
    pub fx: f64,
    pub fy: f64,
    pub mz: f64,
}
impl FemNode {
    fn blank(id: String) -> Self {
        Self {
            id,
            support: SupportKind::Free,
            normal_deg: 90.0,
            fx: 0.0,
            fy: 0.0,
            mz: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointLoad {
    pub xi: f64,
    pub fx: f64,
    pub fy: f64,
    pub mz: f64,
}
impl Default for PointLoad {
    fn default() -> Self {
        Self {
            xi: 0.5,
            fx: 0.0,
            fy: 0.0,
            mz: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FemMember {
    pub id: String,
    pub start_id: String,
    pub end_id: String,
    /// Необязательные численные EA и EI. Нули выбирают расчёт усилий
    /// статически определимой рамы без задания жёсткостей.
    pub ea: f64,
    pub ei: f64,
    /// Равномерные нагрузки в глобальных X, Y.
    pub qx: f64,
    pub qy: f64,
    #[serde(default)]
    pub point_loads: Vec<PointLoad>,
}
impl FemMember {
    fn blank(id: String, start_id: String, end_id: String) -> Self {
        Self {
            id,
            start_id,
            end_id,
            ea: 0.0,
            ei: 0.0,
            qx: 0.0,
            qy: 0.0,
            point_loads: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSelection {
    pub x: bool,
    pub y: bool,
    pub rotation: bool,
}
impl Default for UnitSelection {
    fn default() -> Self {
        Self {
            x: true,
            y: true,
            rotation: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FemProject {
    pub nodes: Vec<FemNode>,
    pub members: Vec<FemMember>,
    #[serde(default)]
    pub units: UnitSelection,
    #[serde(default)]
    pub target_node_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitComponent {
    X,
    Y,
    Rotation,
}
impl UnitComponent {
    pub fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Rotation => "φ",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Rotation => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FemResult {
    pub actual: LoadCaseResult,
    pub units: Vec<UnitResult>,
    pub total_dof: usize,
    pub constraints: usize,
    pub stiffness_mode: StiffnessMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StiffnessMode {
    /// Полная матрица жёсткости с заданными численными EA и EI.
    Numeric,
    /// Усилия статически определимой рамы; масштаб жёсткости не влияет на них.
    Determinate,
}
#[derive(Debug, Clone)]
pub struct UnitResult {
    pub component: UnitComponent,
    pub result: LoadCaseResult,
}
#[derive(Debug, Clone)]
pub struct LoadCaseResult {
    pub displacements: Vec<[f64; 3]>,
    pub reactions: Vec<[f64; 3]>,
    pub members: Vec<MemberResult>,
}
#[derive(Debug, Clone, Copy)]
pub struct SectionForces {
    pub n: f64,
    pub q: f64,
    pub m: f64,
}
#[derive(Debug, Clone)]
pub struct MemberResult {
    pub length: f64,
    pub end_forces: [f64; 6],
    pub qx_local: f64,
    pub qy_local: f64,
    pub point_loads_local: Vec<PointLoad>,
}

impl FemResult {
    /// Передаёт эпюры в ручной метод Мора без изменения исходной модели при ошибке.
    /// Одна `Diagram` не хранит скачки от силы внутри стержня: для них передача
    /// запрещена, хотя самостоятельный МКЭ их рассчитывает и рисует.
    pub fn apply_to_model(
        &self,
        model: &StructuralModel,
        input: &FemProject,
    ) -> Result<StructuralModel, String> {
        if input.members.iter().any(|m| !m.point_loads.is_empty()) {
            return Err("Передача эпюр со сосредоточенной нагрузкой внутри стержня пока недоступна: ручной формат хранит одну функцию на стержень".into());
        }
        let find = |component| {
            self.units
                .iter()
                .find(|u| u.component == component)
                .map(|u| &u.result)
        };
        let x = find(UnitComponent::X)
            .ok_or("Для передачи включите все три единичные системы: X, Y и φ")?;
        let y = find(UnitComponent::Y)
            .ok_or("Для передачи включите все три единичные системы: X, Y и φ")?;
        let rotation = find(UnitComponent::Rotation)
            .ok_or("Для передачи включите все три единичные системы: X, Y и φ")?;
        let mut updated = model.clone();
        for (i, rod) in updated.rods.iter_mut().enumerate() {
            let actual = &self.actual.members[i];
            rod.load_diagram = moment_diagram(actual)?;
            rod.normal_diagram = linear_diagram(actual.at(0.0).n, actual.at(actual.length).n)?;
            rod.shear_diagram = linear_diagram(actual.at(0.0).q, actual.at(actual.length).q)?;
            for (case, m, n) in [
                (
                    &x.members[i],
                    &mut rod.unit_x_diagram,
                    &mut rod.unit_normal_x_diagram,
                ),
                (
                    &y.members[i],
                    &mut rod.unit_y_diagram,
                    &mut rod.unit_normal_y_diagram,
                ),
                (
                    &rotation.members[i],
                    &mut rod.unit_rotation_diagram,
                    &mut rod.unit_normal_rotation_diagram,
                ),
            ] {
                *m = moment_diagram(case)?;
                *n = linear_diagram(case.at(0.0).n, case.at(case.length).n)?;
            }
            if self.stiffness_mode == StiffnessMode::Numeric {
                let ei = input.members[i].ei;
                let id = if let Some(p) = updated.rigidity_presets.iter().find(|p| {
                    p.value.numeric
                        && p.value
                            .numeric_value()
                            .is_some_and(|v| (v - ei).abs() <= 1e-9 * ei.max(1.0))
                }) {
                    p.id
                } else {
                    let id = updated
                        .rigidity_presets
                        .iter()
                        .map(|p| p.id)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    updated.rigidity_presets.push(PropertyPreset {
                        id,
                        name: format!("EI МКЭ {id}"),
                        value: PropertyValue::from_number(ei),
                    });
                    id
                };
                rod.rigidity_preset_id = Some(id);
            }
        }
        // Реакции трёх единичных систем нужны для −ΣRᵢcᵢ. Уже заданные
        // смещения сохраняем для совпадающих связей.
        let previous = &model.supports;
        updated.supports.clear();
        for (node, data) in input.nodes.iter().enumerate() {
            let mut directions = Vec::new();
            match data.support {
                SupportKind::Free => {}
                SupportKind::Fixed => directions.extend([
                    SupportDirection::Horizontal,
                    SupportDirection::Vertical,
                    SupportDirection::Rotation,
                ]),
                SupportKind::Pin => {
                    directions.extend([SupportDirection::Horizontal, SupportDirection::Vertical])
                }
                SupportKind::Roller | SupportKind::Guided => {
                    let angle = data.normal_deg.to_radians();
                    let direction = if angle.sin().abs() < 1e-10 {
                        SupportDirection::Horizontal
                    } else if angle.cos().abs() < 1e-10 {
                        SupportDirection::Vertical
                    } else {
                        SupportDirection::Normal
                    };
                    directions.push(direction);
                    if data.support == SupportKind::Guided {
                        directions.push(SupportDirection::Rotation);
                    }
                }
            }
            for direction in directions {
                let angle = data.normal_deg.to_radians();
                let projection = |case: &LoadCaseResult| {
                    let r = case.reactions[node];
                    match direction {
                        SupportDirection::Horizontal => r[0],
                        SupportDirection::Vertical => r[1],
                        SupportDirection::Rotation => r[2],
                        SupportDirection::Normal => r[0] * angle.cos() + r[1] * angle.sin(),
                    }
                };
                let displacement = previous
                    .iter()
                    .find(|old| {
                        old.node == node
                            && old.direction == direction
                            && (direction != SupportDirection::Normal
                                || (old.normal_deg - data.normal_deg).abs() < 1e-9)
                    })
                    .map_or(0.0, |old| old.displacement);
                updated.supports.push(SupportMotion {
                    node,
                    direction,
                    normal_deg: data.normal_deg,
                    displacement,
                    unit_x_reaction: projection(x),
                    unit_y_reaction: projection(y),
                    unit_rotation_reaction: projection(rotation),
                });
            }
        }
        updated.validate()?;
        Ok(updated)
    }
}

fn linear_diagram(left: f64, right: f64) -> Result<Diagram, String> {
    Diagram::new(
        DiagramKind::Linear,
        Ordinate::from_signed(left)?,
        Ordinate::from_signed(right)?,
        None,
        None,
        false,
    )
}

fn moment_diagram(member: &MemberResult) -> Result<Diagram, String> {
    let left = Ordinate::from_signed(member.at(0.0).m)?;
    let right = Ordinate::from_signed(member.at(member.length).m)?;
    if member.qy_local.abs() < 1e-12 {
        return Diagram::new(DiagramKind::Linear, left, right, None, None, false);
    }
    let side = if member.qy_local < 0.0 {
        FiberSide::LowerOrRight
    } else {
        FiberSide::UpperOrLeft
    };
    let vertex = -member.end_forces[1] / member.qy_local;
    Diagram::new(
        DiagramKind::Parabolic,
        left,
        right,
        Some(member.qy_local.abs()),
        Some(side),
        vertex > 0.0 && vertex < member.length,
    )
}
impl MemberResult {
    /// Усилия в сечении по равновесию левой части стержня.
    pub fn at(&self, x: f64) -> SectionForces {
        let x = x.clamp(0.0, self.length);
        let mut n = -self.end_forces[0] - self.qx_local * x;
        let mut q = -self.end_forces[1] - self.qy_local * x;
        let mut m = -self.end_forces[2] + self.end_forces[1] * x + self.qy_local * x * x / 2.0;
        for p in &self.point_loads_local {
            let a = p.xi * self.length;
            if x + 1e-10 >= a {
                n -= p.fx;
                q -= p.fy;
                m += p.fy * (x - a) - p.mz;
            }
        }
        SectionForces { n, q, m }
    }
}

impl FemProject {
    pub fn is_aligned(&self, model: &StructuralModel) -> bool {
        self.nodes.len() == model.nodes.len()
            && self.members.len() == model.rods.len()
            && self
                .nodes
                .iter()
                .zip(&model.nodes)
                .all(|(a, b)| a.id == b.id)
            && self.members.iter().zip(&model.rods).all(|(a, b)| {
                a.id == b.id
                    && model
                        .nodes
                        .get(b.start_node)
                        .is_some_and(|n| a.start_id == n.id)
                    && model
                        .nodes
                        .get(b.end_node)
                        .is_some_and(|n| a.end_id == n.id)
            })
    }

    /// Сопоставление по именам и концам не переносит нагрузки на другой стержень.
    pub fn reconcile(&mut self, model: &StructuralModel) {
        self.nodes = model
            .nodes
            .iter()
            .map(|node| {
                self.nodes
                    .iter()
                    .find(|x| x.id == node.id)
                    .cloned()
                    .unwrap_or_else(|| FemNode::blank(node.id.clone()))
            })
            .collect();
        self.members = model
            .rods
            .iter()
            .map(|rod| {
                let a = model
                    .nodes
                    .get(rod.start_node)
                    .map_or("", |n| n.id.as_str());
                let b = model.nodes.get(rod.end_node).map_or("", |n| n.id.as_str());
                self.members
                    .iter()
                    .find(|x| x.id == rod.id && x.start_id == a && x.end_id == b)
                    .cloned()
                    .unwrap_or_else(|| FemMember::blank(rod.id.clone(), a.into(), b.into()))
            })
            .collect();
        if !model.nodes.iter().any(|n| n.id == self.target_node_id) {
            self.target_node_id = model
                .nodes
                .get(model.target_node)
                .map_or(String::new(), |n| n.id.clone());
        }
    }

    pub fn solve(&self, model: &StructuralModel) -> Result<FemResult, String> {
        validate_geometry(model)?;
        if !self.is_aligned(model) {
            return Err(
                "Геометрия изменилась: обновите модель МКЭ, проверив сохранённые нагрузки".into(),
            );
        }
        for member in &self.members {
            for (name, value) in [("EA", member.ea), ("EI", member.ei)] {
                if !value.is_finite() || value < 0.0 {
                    return Err(format!(
                        "стержень {}: {name} должно быть неотрицательным конечным числом",
                        member.id
                    ));
                }
            }
        }
        let numeric = self.members.iter().all(|m| m.ea > 0.0 && m.ei > 0.0);
        let stiffness_mode = if numeric {
            StiffnessMode::Numeric
        } else {
            StiffnessMode::Determinate
        };
        // В статически определимой системе усилия не зависят от жёсткостей.
        // Единичные значения нужны только для решения внутренней СЛАУ МКЭ.
        let mut normalized = self.clone();
        if !numeric {
            for member in &mut normalized.members {
                member.ea = 1.0;
                member.ei = 1.0;
            }
        }
        let prepared = Prepared::new(model, &normalized)?;
        let redundancy = prepared.constraints.len() as isize + 3 * model.rods.len() as isize
            - 3 * model.nodes.len() as isize;
        if !numeric && redundancy < 0 {
            return Err(
                "рама имеет недостаточно связей для статически определимого расчёта".into(),
            );
        }
        let actual = prepared.solve_case(self, None)?;
        if !numeric && redundancy > 0 {
            return Err(format!(
                "рама статически неопределима (лишних связей/контуров: {redundancy}); для её эпюр задайте численные EA и EI всех стержней"
            ));
        }
        let target = model
            .nodes
            .iter()
            .position(|n| n.id == self.target_node_id)
            .ok_or("Выберите точку для единичных эпюр")?;
        let mut units = Vec::new();
        for (enabled, component) in [
            (self.units.x, UnitComponent::X),
            (self.units.y, UnitComponent::Y),
            (self.units.rotation, UnitComponent::Rotation),
        ] {
            if enabled {
                units.push(UnitResult {
                    component,
                    result: prepared.solve_case(self, Some((target, component)))?,
                });
            }
        }
        Ok(FemResult {
            actual,
            units,
            total_dof: model.nodes.len() * 3,
            constraints: prepared.constraints.len(),
            stiffness_mode,
        })
    }
}

fn validate_geometry(model: &StructuralModel) -> Result<(), String> {
    if model.nodes.len() < 2 || model.rods.is_empty() {
        return Err("Для МКЭ нужны как минимум два узла и один стержень".into());
    }
    for node in &model.nodes {
        if node.id.trim().is_empty() || !node.position.x.is_finite() || !node.position.y.is_finite()
        {
            return Err("Имя и координаты каждого узла должны быть корректными".into());
        }
    }
    for rod in &model.rods {
        if rod.id.trim().is_empty()
            || rod.start_node >= model.nodes.len()
            || rod.end_node >= model.nodes.len()
            || rod.start_node == rod.end_node
        {
            return Err(format!("стержень {}: некорректные концы", rod.id));
        }
        let a = model.nodes[rod.start_node].position;
        let b = model.nodes[rod.end_node].position;
        if (b.x - a.x).hypot(b.y - a.y) <= 1e-10 {
            return Err(format!("стержень {} имеет нулевую длину", rod.id));
        }
    }
    model.validate_unique_geometry()
}

struct Element {
    dofs: [usize; 6],
    length: f64,
    c: f64,
    s: f64,
    k_local: [[f64; 6]; 6],
}
struct Prepared {
    n: usize,
    k: Vec<f64>,
    elements: Vec<Element>,
    constraints: Vec<Vec<f64>>,
}

fn rotation(local: usize, global: usize, c: f64, s: f64) -> f64 {
    if local / 3 != global / 3 {
        return 0.0;
    }
    match (local % 3, global % 3) {
        (0, 0) => c,
        (0, 1) => s,
        (1, 0) => -s,
        (1, 1) => c,
        (2, 2) => 1.0,
        _ => 0.0,
    }
}

fn local_stiffness(ea: f64, ei: f64, l: f64) -> [[f64; 6]; 6] {
    let a = ea / l;
    let b = 12.0 * ei / l.powi(3);
    let c = 6.0 * ei / l.powi(2);
    let d = 4.0 * ei / l;
    let e = 2.0 * ei / l;
    [
        [a, 0.0, 0.0, -a, 0.0, 0.0],
        [0.0, b, c, 0.0, -b, c],
        [0.0, c, d, 0.0, -c, e],
        [-a, 0.0, 0.0, a, 0.0, 0.0],
        [0.0, -b, -c, 0.0, b, -c],
        [0.0, c, e, 0.0, -c, d],
    ]
}

fn solve_linear(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Result<Vec<f64>, ()> {
    let max = a.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    if max == 0.0 || !max.is_finite() {
        return Err(());
    }
    for p in 0..n {
        let pivot = (p..n)
            .max_by(|&i, &j| a[i * n + p].abs().total_cmp(&a[j * n + p].abs()))
            .unwrap();
        if a[pivot * n + p].abs() <= max * 1e-12 {
            return Err(());
        }
        if pivot != p {
            for col in 0..n {
                a.swap(p * n + col, pivot * n + col);
            }
            b.swap(p, pivot);
        }
        for row in p + 1..n {
            let t = a[row * n + p] / a[p * n + p];
            a[row * n + p] = 0.0;
            for col in p + 1..n {
                a[row * n + col] -= t * a[p * n + col];
            }
            b[row] -= t * b[p];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|j| a[i * n + j] * x[j]).sum::<f64>()) / a[i * n + i];
    }
    x.iter().all(|v| v.is_finite()).then_some(x).ok_or(())
}

impl Prepared {
    fn new(model: &StructuralModel, input: &FemProject) -> Result<Self, String> {
        let n = model.nodes.len() * 3;
        let mut k = vec![0.0; n * n];
        let mut elements = Vec::new();
        for (rod, member) in model.rods.iter().zip(&input.members) {
            for (name, value) in [("EA", member.ea), ("EI", member.ei)] {
                if !value.is_finite() || value <= 0.0 {
                    return Err(format!(
                        "стержень {}: задайте положительное числовое {name}",
                        rod.id
                    ));
                }
            }
            let a = model.nodes[rod.start_node].position;
            let b = model.nodes[rod.end_node].position;
            let length = (b.x - a.x).hypot(b.y - a.y);
            if length <= 1e-10 {
                return Err(format!("стержень {} имеет нулевую длину", rod.id));
            }
            let c = (b.x - a.x) / length;
            let s = (b.y - a.y) / length;
            let kl = local_stiffness(member.ea, member.ei, length);
            let dofs = [
                rod.start_node * 3,
                rod.start_node * 3 + 1,
                rod.start_node * 3 + 2,
                rod.end_node * 3,
                rod.end_node * 3 + 1,
                rod.end_node * 3 + 2,
            ];
            for i in 0..6 {
                for j in 0..6 {
                    let mut entry = 0.0;
                    for (p, row) in kl.iter().enumerate() {
                        for (q, value) in row.iter().enumerate() {
                            entry += rotation(p, i, c, s) * value * rotation(q, j, c, s);
                        }
                    }
                    k[dofs[i] * n + dofs[j]] += entry;
                }
            }
            elements.push(Element {
                dofs,
                length,
                c,
                s,
                k_local: kl,
            });
        }
        let mut constraints = Vec::new();
        for (index, node) in input.nodes.iter().enumerate() {
            if !node.normal_deg.is_finite() {
                return Err(format!("узел {}: угол нормали некорректен", node.id));
            }
            let mut add = |x: f64, y: f64, phi: f64| {
                let mut row = vec![0.0; n];
                row[index * 3] = x;
                row[index * 3 + 1] = y;
                row[index * 3 + 2] = phi;
                constraints.push(row);
            };
            let angle = node.normal_deg.to_radians();
            match node.support {
                SupportKind::Free => (),
                SupportKind::Fixed => {
                    add(1.0, 0.0, 0.0);
                    add(0.0, 1.0, 0.0);
                    add(0.0, 0.0, 1.0);
                }
                SupportKind::Pin => {
                    add(1.0, 0.0, 0.0);
                    add(0.0, 1.0, 0.0);
                }
                SupportKind::Roller => add(angle.cos(), angle.sin(), 0.0),
                SupportKind::Guided => {
                    add(angle.cos(), angle.sin(), 0.0);
                    add(0.0, 0.0, 1.0);
                }
            }
        }
        if constraints.is_empty() {
            return Err("Рама не закреплена: отсутствуют опорные связи".into());
        }
        Ok(Self {
            n,
            k,
            elements,
            constraints,
        })
    }

    fn solve_case(
        &self,
        input: &FemProject,
        unit: Option<(usize, UnitComponent)>,
    ) -> Result<LoadCaseResult, String> {
        let mut f = vec![0.0; self.n];
        if let Some((node, component)) = unit {
            f[node * 3 + component.index()] = 1.0;
        } else {
            for (i, node) in input.nodes.iter().enumerate() {
                for (j, value) in [node.fx, node.fy, node.mz].into_iter().enumerate() {
                    if !value.is_finite() {
                        return Err(format!("узел {}: нагрузка должна быть конечной", node.id));
                    }
                    f[i * 3 + j] += value;
                }
            }
        }
        let mut local_loads = Vec::new();
        for (element, member) in self.elements.iter().zip(&input.members) {
            let mut fe = [0.0; 6];
            let mut points = Vec::new();
            let (qx, qy) = if unit.is_some() {
                (0.0, 0.0)
            } else {
                if !member.qx.is_finite() || !member.qy.is_finite() {
                    return Err(format!(
                        "стержень {}: распределённая нагрузка некорректна",
                        member.id
                    ));
                }
                (
                    element.c * member.qx + element.s * member.qy,
                    -element.s * member.qx + element.c * member.qy,
                )
            };
            let l = element.length;
            fe[0] += qx * l / 2.0;
            fe[3] += qx * l / 2.0;
            fe[1] += qy * l / 2.0;
            fe[4] += qy * l / 2.0;
            fe[2] += qy * l * l / 12.0;
            fe[5] -= qy * l * l / 12.0;
            if unit.is_none() {
                for p in &member.point_loads {
                    if !p.xi.is_finite()
                        || !(0.0..=1.0).contains(&p.xi)
                        || ![p.fx, p.fy, p.mz].iter().all(|v| v.is_finite())
                    {
                        return Err(format!(
                            "стержень {}: сосредоточенная нагрузка некорректна",
                            member.id
                        ));
                    }
                    let fx = element.c * p.fx + element.s * p.fy;
                    let fy = -element.s * p.fx + element.c * p.fy;
                    let r = p.xi;
                    let r2 = r * r;
                    let r3 = r2 * r;
                    fe[0] += fx * (1.0 - r);
                    fe[3] += fx * r;
                    fe[1] += fy * (1.0 - 3.0 * r2 + 2.0 * r3) + p.mz * (-6.0 * r + 6.0 * r2) / l;
                    fe[2] += fy * l * (r - 2.0 * r2 + r3) + p.mz * (1.0 - 4.0 * r + 3.0 * r2);
                    fe[4] += fy * (3.0 * r2 - 2.0 * r3) + p.mz * (6.0 * r - 6.0 * r2) / l;
                    fe[5] += fy * l * (-r2 + r3) + p.mz * (-2.0 * r + 3.0 * r2);
                    points.push(PointLoad {
                        xi: r,
                        fx,
                        fy,
                        mz: p.mz,
                    });
                }
            }
            for i in 0..6 {
                let mut value = 0.0;
                for (j, local) in fe.iter().enumerate() {
                    value += rotation(j, i, element.c, element.s) * local;
                }
                f[element.dofs[i]] += value;
            }
            local_loads.push((fe, qx, qy, points));
        }
        // Множители Лагранжа позволяют задавать наклонную нормаль ролика без
        // искусственного штрафного коэффициента жёсткости.
        let size = self.n + self.constraints.len();
        let mut a = vec![0.0; size * size];
        let mut rhs = vec![0.0; size];
        for i in 0..self.n {
            rhs[i] = f[i];
            for j in 0..self.n {
                a[i * size + j] = self.k[i * self.n + j];
            }
        }
        for (r, row) in self.constraints.iter().enumerate() {
            for i in 0..self.n {
                a[i * size + self.n + r] = row[i];
                a[(self.n + r) * size + i] = row[i];
            }
        }
        // Масштабирование конгруэнцией приводит жёсткости и связи к близкому
        // порядку величины. Без него реальное EA ~ 10¹¹ делает опорные
        // коэффициенты порядка единицы ложной «вырожденностью».
        let mut scales = vec![1.0; size];
        for (i, scale) in scales.iter_mut().enumerate().take(self.n) {
            *scale = self.k[i * self.n + i].abs().sqrt().max(1e-30);
        }
        for (r, row) in self.constraints.iter().enumerate() {
            let characteristic = (0..self.n)
                .map(|i| row[i] * row[i] * self.k[i * self.n + i].abs())
                .sum::<f64>();
            scales[self.n + r] = 1.0 / characteristic.sqrt().max(1e-30);
        }
        for i in 0..size {
            rhs[i] /= scales[i];
            for j in 0..size {
                a[i * size + j] /= scales[i] * scales[j];
            }
        }
        let mut solved=solve_linear(a,rhs,size).map_err(|_|"Рама геометрически изменяема или связи зависимы: матрица МКЭ вырождена. Проверьте закрепления и соединения".to_string())?;
        for i in 0..size {
            solved[i] /= scales[i];
        }
        let d = &solved[..self.n];
        let reactions = (0..self.n)
            .map(|i| {
                (0..self.n)
                    .map(|j| self.k[i * self.n + j] * d[j])
                    .sum::<f64>()
                    - f[i]
            })
            .collect::<Vec<_>>();
        let displacements = d.as_chunks::<3>().0.to_vec();
        let reactions = reactions.as_chunks::<3>().0.to_vec();
        let members = self
            .elements
            .iter()
            .zip(local_loads)
            .map(|(e, (fe, qx, qy, points))| {
                let mut dl = [0.0; 6];
                for (i, value) in dl.iter_mut().enumerate() {
                    for j in 0..6 {
                        *value += rotation(i, j, e.c, e.s) * d[e.dofs[j]];
                    }
                }
                let mut end_forces = [0.0; 6];
                for i in 0..6 {
                    end_forces[i] = (0..6).map(|j| e.k_local[i][j] * dl[j]).sum::<f64>() - fe[i];
                }
                MemberResult {
                    length: e.length,
                    end_forces,
                    qx_local: qx,
                    qy_local: qy,
                    point_loads_local: points,
                }
            })
            .collect();
        Ok(LoadCaseResult {
            displacements,
            reactions,
            members,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Node, Point, Rod};

    fn beam(length: f64) -> (StructuralModel, FemProject) {
        let model = StructuralModel {
            nodes: vec![
                Node {
                    id: "A".into(),
                    position: Point { x: 0.0, y: 0.0 },
                },
                Node {
                    id: "B".into(),
                    position: Point { x: length, y: 0.0 },
                },
            ],
            rods: vec![Rod::blank("AB".into(), 0, 1)],
            target_node: 1,
            ..Default::default()
        };
        let mut input = FemProject::default();
        input.reconcile(&model);
        input.members[0].ea = 1000.0;
        input.members[0].ei = 10.0;
        (model, input)
    }

    #[test]
    fn determinate_frame_needs_no_numeric_stiffness_and_keeps_analytical_ei() {
        let (mut model, _) = beam(2.0);
        model.rigidity_presets.push(PropertyPreset {
            id: 1,
            name: "Аналитическая EI".into(),
            value: PropertyValue {
                symbolic_factor: 2.0,
                ..PropertyValue::default()
            },
        });
        model.rods[0].rigidity_preset_id = Some(1);
        let mut input = FemProject::default();
        input.reconcile(&model);
        assert_eq!(input.members[0].ea, 0.0);
        assert_eq!(input.members[0].ei, 0.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].fy = -5.0;
        let result = input.solve(&model).unwrap();
        assert_eq!(result.stiffness_mode, StiffnessMode::Determinate);
        near(result.actual.reactions[0][1], 5.0);
        near(result.actual.members[0].at(0.0).m, -10.0);
        let updated = result.apply_to_model(&model, &input).unwrap();
        assert_eq!(updated.rods[0].rigidity_preset_id, Some(1));
        assert!(!updated.rigidity_presets[0].value.numeric);
        near(updated.rigidity_presets[0].value.symbolic_factor, 2.0);
    }

    #[test]
    fn indeterminate_frame_without_stiffness_is_rejected() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].support = SupportKind::Fixed;
        input.members[0].ea = 0.0;
        input.members[0].ei = 0.0;
        assert!(input
            .solve(&model)
            .unwrap_err()
            .contains("статически неопределима"));
    }

    #[test]
    fn determinate_reactions_match_numeric_stiffness_result() {
        let (model, mut input) = beam(4.0);
        input.nodes[0].support = SupportKind::Pin;
        input.nodes[1].support = SupportKind::Roller;
        input.members[0].qy = -5.0;
        let numeric = input.solve(&model).unwrap();
        input.members[0].ea = 0.0;
        input.members[0].ei = 0.0;
        let analytical = input.solve(&model).unwrap();
        assert_eq!(analytical.stiffness_mode, StiffnessMode::Determinate);
        for (a, b) in analytical
            .actual
            .reactions
            .iter()
            .zip(numeric.actual.reactions.iter())
        {
            for (actual, expected) in a.iter().zip(b) {
                near(*actual, *expected);
            }
        }
        near(analytical.actual.members[0].at(2.0).m, 10.0);
    }

    #[test]
    fn bent_cantilever_epures_do_not_depend_on_stiffness() {
        let (mut model, mut input) = beam(2.0);
        model.nodes.push(Node {
            id: "C".into(),
            position: Point { x: 2.0, y: 3.0 },
        });
        model.rods.push(Rod::blank("BC".into(), 1, 2));
        input.reconcile(&model);
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[2].fx = 7.0;
        input.members[0].ea = 900.0;
        input.members[0].ei = 15.0;
        input.members[1].ea = 1700.0;
        input.members[1].ei = 4.0;
        let numeric = input.solve(&model).unwrap();
        for member in &mut input.members {
            member.ea = 0.0;
            member.ei = 0.0;
        }
        let analytical = input.solve(&model).unwrap();
        near(analytical.actual.reactions[0][0], -7.0);
        near(analytical.actual.reactions[0][2], 21.0);
        for (a, b) in analytical
            .actual
            .members
            .iter()
            .zip(&numeric.actual.members)
        {
            for x in [0.0, a.length / 2.0, a.length] {
                let actual = a.at(x);
                let expected = b.at(x);
                near(actual.n, expected.n);
                near(actual.q, expected.q);
                near(actual.m, expected.m);
            }
        }
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-7, "{a} != {b}");
    }

    #[test]
    fn cantilever_tip_force_and_unit_diagrams() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].fy = -5.0;
        let result = input.solve(&model).unwrap();
        near(result.actual.displacements[1][1], -5.0 * 8.0 / (3.0 * 10.0));
        near(result.actual.reactions[0][1], 5.0);
        near(result.actual.reactions[0][2], 10.0);
        near(result.actual.members[0].at(0.0).m, -10.0);
        near(result.actual.members[0].at(2.0).m, 0.0);
        assert_eq!(result.units.len(), 3);
        let uy = result
            .units
            .iter()
            .find(|u| u.component == UnitComponent::Y)
            .unwrap();
        near(uy.result.members[0].at(0.0).m, 2.0);
    }

    #[test]
    fn unit_rotation_is_a_positive_counterclockwise_moment() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].mz = 3.0;
        let result = input.solve(&model).unwrap();
        near(result.actual.displacements[1][2], 3.0 * 2.0 / 10.0);
        near(result.actual.reactions[0][2], -3.0);
        near(result.actual.members[0].at(1.0).m, 3.0);
        let unit = result
            .units
            .iter()
            .find(|u| u.component == UnitComponent::Rotation)
            .unwrap();
        near(unit.result.members[0].at(1.0).m, 1.0);
    }

    #[test]
    fn vertical_member_transforms_global_force_to_local_bending() {
        let (mut model, mut input) = beam(2.0);
        model.nodes[1].position = Point { x: 0.0, y: 2.0 };
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].fx = 5.0;
        let result = input.solve(&model).unwrap();
        near(result.actual.displacements[1][0], 5.0 * 8.0 / (3.0 * 10.0));
        near(result.actual.reactions[0][0], -5.0);
        near(result.actual.reactions[0][2], 10.0);
        near(result.actual.members[0].at(0.0).m, -10.0);
    }

    #[test]
    fn simply_supported_uniform_load() {
        let (model, mut input) = beam(4.0);
        input.nodes[0].support = SupportKind::Pin;
        input.nodes[1].support = SupportKind::Roller;
        input.members[0].qy = -5.0;
        let result = input.solve(&model).unwrap();
        near(result.actual.reactions[0][1], 10.0);
        near(result.actual.reactions[1][1], 10.0);
        near(result.actual.members[0].at(0.0).m, 0.0);
        near(result.actual.members[0].at(2.0).m, 10.0);
        near(result.actual.members[0].at(4.0).m, 0.0);
        near(
            result.actual.displacements[0][2],
            -5.0 * 4.0_f64.powi(3) / (24.0 * 10.0),
        );
    }

    #[test]
    fn unstable_frame_rejected() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Pin;
        assert!(input.solve(&model).unwrap_err().contains("вырождена"));
    }

    #[test]
    fn midspan_point_force_and_axial_force() {
        let (model, mut input) = beam(4.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.members[0].point_loads.push(PointLoad {
            xi: 0.5,
            fx: 2.0,
            fy: -6.0,
            mz: 0.0,
        });
        let result = input.solve(&model).unwrap();
        near(result.actual.reactions[0][0], -2.0);
        near(result.actual.reactions[0][1], 6.0);
        near(result.actual.members[0].at(0.0).n, 2.0);
        near(result.actual.members[0].at(4.0).n, 0.0);
        near(result.actual.members[0].at(0.0).m, -12.0);
        near(result.actual.members[0].at(4.0).m, 0.0);
    }

    #[test]
    fn member_point_moment_creates_moment_jump() {
        let (model, mut input) = beam(4.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.members[0].point_loads.push(PointLoad {
            xi: 0.5,
            fx: 0.0,
            fy: 0.0,
            mz: 3.0,
        });
        let result = input.solve(&model).unwrap();
        near(result.actual.reactions[0][2], -3.0);
        near(result.actual.members[0].at(1.0).m, 3.0);
        near(result.actual.members[0].at(3.0).m, 0.0);
    }

    #[test]
    fn realistic_stiffness_scale_does_not_look_singular() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.members[0].ea = 2.0e11;
        input.members[0].ei = 8.0e6;
        input.nodes[1].fy = -1000.0;
        let result = input.solve(&model).unwrap();
        near(result.actual.reactions[0][1], 1000.0);
        near(
            result.actual.displacements[1][1],
            -1000.0 * 8.0 / (3.0 * 8.0e6),
        );
    }

    #[test]
    fn calculated_epures_transfer_to_mohr_model() {
        let (model, mut input) = beam(4.0);
        input.nodes[0].support = SupportKind::Pin;
        input.nodes[1].support = SupportKind::Roller;
        input.members[0].qy = -5.0;
        let result = input.solve(&model).unwrap();
        let updated = result.apply_to_model(&model, &input).unwrap();
        near(updated.rods[0].load_diagram.midpoint_value(4.0), 10.0);
        near(
            updated.rods[0].unit_y_diagram.midpoint_value(4.0),
            result.units[1].result.members[0].at(2.0).m,
        );
        near(
            updated.rigidity_presets[0].value.numeric_value().unwrap(),
            10.0,
        );
        assert_eq!(model.rods[0].load_diagram, crate::Diagram::default());
    }

    #[test]
    fn transfer_includes_unit_axial_diagram_and_kinematic_reactions() {
        let (mut model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        model.supports.push(SupportMotion {
            node: 0,
            direction: SupportDirection::Horizontal,
            normal_deg: 90.0,
            displacement: 0.03,
            unit_x_reaction: 0.0,
            unit_y_reaction: 0.0,
            unit_rotation_reaction: 0.0,
        });
        let result = input.solve(&model).unwrap();
        let updated = result.apply_to_model(&model, &input).unwrap();
        near(
            updated.rods[0].unit_normal_x_diagram.midpoint_value(2.0),
            1.0,
        );
        let support = updated
            .supports
            .iter()
            .find(|s| s.node == 0 && s.direction == SupportDirection::Horizontal)
            .unwrap();
        near(support.displacement, 0.03);
        near(support.unit_x_reaction, -1.0);
        near(
            crate::calculate_kinematic_displacement_components(&updated)
                .unwrap()
                .horizontal
                .total,
            0.03,
        );
    }

    #[test]
    fn oblique_roller_reaction_is_projected_along_its_normal() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].support = SupportKind::Roller;
        input.nodes[1].normal_deg = 45.0;
        let result = input.solve(&model).unwrap();
        let updated = result.apply_to_model(&model, &input).unwrap();
        let support = updated.supports.iter().find(|s| s.node == 1).unwrap();
        assert_eq!(support.direction, SupportDirection::Normal);
        near(support.normal_deg, 45.0);
        let expected = (result.units[0].result.reactions[1][0]
            + result.units[0].result.reactions[1][1])
            / 2.0_f64.sqrt();
        near(support.unit_x_reaction, expected);
    }

    #[test]
    fn interior_point_load_keeps_transfer_disabled() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Fixed;
        input.members[0].point_loads.push(PointLoad {
            xi: 0.5,
            fx: 0.0,
            fy: -1.0,
            mz: 0.0,
        });
        let result = input.solve(&model).unwrap();
        assert!(result.apply_to_model(&model, &input).is_err());
    }

    #[test]
    fn guided_support_and_horizontal_roller_are_stable() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Guided;
        input.nodes[1].support = SupportKind::Roller;
        input.nodes[1].normal_deg = 0.0;
        input.nodes[0].fx = 5.0;
        let result = input.solve(&model).unwrap();
        assert_eq!(result.constraints, 3);
        near(result.actual.displacements[0][0], 5.0 * 2.0 / 1000.0);
        near(result.actual.reactions[1][0], -5.0);
    }

    #[test]
    fn three_constraints_can_still_leave_a_mechanism() {
        let (model, mut input) = beam(2.0);
        input.nodes[0].support = SupportKind::Pin;
        input.nodes[1].support = SupportKind::Roller;
        input.nodes[1].normal_deg = 0.0;
        assert!(input.solve(&model).unwrap_err().contains("вырождена"));
    }
}

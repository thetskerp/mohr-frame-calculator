//! Отдельное окно для нагрузок, опор, решения МКЭ и просмотра эпюр.

use crate::canvas::{position_epure_label, CanvasCamera, EpureLabelSpec};
use crate::docx_report::FemReportSections;
use eframe::egui::{
    self, Align2, Color32, ComboBox, CursorIcon, DragValue, FontId, PointerButton, Pos2, Rect,
    Response, Sense, Shape, Stroke, Vec2,
};
use mohr_frame::{
    fem::{
        FemProject, FemResult, LoadCaseResult, PointLoad, SectionForces, StiffnessMode, SupportKind,
    },
    format_number, StructuralModel,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReactionDisplay {
    #[default]
    Global,
    Actual,
}

impl ReactionDisplay {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "По глобальным осям",
            Self::Actual => "Фактическая",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Selected {
    #[default]
    Node,
    Member,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FemPage {
    #[default]
    Input,
    Results,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ResultTab {
    #[default]
    Check,
    Reactions,
    Diagrams,
    Details,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Quantity {
    #[default]
    Moment,
    Normal,
    Shear,
}

#[derive(Debug, Clone, Copy)]
struct FemDragOrigin {
    center_offset: [f32; 2],
    screen_scale: f32,
}

#[derive(Debug, Clone)]
struct FemViewState {
    camera: CanvasCamera,
    drag_origin: Option<FemDragOrigin>,
    ordinate_scale: f32,
}

impl Default for FemViewState {
    fn default() -> Self {
        Self {
            camera: CanvasCamera::default(),
            drag_origin: None,
            ordinate_scale: 1.0,
        }
    }
}

impl FemViewState {
    fn interact(
        &mut self,
        ui: &mut egui::Ui,
        model: &StructuralModel,
        rect: Rect,
        response: &Response,
        diagram: bool,
    ) {
        if response.double_clicked_by(PointerButton::Primary) {
            *self = Self::default();
        } else {
            let scale = View::new(model, rect, self.camera).scale as f32;
            if response.drag_started_by(PointerButton::Primary) {
                self.drag_origin = Some(FemDragOrigin {
                    center_offset: self.camera.center_offset,
                    screen_scale: scale,
                });
            }
            if response.dragged_by(PointerButton::Primary) {
                let origin = self.drag_origin.get_or_insert(FemDragOrigin {
                    center_offset: self.camera.center_offset,
                    screen_scale: scale,
                });
                if let Some(delta) = response.total_drag_delta() {
                    self.camera.center_offset = origin.center_offset;
                    self.camera.center_offset[0] -= delta.x / origin.screen_scale;
                    self.camera.center_offset[1] += delta.y / origin.screen_scale;
                }
            }
            if response.drag_stopped_by(PointerButton::Primary) {
                self.drag_origin = None;
            }
        }
        if response.hovered() {
            let (scroll, shift, pointer) = ui.input(|input| {
                (
                    input.smooth_scroll_delta,
                    input.modifiers.shift,
                    input.pointer.hover_pos(),
                )
            });
            let amount = if diagram && shift && scroll.x.abs() > scroll.y.abs() {
                scroll.x
            } else {
                scroll.y
            };
            if amount.abs() > f32::EPSILON {
                let factor = (amount * 0.003).exp();
                if diagram && shift {
                    self.ordinate_scale = (self.ordinate_scale * factor).clamp(0.1, 10.0);
                } else if let Some(pointer) = pointer {
                    let before = View::new(model, rect, self.camera).world_at(pointer);
                    self.camera.zoom = (self.camera.zoom * factor).clamp(0.2, 10.0);
                    let after = View::new(model, rect, self.camera).world_at(pointer);
                    self.camera.center_offset[0] += (before[0] - after[0]) as f32;
                    self.camera.center_offset[1] += (before[1] - after[1]) as f32;
                }
                ui.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FemWindow {
    pub open: bool,
    selected: Selected,
    list_category: Selected,
    selected_node: usize,
    selected_member: usize,
    page: FemPage,
    tab: ResultTab,
    quantity: Quantity,
    case_index: usize,
    result: Option<FemResult>,
    solved_geometry: Option<GeometrySnapshot>,
    message: String,
    error: bool,
    report_sections: FemReportSections,
    pending_report: Option<(FemResult, FemReportSections)>,
    view: FemViewState,
}

#[derive(Debug, Clone, PartialEq)]
struct GeometrySnapshot {
    nodes: Vec<(String, f64, f64)>,
    rods: Vec<(String, usize, usize, bool, bool)>,
}

impl GeometrySnapshot {
    fn of(model: &StructuralModel) -> Self {
        Self {
            nodes: model
                .nodes
                .iter()
                .map(|n| (n.id.clone(), n.position.x, n.position.y))
                .collect(),
            rods: model
                .rods
                .iter()
                .map(|r| {
                    (
                        r.id.clone(),
                        r.start_node,
                        r.end_node,
                        r.release_start_moment,
                        r.release_end_moment,
                    )
                })
                .collect(),
        }
    }
}

impl FemWindow {
    pub fn open_with(&mut self, model: &StructuralModel, input: &mut FemProject) {
        if input.nodes.is_empty() && input.members.is_empty() {
            input.reconcile(model);
        }
        self.selected_node = self.selected_node.min(model.nodes.len().saturating_sub(1));
        self.selected_member = self.selected_member.min(model.rods.len().saturating_sub(1));
        self.list_category = self.selected;
        self.result = None;
        self.solved_geometry = None;
        self.pending_report = None;
        self.page = FemPage::Input;
        self.message = if input.is_aligned(model) {
            String::new()
        } else {
            "Геометрия проекта изменилась. Нажмите «Обновить модель», затем проверьте назначения нагрузок".into()
        };
        self.error = !input.is_aligned(model);
        self.open = true;
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        model: &mut StructuralModel,
        input: &mut FemProject,
        decimals: Option<u8>,
        reaction_display: ReactionDisplay,
    ) -> Option<StructuralModel> {
        if !self.open {
            return None;
        }
        if self.result.is_some()
            && self.solved_geometry.as_ref() != Some(&GeometrySnapshot::of(model))
        {
            self.result = None;
            self.solved_geometry = None;
            self.message = "Геометрия рамы изменилась — повторите расчёт МКЭ".into();
            self.error = true;
        }
        let mut open = self.open;
        let mut transfer = false;
        egui::Window::new("Расчёт рамы МКЭ")
            .open(&mut open)
            .resizable(true)
            .default_size([980.0, 680.0])
            .min_size([520.0, 400.0])
            .show(ctx, |ui| {
                let before = input.clone();
                let before_geometry = GeometrySnapshot::of(model);
                ui.horizontal_wrapped(|ui| {
                    ui.strong("Геометрия из текущего проекта");
                    if !input.is_aligned(model) && ui.button("Обновить модель").clicked()
                    {
                        input.reconcile(model);
                        self.message = "Модель обновлена; проверьте нагрузки и опоры".into();
                        self.error = false;
                    }
                    ui.separator();
                    ui.label("Искомая точка:");
                    let target = input.target_node_id.clone();
                    ComboBox::from_id_salt("fem_target")
                        .selected_text(if target.is_empty() { "—" } else { &target })
                        .show_ui(ui, |ui| {
                            for node in &model.nodes {
                                ui.selectable_value(
                                    &mut input.target_node_id,
                                    node.id.clone(),
                                    &node.id,
                                );
                            }
                        });
                    ui.menu_button("Единичные системы ▾", |ui| {
                        ui.checkbox(&mut input.units.x, "Сила X = 1");
                        ui.checkbox(&mut input.units.y, "Сила Y = 1");
                        ui.checkbox(&mut input.units.rotation, "Момент Mz = 1");
                    });
                    if ui.button("Проверить и рассчитать").clicked() {
                        match input.solve(model) {
                            Ok(result) => {
                                let mode = result.stiffness_mode;
                                self.result = Some(result);
                                self.solved_geometry = Some(GeometrySnapshot::of(model));
                                self.case_index = 0;
                                self.page = FemPage::Results;
                                self.message = if mode == StiffnessMode::Determinate {
                                    "Эпюры и реакции найдены без задания EA и EI; аналитическая жёсткость основного расчёта сохранится".into()
                                } else {
                                    "Эпюры и реакции рассчитаны с заданными EA и EI".into()
                                };
                                self.error = false;
                            }
                            Err(err) => {
                                self.result = None;
                                self.solved_geometry = None;
                                self.message = err;
                                self.error = true;
                            }
                        }
                    }
                });
                if !self.message.is_empty() {
                    ui.colored_label(
                        if self.error {
                            Color32::LIGHT_RED
                        } else {
                            Color32::LIGHT_GREEN
                        },
                        &self.message,
                    );
                }
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.page, FemPage::Input, "Схема и нагрузки");
                    ui.selectable_value(&mut self.page, FemPage::Results, "Результаты МКЭ");
                });
                ui.small("Холст: колесо — масштаб; левая кнопка — перемещение; двойной щелчок — сброс. На эпюре Shift + колесо меняет высоту ординат.");
                ui.separator();
                let sidebar_max = (ui.available_width() - 255.0).max(220.0);
                egui::Panel::left("fem_object_properties")
                    .default_size(300.0)
                    .min_size(220.0)
                    .max_size(sidebar_max)
                    .resizable(true)
                    .show(ui, |ui| self.sidebar(ui, model, input));
                match self.page {
                    FemPage::Input => {
                        let previous_selection =
                            (self.selected, self.selected_node, self.selected_member);
                        draw_model(
                            ui,
                            model,
                            input,
                            &mut self.view,
                            &mut self.selected,
                            &mut self.selected_node,
                            &mut self.selected_member,
                        );
                        if (self.selected, self.selected_node, self.selected_member)
                            != previous_selection
                        {
                            self.list_category = self.selected;
                        }
                    }
                    FemPage::Results => {
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut self.tab, ResultTab::Check, "Проверка модели");
                            ui.selectable_value(&mut self.tab, ResultTab::Reactions, "Реакции");
                            ui.selectable_value(
                                &mut self.tab,
                                ResultTab::Diagrams,
                                "Эпюры M, N, Q",
                            );
                            ui.selectable_value(&mut self.tab, ResultTab::Details, "Ход расчёта");
                            if self.result.is_some()
                                && ui.button("Передать эпюры и реакции").clicked()
                            {
                                transfer = true;
                            }
                            ui.add_enabled_ui(self.result.is_some(), |ui| {
                                ui.menu_button("Экспорт Word ▾", |ui| {
                                    ui.checkbox(
                                        &mut self.report_sections.geometry,
                                        "Геометрия рамы",
                                    );
                                    ui.checkbox(
                                        &mut self.report_sections.loads,
                                        "Нагрузки, опоры, EA и EI",
                                    );
                                    ui.checkbox(
                                        &mut self.report_sections.stability,
                                        "Проверка устойчивости",
                                    );
                                    ui.checkbox(
                                        &mut self.report_sections.actual_diagrams,
                                        "Грузовые эпюры M, N, Q",
                                    );
                                    ui.checkbox(
                                        &mut self.report_sections.unit_diagrams,
                                        "Единичные эпюры M, N",
                                    );
                                    ui.checkbox(
                                        &mut self.report_sections.reactions,
                                        "Реакции грузовой и единичных систем",
                                    );
                                    ui.separator();
                                    if ui.button("Сохранить выбранные разделы").clicked()
                                    {
                                        self.pending_report =
                                            self.result.clone().map(|r| (r, self.report_sections));
                                        ui.close();
                                    }
                                });
                            });
                        });
                        ui.separator();
                        let results_height = ui.available_height();
                        egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.results(ui, model, input, results_height, reaction_display));
                    }
                }
                if *input != before || GeometrySnapshot::of(model) != before_geometry {
                    self.result = None;
                    self.solved_geometry = None;
                    self.message = "Данные изменены — повторите расчёт".into();
                    self.error = false;
                }
            });
        self.open = open;
        if transfer {
            if let Some(result) = &self.result {
                match result.apply_to_model_with_rounding(model, input, decimals) {
                    Ok(updated) => {
                        self.message = if result.stiffness_mode == StiffnessMode::Determinate {
                            "Эпюры и единичные реакции переданы; аналитическое EI сохранено".into()
                        } else {
                            "Эпюры, численные EI и единичные реакции переданы в основной расчёт"
                                .into()
                        };
                        self.error = false;
                        return Some(updated);
                    }
                    Err(err) => {
                        self.message = err;
                        self.error = true;
                    }
                }
            }
        }
        None
    }

    pub fn take_report_request(&mut self) -> Option<(FemResult, FemReportSections)> {
        self.pending_report.take()
    }

    pub fn report_status(&mut self, message: String, error: bool) {
        self.message = message;
        self.error = error;
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, model: &mut StructuralModel, input: &mut FemProject) {
        ui.heading("Объекты рамы");
        ui.horizontal(|ui| {
            if ui
                .selectable_value(
                    &mut self.list_category,
                    Selected::Node,
                    format!("Точки ({})", model.nodes.len()),
                )
                .clicked()
                && !model.nodes.is_empty()
            {
                self.selected = Selected::Node;
                self.selected_node = self.selected_node.min(model.nodes.len() - 1);
            }
            if ui
                .selectable_value(
                    &mut self.list_category,
                    Selected::Member,
                    format!("Стержни ({})", model.rods.len()),
                )
                .clicked()
                && !model.rods.is_empty()
            {
                self.selected = Selected::Member;
                self.selected_member = self.selected_member.min(model.rods.len() - 1);
            }
        });
        let list_height = (ui.available_height() * 0.30).clamp(100.0, 230.0);
        egui::ScrollArea::vertical()
            .id_salt("fem_object_list")
            .max_height(list_height)
            .auto_shrink([false, false])
            .show(ui, |ui| self.object_list(ui, model, input));
        ui.separator();
        ui.heading("Свойства");
        self.properties(ui, model, input);
    }

    fn object_list(&mut self, ui: &mut egui::Ui, model: &StructuralModel, input: &FemProject) {
        if self.list_category == Selected::Node {
            if model.nodes.is_empty() {
                ui.small("Точек нет");
            }
            for (i, node) in model.nodes.iter().enumerate() {
                let data = input.nodes.get(i);
                let has_load = data.is_some_and(|n| n.fx != 0.0 || n.fy != 0.0 || n.mz != 0.0);
                let has_support = data.is_some_and(|n| n.support != SupportKind::Free);
                let mark = if has_support {
                    "◉"
                } else if has_load {
                    "•"
                } else {
                    "○"
                };
                ui.horizontal(|ui| {
                    ui.label(mark);
                    if ui
                        .selectable_label(
                            self.selected == Selected::Node && self.selected_node == i,
                            &node.id,
                        )
                        .clicked()
                    {
                        self.selected = Selected::Node;
                        self.selected_node = i;
                    }
                });
            }
        } else {
            if model.rods.is_empty() {
                ui.small("Стержней нет");
            }
            for (i, rod) in model.rods.iter().enumerate() {
                let has_load = input
                    .members
                    .get(i)
                    .is_some_and(|m| m.qx != 0.0 || m.qy != 0.0 || !m.point_loads.is_empty());
                ui.horizontal(|ui| {
                    ui.label(if has_load { "≋" } else { "─" });
                    if ui
                        .selectable_label(
                            self.selected == Selected::Member && self.selected_member == i,
                            &rod.id,
                        )
                        .clicked()
                    {
                        self.selected = Selected::Member;
                        self.selected_member = i;
                    }
                });
            }
        }
    }
    fn properties(
        &mut self,
        ui: &mut egui::Ui,
        model: &mut StructuralModel,
        input: &mut FemProject,
    ) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    if self.selected == Selected::Node {
                        let Some(node) = input.nodes.get_mut(self.selected_node) else {
                            return;
                        };
                        ui.heading(format!("Узел {}", node.id));
                        ComboBox::from_id_salt("fem_support")
                            .selected_text(node.support.label())
                            .show_ui(ui, |ui| {
                                for kind in [
                                    SupportKind::Free,
                                    SupportKind::Fixed,
                                    SupportKind::Pin,
                                    SupportKind::Roller,
                                    SupportKind::Guided,
                                ] {
                                    ui.selectable_value(&mut node.support, kind, kind.label());
                                }
                            });
                        if matches!(node.support, SupportKind::Roller | SupportKind::Guided) {
                            ui.horizontal(|ui| {
                                ui.label("Нормаль, °");
                                ui.add(DragValue::new(&mut node.normal_deg).speed(1.0));
                            });
                        }
                        ui.separator();
                        ui.strong("Узловые нагрузки");
                        field(ui, "Fₓ", &mut node.fx);
                        field(ui, "Fᵧ", &mut node.fy);
                        field(ui, "Mᶻ", &mut node.mz);
                    } else {
                        let Some(member) = input.members.get_mut(self.selected_member) else {
                            return;
                        };
                        ui.heading(format!("Стержень {}", member.id));
                        if let Some(rod) = model.rods.get_mut(self.selected_member) {
                            ui.checkbox(&mut rod.release_start_moment, "Шарнир в начале");
                            ui.checkbox(&mut rod.release_end_moment, "Шарнир в конце");
                            ui.small(
                                "Один из двух стержней в общей точке должен освобождать момент.",
                            );
                        }
                        ui.small("Для статически определимой рамы EA и EI оставьте пустыми.");
                        ui.collapsing(
                            "Численные жёсткости для неопределимой рамы",
                            |ui| {
                                field(ui, "EA", &mut member.ea);
                                field(ui, "EI", &mut member.ei);
                                if ui.small_button("Сбросить EA и EI").clicked() {
                                    member.ea = 0.0;
                                    member.ei = 0.0;
                                }
                                if let Some(rod) = model.rods.get(self.selected_member) {
                                    if let Some(value) = model
                                        .rigidity_presets
                                        .iter()
                                        .find(|p| Some(p.id) == rod.rigidity_preset_id)
                                        .and_then(|p| {
                                            p.value
                                                .numeric
                                                .then(|| p.value.numeric_value())
                                                .flatten()
                                        })
                                    {
                                        if ui.small_button("Взять числовое EI из таблицы").clicked()
                                        {
                                            member.ei = value;
                                        }
                                    }
                                }
                            },
                        );
                        ui.separator();
                        ui.strong("Распределённая нагрузка");
                        ui.small("Постоянные проекции в глобальных осях");
                        field(ui, "qₓ", &mut member.qx);
                        field(ui, "qᵧ", &mut member.qy);
                        ui.separator();
                        ui.strong("Силы на стержне");
                        if ui.small_button("＋ Сила или момент").clicked() {
                            member.point_loads.push(PointLoad::default());
                        }
                        let mut remove = None;
                        for (i, p) in member.point_loads.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("№{} · x/L", i + 1));
                                ui.add(DragValue::new(&mut p.xi).range(0.0..=1.0).speed(0.01));
                                if ui.small_button("−").clicked() {
                                    remove = Some(i);
                                }
                            });
                            field(ui, "Fₓ", &mut p.fx);
                            field(ui, "Fᵧ", &mut p.fy);
                            field(ui, "Mᶻ", &mut p.mz);
                        }
                        if let Some(i) = remove {
                            member.point_loads.remove(i);
                        }
                    }
                });
            });
    }

    fn results(
        &mut self,
        ui: &mut egui::Ui,
        model: &StructuralModel,
        input: &FemProject,
        viewport_height: f32,
        reaction_display: ReactionDisplay,
    ) {
        let Some(result) = self.result.as_ref() else {
            ui.small("Задайте опоры и нагрузки, затем выполните расчёт. EA и EI нужны только для статически неопределимой рамы.");
            return;
        };
        match self.tab {
            ResultTab::Check => {
                ui.label(format!(
                    "Степеней свободы: {} · связей: {} · свободных: {}",
                    result.total_dof,
                    result.constraints,
                    result.total_dof - result.constraints
                ));
                ui.label("Матрица системы невырождена: рама геометрически неизменяема при заданных опорах.");
                ui.small("Модель: линейный плоский стержневой элемент Эйлера — Бернулли; деформации сдвига не учитываются.");
            }
            ResultTab::Reactions => {
                let content_top = ui.cursor().top();
                ui.horizontal(|ui| {
                    ui.label("Реакции системы:");
                    select_case(ui, result, &mut self.case_index);
                });
                let case = selected_case(result, self.case_index);
                ui.small(match reaction_display {
                    ReactionDisplay::Global => "Стрелки показывают положительные глобальные направления; знак подписи задаёт фактическое направление.",
                    ReactionDisplay::Actual => "Стрелки показывают фактическое направление; подписи содержат модули реакций.",
                });
                let controls_height = ui.cursor().top() - content_top;
                let canvas_height = ((viewport_height - controls_height) * 0.62).max(220.0);
                draw_reactions(
                    ui,
                    model,
                    input,
                    case,
                    canvas_height,
                    &mut self.view,
                    reaction_display,
                );
                ui.separator();
                egui::Grid::new("fem_reactions")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Опора");
                        ui.strong("Rₓ");
                        ui.strong("Rᵧ");
                        ui.strong("Mᶻ");
                        ui.strong("Rₙ");
                        ui.end_row();
                        for ((node, data), r) in
                            model.nodes.iter().zip(&input.nodes).zip(&case.reactions)
                        {
                            ui.label(&node.id);
                            for v in r {
                                ui.monospace(format_number(*v));
                            }
                            if matches!(data.support, SupportKind::Roller | SupportKind::Guided) {
                                let angle = data.normal_deg.to_radians();
                                ui.monospace(format_number(
                                    r[0] * angle.cos() + r[1] * angle.sin(),
                                ));
                            } else {
                                ui.label("—");
                            }
                            ui.end_row();
                        }
                    });
            }
            ResultTab::Diagrams => {
                let content_top = ui.cursor().top();
                ui.horizontal(|ui| {
                    select_case(ui, result, &mut self.case_index);
                    ui.selectable_value(&mut self.quantity, Quantity::Moment, "M");
                    ui.selectable_value(&mut self.quantity, Quantity::Normal, "N");
                    ui.selectable_value(&mut self.quantity, Quantity::Shear, "Q");
                    ui.add(
                        DragValue::new(&mut self.view.ordinate_scale)
                            .range(0.1..=10.0)
                            .speed(0.05)
                            .prefix("Высота ×"),
                    )
                    .on_hover_text("Масштаб ординат; также Shift + колесо над эпюрой");
                });
                if self.case_index > 0 && self.quantity == Quantity::Normal {
                    ui.small("Единичная эпюра N используется в температурном перемещении от средней температуры сечения.");
                }
                let case = selected_case(result, self.case_index);
                let controls_height = ui.cursor().top() - content_top;
                let epure_height = (viewport_height - controls_height - 8.0).max(200.0);
                draw_epure(ui, model, case, self.quantity, epure_height, &mut self.view);
            }
            ResultTab::Details => {
                select_case(ui, result, &mut self.case_index);
                let case = selected_case(result, self.case_index);
                let unit =
                    (self.case_index > 0).then(|| result.units[self.case_index - 1].component);
                ui.heading("Опорные реакции по равновесию узлов");
                ui.small("Для каждой оси: R = Σ концевых усилий стержней − узловая нагрузка. Нулевые слагаемые показаны.");
                for step in case.reaction_steps(model, input, unit) {
                    ui.collapsing(format!("Узел {}", step.node_id), |ui| {
                        for (name, i) in [("Rₓ", 0), ("Rᵧ", 1), ("Mᶻ", 2)] {
                            ui.monospace(format!(
                                "{name} = {} − {} = {}",
                                format_number(step.member_end_forces[i]),
                                format_number(step.applied_load[i]),
                                format_number(step.reaction[i])
                            ));
                        }
                    });
                }
                ui.separator();
                ui.heading("Расчёт эпюр по сечениям стержней");
                ui.small("x отсчитывается от начала стержня; H(x−a) описывает скачок в точке приложения силы или момента.");
                for (rod, member) in model.rods.iter().zip(&case.members) {
                    ui.collapsing(
                        format!("{} · L={}", rod.id, format_number(member.length)),
                        |ui| {
                            for formula in member.calculation_formulas() {
                                ui.monospace(formula);
                            }
                            egui::Grid::new(("fem_detail", &rod.id))
                                .striped(true)
                                .show(ui, |ui| {
                                    for label in ["x", "N", "Q", "M"] {
                                        ui.strong(label);
                                    }
                                    ui.end_row();
                                    for (label, section) in member.calculation_samples() {
                                        ui.label(label);
                                        for value in [section.n, section.q, section.m] {
                                            ui.monospace(format_number(value));
                                        }
                                        ui.end_row();
                                    }
                                });
                        },
                    );
                }
            }
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(DragValue::new(value).speed(0.1));
    });
}
fn case_label(result: &FemResult, index: usize) -> String {
    if index == 0 {
        "Грузовая система".into()
    } else {
        result
            .units
            .get(index - 1)
            .map_or("Грузовая система".into(), |u| {
                format!("Единичная {}", u.component.label())
            })
    }
}

fn selected_case(result: &FemResult, index: usize) -> &LoadCaseResult {
    if index == 0 {
        &result.actual
    } else {
        result
            .units
            .get(index - 1)
            .map_or(&result.actual, |unit| &unit.result)
    }
}

fn select_case(ui: &mut egui::Ui, result: &FemResult, index: &mut usize) {
    ComboBox::from_id_salt("fem_case")
        .selected_text(case_label(result, *index))
        .show_ui(ui, |ui| {
            ui.selectable_value(index, 0, "Грузовая система");
            for (i, unit) in result.units.iter().enumerate() {
                ui.selectable_value(
                    index,
                    i + 1,
                    format!("Единичная {}", unit.component.label()),
                );
            }
        });
}

struct View {
    rect: Rect,
    center: [f64; 2],
    scale: f64,
}
impl View {
    fn new(model: &StructuralModel, rect: Rect, camera: CanvasCamera) -> Self {
        let (mut minx, mut maxx, mut miny, mut maxy) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for n in &model.nodes {
            minx = minx.min(n.position.x);
            maxx = maxx.max(n.position.x);
            miny = miny.min(n.position.y);
            maxy = maxy.max(n.position.y);
        }
        if !minx.is_finite() {
            return Self {
                rect,
                center: [0.0, 0.0],
                scale: 1.0,
            };
        }
        let width = (maxx - minx).max(1.0);
        let height = (maxy - miny).max(1.0);
        let scale =
            ((rect.width() as f64 * 0.72) / width).min((rect.height() as f64 * 0.72) / height);
        Self {
            rect,
            center: [
                (minx + maxx) / 2.0 + camera.center_offset[0] as f64,
                (miny + maxy) / 2.0 + camera.center_offset[1] as f64,
            ],
            scale: scale * camera.zoom as f64,
        }
    }
    fn point(&self, x: f64, y: f64) -> Pos2 {
        Pos2::new(
            self.rect.center().x + ((x - self.center[0]) * self.scale) as f32,
            self.rect.center().y - ((y - self.center[1]) * self.scale) as f32,
        )
    }
    fn world_at(&self, p: Pos2) -> [f64; 2] {
        [
            self.center[0] + (p.x - self.rect.center().x) as f64 / self.scale,
            self.center[1] - (p.y - self.rect.center().y) as f64 / self.scale,
        ]
    }
}

fn draw_model(
    ui: &mut egui::Ui,
    model: &StructuralModel,
    input: &FemProject,
    view_state: &mut FemViewState,
    selected: &mut Selected,
    selected_node: &mut usize,
    selected_member: &mut usize,
) {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(
        Vec2::new(available.x.max(180.0), available.y.max(180.0)),
        Sense::click_and_drag(),
    );
    let cursor = if response.dragged() {
        CursorIcon::Grabbing
    } else {
        CursorIcon::Grab
    };
    let response = response.on_hover_cursor(cursor);
    let rect = response.rect.shrink(8.0);
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 25, 34));
    let view_rect = rect.shrink(18.0);
    view_state.interact(ui, model, view_rect, &response, false);
    let view = View::new(model, view_rect, view_state.camera);
    let mut closest = (f32::INFINITY, None);
    let click = response.interact_pointer_pos();
    for (i, rod) in model.rods.iter().enumerate() {
        let (Some(a), Some(b)) = (
            model.nodes.get(rod.start_node),
            model.nodes.get(rod.end_node),
        ) else {
            continue;
        };
        let start = view.point(a.position.x, a.position.y);
        let end = view.point(b.position.x, b.position.y);
        let color = if *selected == Selected::Member && *selected_member == i {
            Color32::from_rgb(255, 200, 85)
        } else {
            Color32::LIGHT_GRAY
        };
        painter.line_segment([start, end], Stroke::new(3.0, color));
        painter.text(
            start.lerp(end, 0.5) + Vec2::new(0.0, -12.0),
            Align2::CENTER_CENTER,
            &rod.id,
            FontId::proportional(12.0),
            Color32::LIGHT_GRAY,
        );
        if let Some(p) = click {
            let d = segment_distance(p, start, end);
            if d < closest.0 {
                closest = (d, Some((Selected::Member, i)));
            }
        }
        if let Some(load) = input.members.get(i) {
            if load.qx != 0.0 || load.qy != 0.0 {
                let direction = Vec2::new(load.qx as f32, -load.qy as f32).normalized();
                for t in [0.2, 0.4, 0.6, 0.8] {
                    let tip = start.lerp(end, t);
                    draw_arrow(
                        &painter,
                        tip - direction * 27.0,
                        tip,
                        Color32::from_rgb(235, 160, 105),
                    );
                }
                painter.text(
                    start.lerp(end, 0.5) + Vec2::new(0.0, 16.0),
                    Align2::CENTER_CENTER,
                    format!(
                        "qₓ={}, qᵧ={}",
                        format_number(load.qx),
                        format_number(load.qy)
                    ),
                    FontId::proportional(11.0),
                    Color32::from_rgb(235, 160, 105),
                );
            }
            for p in &load.point_loads {
                let tip = start.lerp(end, p.xi as f32);
                if p.fx != 0.0 || p.fy != 0.0 {
                    let direction = Vec2::new(p.fx as f32, -p.fy as f32).normalized();
                    draw_arrow(
                        &painter,
                        tip - direction * 32.0,
                        tip,
                        Color32::from_rgb(235, 160, 105),
                    );
                }
                if p.mz != 0.0 {
                    draw_moment_arrow(
                        &painter,
                        rect,
                        tip,
                        p.mz,
                        Color32::from_rgb(235, 160, 105),
                        None,
                    );
                }
            }
        }
    }
    for (i, node) in model.nodes.iter().enumerate() {
        let p = view.point(node.position.x, node.position.y);
        let color = if *selected == Selected::Node && *selected_node == i {
            Color32::YELLOW
        } else {
            Color32::WHITE
        };
        if model.is_internal_hinge_node(i) {
            painter.circle_filled(p, 7.0, Color32::from_rgb(18, 25, 34));
            painter.circle_stroke(p, 7.0, Stroke::new(2.0, color));
        } else {
            painter.circle_filled(p, 5.0, color);
        }
        painter.text(
            p + Vec2::new(9.0, -9.0),
            Align2::LEFT_CENTER,
            &node.id,
            FontId::proportional(13.0),
            color,
        );
        if let Some(data) = input.nodes.get(i) {
            if data.support != SupportKind::Free {
                draw_support_symbol(
                    &painter,
                    p,
                    data.support,
                    data.normal_deg,
                    support_outward(model, i, &view),
                );
            }
            if data.fx != 0.0 || data.fy != 0.0 {
                let direction = Vec2::new(data.fx as f32, -data.fy as f32).normalized();
                draw_arrow(
                    &painter,
                    p - direction * 38.0,
                    p,
                    Color32::from_rgb(235, 160, 105),
                );
                painter.text(
                    p + Vec2::new(0.0, -25.0),
                    Align2::CENTER_CENTER,
                    format!(
                        "Fₓ={}, Fᵧ={}",
                        format_number(data.fx),
                        format_number(data.fy)
                    ),
                    FontId::proportional(10.0),
                    Color32::from_rgb(235, 160, 105),
                );
            }
            if data.mz != 0.0 {
                draw_moment_arrow(
                    &painter,
                    rect,
                    p,
                    data.mz,
                    Color32::from_rgb(235, 160, 105),
                    None,
                );
            }
        }
        if let Some(click) = click {
            let d = click.distance(p);
            if d < closest.0 {
                closest = (d, Some((Selected::Node, i)));
            }
        }
    }
    if response.clicked() {
        if let Some((kind, index)) = closest.1 {
            if closest.0 < 16.0 {
                *selected = kind;
                match kind {
                    Selected::Node => *selected_node = index,
                    Selected::Member => *selected_member = index,
                }
            }
        }
    }
}

fn support_outward(model: &StructuralModel, index: usize, view: &View) -> Vec2 {
    let p = view.point(model.nodes[index].position.x, model.nodes[index].position.y);
    let outward = model
        .rods
        .iter()
        .filter_map(|rod| {
            let other = if rod.start_node == index {
                rod.end_node
            } else if rod.end_node == index {
                rod.start_node
            } else {
                return None;
            };
            let neighbor = model.nodes.get(other)?;
            Some(p - view.point(neighbor.position.x, neighbor.position.y))
        })
        .fold(Vec2::ZERO, |sum, direction| sum + direction);
    if outward.length_sq() > 1e-6 {
        outward.normalized()
    } else {
        Vec2::new(-1.0, 0.0)
    }
}

fn draw_reactions(
    ui: &mut egui::Ui,
    model: &StructuralModel,
    input: &FemProject,
    result: &LoadCaseResult,
    height: f32,
    view_state: &mut FemViewState,
    display: ReactionDisplay,
) {
    let width = ui.available_width();
    let (response, painter) =
        ui.allocate_painter(Vec2::new(width, height), Sense::click_and_drag());
    let cursor = if response.dragged() {
        CursorIcon::Grabbing
    } else {
        CursorIcon::Grab
    };
    let response = response.on_hover_cursor(cursor);
    let rect = response.rect.shrink(7.0);
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 25, 34));
    let view_rect = rect.shrink2(Vec2::new(62.0, 42.0));
    view_state.interact(ui, model, view_rect, &response, false);
    let view = View::new(model, view_rect, view_state.camera);
    let reaction_color = Color32::from_rgb(85, 225, 235);
    let max_reaction = result
        .reactions
        .iter()
        .flat_map(|r| r.iter())
        .fold(0.0_f64, |max, value| max.max(value.abs()));
    let visible = |value: f64| value.abs() > (max_reaction * 1e-8).max(1e-10);

    for rod in &model.rods {
        let (Some(a), Some(b)) = (
            model.nodes.get(rod.start_node),
            model.nodes.get(rod.end_node),
        ) else {
            continue;
        };
        painter.line_segment(
            [
                view.point(a.position.x, a.position.y),
                view.point(b.position.x, b.position.y),
            ],
            Stroke::new(2.5, Color32::LIGHT_GRAY),
        );
    }
    for (i, ((node, data), reaction)) in model
        .nodes
        .iter()
        .zip(&input.nodes)
        .zip(&result.reactions)
        .enumerate()
    {
        let p = view.point(node.position.x, node.position.y);
        if model.is_internal_hinge_node(i) {
            painter.circle_filled(p, 7.0, Color32::from_rgb(18, 25, 34));
            painter.circle_stroke(p, 7.0, Stroke::new(2.0, Color32::WHITE));
        } else {
            painter.circle_filled(p, 4.0, Color32::WHITE);
        }
        painter.text(
            p + Vec2::new(9.0, -10.0),
            Align2::LEFT_CENTER,
            &node.id,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
        if data.support == SupportKind::Free {
            continue;
        }
        draw_support_symbol(
            &painter,
            p,
            data.support,
            data.normal_deg,
            support_outward(model, i, &view),
        );
        if matches!(data.support, SupportKind::Roller | SupportKind::Guided) {
            let angle = data.normal_deg.to_radians();
            let normal = reaction[0] * angle.cos() + reaction[1] * angle.sin();
            if visible(normal) {
                let axis = Vec2::new(angle.cos() as f32, -angle.sin() as f32);
                let direction = if display == ReactionDisplay::Actual {
                    axis * normal.signum() as f32
                } else {
                    axis
                };
                draw_arrow(
                    &painter,
                    p - direction * 52.0,
                    p - direction * 10.0,
                    reaction_color,
                );
                painter.text(
                    p - direction * 58.0 + Vec2::new(0.0, -10.0),
                    Align2::CENTER_CENTER,
                    if display == ReactionDisplay::Actual {
                        format!("|Rₙ|={}", format_number(normal.abs()))
                    } else {
                        format!("Rₙ={}", format_number(normal))
                    },
                    FontId::proportional(11.0),
                    reaction_color,
                );
            }
        } else {
            for (value, axis, label, offset) in [
                (reaction[0], Vec2::X, "Rₓ", Vec2::new(0.0, -12.0)),
                (
                    reaction[1],
                    Vec2::new(0.0, -1.0),
                    "Rᵧ",
                    Vec2::new(18.0, 0.0),
                ),
            ] {
                if visible(value) {
                    let direction = if display == ReactionDisplay::Actual {
                        axis * value.signum() as f32
                    } else {
                        axis
                    };
                    draw_arrow(
                        &painter,
                        p - direction * 48.0,
                        p - direction * 10.0,
                        reaction_color,
                    );
                    painter.text(
                        p - direction * 55.0 + offset,
                        Align2::CENTER_CENTER,
                        if display == ReactionDisplay::Actual {
                            format!("|{label}|={}", format_number(value.abs()))
                        } else {
                            format!("{label}={}", format_number(value))
                        },
                        FontId::proportional(11.0),
                        reaction_color,
                    );
                }
            }
        }
        if visible(reaction[2]) {
            draw_moment_arrow(
                &painter,
                rect,
                p,
                reaction[2],
                reaction_color,
                Some(display),
            );
        }
    }
}

/// Условные обозначения опор: заделка, шарнир на основании, шарнир на роликах
/// и ползун в направляющих. Нормаль ролика/ползуна совпадает со связью МКЭ.
fn draw_support_symbol(
    painter: &egui::Painter,
    p: Pos2,
    kind: SupportKind,
    normal_deg: f64,
    outward: Vec2,
) {
    let color = Color32::from_rgb(115, 185, 245);
    let stroke = Stroke::new(1.7, color);
    if kind == SupportKind::Fixed {
        let wall = p + outward * 9.0;
        let tangent = Vec2::new(-outward.y, outward.x);
        painter.line_segment(
            [wall - tangent * 15.0, wall + tangent * 15.0],
            Stroke::new(3.0, color),
        );
        for y in [-14.0, -7.0, 0.0, 7.0, 14.0] {
            painter.line_segment(
                [
                    wall + tangent * y,
                    wall + outward * 8.0 + tangent * (y + 6.0),
                ],
                stroke,
            );
        }
        return;
    }
    // Локальная ось v направлена от узла к основанию, ось u — вдоль него.
    let a = if kind == SupportKind::Pin {
        90.0
    } else {
        normal_deg
    }
    .to_radians() as f32;
    let v = Vec2::new(-a.cos(), a.sin());
    let u = Vec2::new(v.y, -v.x);
    let at = |along: f32, away: f32| p + u * along + v * away;
    let ground = |distance: f32| {
        painter.line_segment([at(-15.0, distance), at(15.0, distance)], stroke);
        for s in [-12.0, -6.0, 0.0, 6.0, 12.0] {
            painter.line_segment(
                [at(s, distance), at(s - 5.0, distance + 6.0)],
                Stroke::new(1.0, color),
            );
        }
    };
    match kind {
        SupportKind::Pin | SupportKind::Roller => {
            painter.add(Shape::convex_polygon(
                vec![at(0.0, 3.0), at(-11.0, 15.0), at(11.0, 15.0)],
                Color32::from_rgb(18, 25, 34),
                stroke,
            ));
            if kind == SupportKind::Roller {
                for side in [-6.0, 6.0] {
                    painter.circle(at(side, 18.0), 2.5, Color32::from_rgb(18, 25, 34), stroke);
                }
                ground(22.0);
            } else {
                ground(16.0);
            }
        }
        SupportKind::Guided => {
            // Ползун ограничивает движение по нормали и поворот, но
            // сохраняет движение вдоль направляющей.
            painter.line_segment([at(-8.0, 5.0), at(8.0, 5.0)], Stroke::new(3.0, color));
            painter.line_segment([at(-8.0, 5.0), at(-8.0, 13.0)], stroke);
            painter.line_segment([at(8.0, 5.0), at(8.0, 13.0)], stroke);
            ground(15.0);
        }
        SupportKind::Free | SupportKind::Fixed => {}
    }
}

fn draw_arrow(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32) {
    let direction = (end - start).normalized();
    let side = Vec2::new(-direction.y, direction.x);
    painter.line_segment([start, end], Stroke::new(1.7, color));
    painter.line_segment(
        [end, end - direction * 8.0 + side * 4.0],
        Stroke::new(1.7, color),
    );
    painter.line_segment(
        [end, end - direction * 8.0 - side * 4.0],
        Stroke::new(1.7, color),
    );
}

/// Положительный Mᶻ поворачивает против часовой стрелки в плоскости рамы.
/// Координата Y на экране направлена вниз, поэтому её знак меняется.
fn moment_arc_points(center: Pos2, moment: f64) -> Vec<Pos2> {
    let direction = if moment > 0.0 { 1.0 } else { -1.0 };
    let start = -direction * 3.0 * std::f32::consts::PI / 4.0;
    let sweep = direction * 3.0 * std::f32::consts::PI / 2.0;
    let radius = 20.0;
    (0..=32)
        .map(|step| {
            let angle = start + sweep * step as f32 / 32.0;
            center + Vec2::new(radius * angle.cos(), -radius * angle.sin())
        })
        .collect()
}

fn draw_moment_arrow(
    painter: &egui::Painter,
    bounds: Rect,
    center: Pos2,
    moment: f64,
    color: Color32,
    display: Option<ReactionDisplay>,
) {
    let direction_moment = if display == Some(ReactionDisplay::Global) {
        1.0
    } else {
        moment
    };
    let curve = moment_arc_points(center, direction_moment);
    let end = curve[curve.len() - 1];
    let tangent = (end - curve[curve.len() - 2]).normalized();
    let side = Vec2::new(-tangent.y, tangent.x);
    let stroke = Stroke::new(2.0, color);
    painter.add(Shape::line(curve, stroke));
    painter.line_segment([end, end - tangent * 9.0 + side * 4.0], stroke);
    painter.line_segment([end, end - tangent * 9.0 - side * 4.0], stroke);
    let on_right = center.x + 100.0 <= bounds.max.x;
    let preferred_y = if direction_moment > 0.0 { -27.0 } else { 27.0 };
    let label_y = if center.y + preferred_y < bounds.min.y + 12.0
        || center.y + preferred_y > bounds.max.y - 12.0
    {
        -preferred_y
    } else {
        preferred_y
    };
    painter.text(
        center + Vec2::new(if on_right { 27.0 } else { -27.0 }, label_y),
        if on_right {
            Align2::LEFT_CENTER
        } else {
            Align2::RIGHT_CENTER
        },
        if display == Some(ReactionDisplay::Actual) {
            format!("|Mᶻ|={}", format_number(moment.abs()))
        } else {
            format!("Mᶻ={}", format_number(moment))
        },
        FontId::proportional(11.0),
        color,
    );
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let d = b - a;
    let len = d.length_sq();
    if len < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(d) / len).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

fn section_value(s: SectionForces, quantity: Quantity) -> f64 {
    match quantity {
        Quantity::Moment => s.m,
        Quantity::Normal => s.n,
        Quantity::Shear => s.q,
    }
}

fn draw_epure(
    ui: &mut egui::Ui,
    model: &StructuralModel,
    result: &LoadCaseResult,
    quantity: Quantity,
    height: f32,
    view_state: &mut FemViewState,
) {
    let width = ui.available_width();
    let (response, painter) =
        ui.allocate_painter(Vec2::new(width, height), Sense::click_and_drag());
    let cursor = if response.dragged() {
        CursorIcon::Grabbing
    } else {
        CursorIcon::Grab
    };
    let response = response.on_hover_cursor(cursor);
    let rect = response.rect.shrink(7.0);
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 25, 34));
    let view_rect = rect.shrink(22.0);
    view_state.interact(ui, model, view_rect, &response, true);
    let view = View::new(model, view_rect, view_state.camera);
    let max = result
        .members
        .iter()
        .flat_map(|member| {
            (0..=64).map(move |i| {
                section_value(member.at(member.length * i as f64 / 64.0), quantity).abs()
            })
        })
        .fold(0.0_f64, f64::max)
        .max(1e-12);
    let scale = rect.height() * 0.21 * view_state.ordinate_scale / max as f32;
    let color = match quantity {
        Quantity::Moment => Color32::from_rgb(245, 155, 85),
        Quantity::Normal => Color32::from_rgb(190, 135, 235),
        Quantity::Shear => Color32::from_rgb(235, 205, 90),
    };
    let mut line_obstacles = Vec::new();
    let mut labels = Vec::new();
    for (rod, member) in model.rods.iter().zip(&result.members) {
        let a = model.nodes[rod.start_node].position;
        let b = model.nodes[rod.end_node].position;
        let start = view.point(a.x, a.y);
        let end = view.point(b.x, b.y);
        let d = end - start;
        let normal = Vec2::new(-d.y, d.x) / d.length().max(1.0);
        let tangent = d / d.length().max(1.0);
        let mut samples = (0..=64).map(|i| i as f64 / 64.0).collect::<Vec<_>>();
        for point in &member.point_loads_local {
            samples.push((point.xi - 1e-8).max(0.0));
            samples.push((point.xi + 1e-8).min(1.0));
        }
        samples.sort_by(|a, b| a.total_cmp(b));
        let curve = samples
            .into_iter()
            .map(|t| {
                let ordinate = section_value(member.at(t * member.length), quantity) as f32;
                start.lerp(end, t as f32) + normal * ordinate * scale
            })
            .collect::<Vec<_>>();
        painter.line_segment([start, end], Stroke::new(2.0, Color32::LIGHT_GRAY));
        line_obstacles.push([start, end]);
        let stroke = Stroke::new(2.2, color);
        if let (Some(first), Some(last)) = (curve.first(), curve.last()) {
            // Замыкаем крайние ординаты на узлы стержня.
            painter.line_segment([start, *first], stroke);
            painter.line_segment([*last, end], stroke);
            line_obstacles.push([start, *first]);
            line_obstacles.push([*last, end]);
        }
        line_obstacles.extend(curve.windows(2).map(|pair| [pair[0], pair[1]]));
        painter.add(Shape::line(curve, stroke));
        for t in [0.0_f64, 0.5, 1.0] {
            let ordinate = section_value(member.at(t * member.length), quantity);
            let pos = start.lerp(end, t as f32) + normal * ordinate as f32 * scale;
            painter.circle_filled(pos, 2.5, color);
            labels.push((
                pos,
                normal,
                tangent,
                ordinate,
                t as f32,
                format_number(ordinate),
            ));
        }
    }
    let mut placed = Vec::new();
    let font = FontId::monospace(10.0);
    for (anchor, normal, tangent, value, fraction, text) in labels {
        let size = painter
            .layout_no_wrap(text.clone(), font.clone(), color)
            .size();
        let center = position_epure_label(
            EpureLabelSpec {
                anchor,
                normal,
                tangent,
                value,
                fraction,
                text_size: size,
            },
            rect,
            &line_obstacles,
            &placed,
        );
        painter.text(center, Align2::CENTER_CENTER, text, font.clone(), color);
        placed.push(Rect::from_center_size(center, size).expand(3.0));
    }
    for (index, node) in model.nodes.iter().enumerate() {
        if model.is_internal_hinge_node(index) {
            let p = view.point(node.position.x, node.position.y);
            painter.circle_filled(p, 7.0, Color32::from_rgb(18, 25, 34));
            painter.circle_stroke(p, 7.0, Stroke::new(2.0, Color32::WHITE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epure_value_label_stays_clear_of_member_and_neighbor_line() {
        let bounds = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(220.0, 200.0));
        let lines = vec![
            [Pos2::new(100.0, 20.0), Pos2::new(100.0, 180.0)],
            [Pos2::new(150.0, 20.0), Pos2::new(150.0, 180.0)],
        ];
        let size = Vec2::new(36.0, 14.0);
        let center = position_epure_label(
            EpureLabelSpec {
                anchor: Pos2::new(100.0, 100.0),
                normal: Vec2::X,
                tangent: Vec2::Y,
                value: 1.0,
                fraction: 0.5,
                text_size: size,
            },
            bounds,
            &lines,
            &[],
        );
        let label = Rect::from_center_size(center, size).expand(2.0);
        assert!(bounds.contains_rect(label));
        assert!(lines
            .iter()
            .all(|line| !crate::canvas::segment_intersects_rect(*line, label)));
    }

    #[test]
    fn epure_value_label_moves_to_other_side_when_first_side_is_blocked() {
        let bounds = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(220.0, 200.0));
        let lines = vec![
            [Pos2::new(100.0, 20.0), Pos2::new(100.0, 180.0)],
            [Pos2::new(128.0, 20.0), Pos2::new(128.0, 180.0)],
        ];
        let size = Vec2::new(36.0, 14.0);
        let center = position_epure_label(
            EpureLabelSpec {
                anchor: Pos2::new(100.0, 100.0),
                normal: Vec2::X,
                tangent: Vec2::Y,
                value: 1.0,
                fraction: 0.5,
                text_size: size,
            },
            bounds,
            &lines,
            &[],
        );
        let label = Rect::from_center_size(center, size).expand(2.0);
        assert!(center.x < 100.0);
        assert!(lines
            .iter()
            .all(|line| !crate::canvas::segment_intersects_rect(*line, label)));
    }

    #[test]
    fn moment_arrows_follow_the_sign_convention() {
        let center = Pos2::new(100.0, 100.0);
        let cross = |points: &[Pos2]| {
            let a = points[0] - center;
            let b = points[1] - center;
            a.x * b.y - a.y * b.x
        };
        assert!(cross(&moment_arc_points(center, 1.0)) < 0.0);
        assert!(cross(&moment_arc_points(center, -1.0)) > 0.0);
    }
}

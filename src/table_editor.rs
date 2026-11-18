//! Табличный ввод геометрии с предварительным просмотром.

use eframe::egui::{self, Color32, ComboBox, DragValue};
use mohr_frame::{next_node_id, Node, Point, Rod, StructuralModel};

use crate::canvas::{frame_canvas, CanvasOptions, CanvasViewState};

#[derive(Debug, Clone, Default)]
pub struct TableEditor {
    pub open: bool,
    draft: Option<StructuralModel>,
    preview_view: CanvasViewState,
    message: String,
}

impl TableEditor {
    pub fn open_with(&mut self, model: &StructuralModel) {
        self.draft = Some(model.clone());
        self.preview_view.reset();
        self.message.clear();
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.draft = None;
        self.message.clear();
    }

    /// Возвращает модель только после проверки и нажатия «Применить».
    pub fn show(&mut self, context: &egui::Context) -> Option<StructuralModel> {
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Табличное задание точек и стержней")
            .open(&mut open)
            .resizable(true)
            .default_size([920.0, 620.0])
            .show(context, |ui| {
                let Some(draft) = self.draft.as_mut() else {
                    return;
                };
                let previous_nodes = draft.nodes.clone();
                let previous_rods = draft.rods.iter().map(|rod| (rod.id.clone(), rod.start_node, rod.end_node)).collect::<Vec<_>>();
                ui.small("№ — номер строки; обозначения точек и стержней можно менять. Предварительный вид обновляется сразу.");
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                        ui.heading("1. Точки");
                        let mut count = draft.nodes.len();
                        ui.horizontal(|ui| {
                            ui.label("Количество точек:");
                            ui.add(DragValue::new(&mut count).range(0..=200));
                        });
                        if count > draft.nodes.len() {
                            while draft.nodes.len() < count {
                                let x = draft.nodes.last().map_or(0.0, |node| node.position.x + 2.0);
                                let id = next_node_id(&draft.nodes);
                                draft.nodes.push(Node {
                                    id,
                                    position: Point { x, y: 0.0 },
                                });
                            }
                        } else if count < draft.nodes.len() {
                            let used = draft.rods.iter().any(|rod| rod.start_node >= count || rod.end_node >= count)
                                || draft.supports.iter().any(|support| support.node >= count);
                            if used {
                                self.message = "Сначала измените стержни или опоры, связанные с удаляемыми точками".into();
                            } else {
                                draft.nodes.truncate(count);
                                draft.target_node = draft.target_node.min(count.saturating_sub(1));
                                self.message.clear();
                            }
                        }
                        egui::Grid::new("table_nodes").striped(true).show(ui, |ui| {
                            ui.strong("№"); ui.strong("Точка"); ui.strong("x"); ui.strong("y"); ui.end_row();
                            for (index, node) in draft.nodes.iter_mut().enumerate() {
                                ui.label((index + 1).to_string());
                                ui.add(egui::TextEdit::singleline(&mut node.id).desired_width(65.0));
                                ui.add(DragValue::new(&mut node.position.x).speed(0.1));
                                ui.add(DragValue::new(&mut node.position.y).speed(0.1));
                                ui.end_row();
                            }
                        });
                        ui.separator();
                        ui.heading("2. Стержни");
                        let mut rod_count = draft.rods.len();
                        ui.horizontal(|ui| {
                            ui.label("Количество стержней:");
                            ui.add(DragValue::new(&mut rod_count).range(0..=300));
                        });
                        if rod_count > draft.rods.len() {
                            if draft.nodes.len() < 2 {
                                self.message = "Для стержня нужны хотя бы две точки".into();
                            } else {
                                while draft.rods.len() < rod_count {
                                    let Some((start, end)) = draft.next_free_rod_endpoints() else {
                                        self.message = "Все пары точек уже соединены стержнями".into();
                                        break;
                                    };
                                    let id = draft.unique_rod_id(start, end).expect("выбраны существующие точки");
                                    let mut rod = Rod::blank(id, start, end);
                                    rod.rigidity_preset_id = draft.rigidity_presets.first().map(|item| item.id);
                                    rod.thermal_preset_id = draft.thermal_presets.first().map(|item| item.id);
                                    draft.rods.push(rod);
                                }
                            }
                        } else if rod_count < draft.rods.len() {
                            draft.rods.truncate(rod_count);
                            self.message.clear();
                        }
                        egui::Grid::new("table_rods").striped(true).show(ui, |ui| {
                            ui.strong("№"); ui.strong("Стержень"); ui.strong("Начало"); ui.strong("Конец"); ui.strong("L"); ui.end_row();
                            for (index, rod) in draft.rods.iter_mut().enumerate() {
                                ui.label((index + 1).to_string());
                                ui.add(egui::TextEdit::singleline(&mut rod.id).desired_width(65.0));
                                node_selector(ui, (index, 0), &mut rod.start_node, &draft.nodes);
                                node_selector(ui, (index, 1), &mut rod.end_node, &draft.nodes);
                                let length = draft.nodes.get(rod.start_node).zip(draft.nodes.get(rod.end_node))
                                    .map(|(a, b)| (a.position.x - b.position.x).hypot(a.position.y - b.position.y));
                                ui.label(length.map_or_else(|| "—".into(), mohr_frame::format_number));
                                ui.end_row();
                            }
                        });
                    });
                    draft.refresh_generated_rod_ids(&previous_nodes, &previous_rods);
                    frame_canvas(
                        &mut columns[1],
                        draft,
                        CanvasOptions {
                            title: "Предварительный вид рамы",
                            diagram: None,
                            height: 500.0,
                            show_values: false,
                            show_dimensions: true,
                            show_temperatures: false,
                        },
                        &mut self.preview_view,
                    );
                });
                if let Err(error) = draft.validate_unique_geometry() {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                if !self.message.is_empty() {
                    ui.colored_label(Color32::YELLOW, &self.message);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Применить").clicked() { apply = true; }
                    if ui.button("Отмена").clicked() { cancel = true; }
                });
            });
        self.open = open && !cancel;
        if !self.open {
            self.close();
            return None;
        }
        if apply {
            if let Some(draft) = self.draft.as_ref() {
                match draft.validate() {
                    Ok(()) => {
                        let result = draft.clone();
                        self.close();
                        return Some(result);
                    }
                    Err(error) => self.message = error,
                }
            }
        }
        None
    }
}

fn node_selector(ui: &mut egui::Ui, key: (usize, usize), selected: &mut usize, nodes: &[Node]) {
    let label = nodes.get(*selected).map_or("—", |node| node.id.as_str());
    ComboBox::from_id_salt(("table_node", key))
        .selected_text(label)
        .width(70.0)
        .show_ui(ui, |ui| {
            for (index, node) in nodes.iter().enumerate() {
                ui.selectable_value(selected, index, &node.id);
            }
        });
}

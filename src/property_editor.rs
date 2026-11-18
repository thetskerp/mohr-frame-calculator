//! Табличное задание независимых жёсткостей EI и коэффициентов α.

use eframe::egui::{self, Color32, ComboBox, DragValue};
use mohr_frame::{PropertyPreset, PropertyValue, StructuralModel};

#[derive(Debug, Clone, Default)]
pub struct PropertyEditor {
    pub open: bool,
    draft: Option<StructuralModel>,
    message: String,
}

impl PropertyEditor {
    pub fn open_with(&mut self, model: &StructuralModel) {
        self.draft = Some(model.clone());
        self.message.clear();
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.draft = None;
        self.message.clear();
    }

    pub fn show(&mut self, context: &egui::Context) -> Option<StructuralModel> {
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Табличное задание свойств стержней")
            .open(&mut open)
            .resizable(true)
            .default_size([1120.0, 680.0])
            .show(context, |ui| {
                let Some(draft) = self.draft.as_mut() else { return; };
                ui.small("Без галочки вводится коэффициент перед EI или α. С галочкой — полное число: мантисса × 10^(степень).");
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                        preset_table(ui, "EI", "Жёсткость", &mut draft.rigidity_presets, &draft.rods.iter().filter_map(|rod| rod.rigidity_preset_id).collect::<Vec<_>>(), &mut self.message);
                    });
                    egui::ScrollArea::vertical().show(&mut columns[1], |ui| {
                        preset_table(ui, "α", "Расширение", &mut draft.thermal_presets, &draft.rods.iter().filter_map(|rod| rod.thermal_preset_id).collect::<Vec<_>>(), &mut self.message);
                    });
                });
                ui.separator();
                ui.strong("Назначение стержням");
                let rigidity = draft.rigidity_presets.iter().map(|preset| (preset.id, preset.name.clone())).collect::<Vec<_>>();
                let thermal = draft.thermal_presets.iter().map(|preset| (preset.id, preset.name.clone())).collect::<Vec<_>>();
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    egui::Grid::new("property_assignments").striped(true).show(ui, |ui| {
                        ui.strong("Стержень"); ui.strong("Жёсткость EI"); ui.strong("Коэффициент α"); ui.end_row();
                        for (index, rod) in draft.rods.iter_mut().enumerate() {
                            ui.label(&rod.id);
                            preset_selector(ui, (index, 0), &mut rod.rigidity_preset_id, &rigidity);
                            preset_selector(ui, (index, 1), &mut rod.thermal_preset_id, &thermal);
                            ui.end_row();
                        }
                    });
                });
                if !self.message.is_empty() { ui.colored_label(Color32::LIGHT_RED, &self.message); }
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
                let mut result = draft.clone();
                for preset in &mut result.rigidity_presets {
                    if preset.name.trim().is_empty() {
                        preset.name = format!("Жёсткость {}", preset.id);
                    }
                }
                for preset in &mut result.thermal_presets {
                    if preset.name.trim().is_empty() {
                        preset.name = format!("Расширение {}", preset.id);
                    }
                }
                match result.validate() {
                    Ok(()) => {
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

fn preset_table(
    ui: &mut egui::Ui,
    symbol: &str,
    prefix: &str,
    items: &mut Vec<PropertyPreset>,
    used: &[u32],
    message: &mut String,
) {
    ui.heading(if symbol == "EI" {
        "Жёсткости EI"
    } else {
        "Коэффициенты α"
    });
    let mut count = items.len();
    ui.horizontal(|ui| {
        ui.label("Количество строк:");
        ui.add(DragValue::new(&mut count).range(1..=100));
    });
    while items.len() < count {
        let id = items.iter().map(|item| item.id).max().unwrap_or(0) + 1;
        let value = PropertyValue {
            exponent: if symbol == "α" { -5 } else { 0 },
            ..PropertyValue::default()
        };
        items.push(PropertyPreset {
            id,
            name: format!("{prefix} {id}"),
            value,
        });
    }
    if count < items.len() {
        if items[count..].iter().any(|item| used.contains(&item.id)) {
            *message = "Сначала переназначьте стержни, использующие удаляемые строки".into();
        } else {
            items.truncate(count);
            message.clear();
        }
    }
    let mut remove = None;
    egui::Grid::new(("preset_table", symbol))
        .striped(true)
        .show(ui, |ui| {
            ui.strong("№");
            ui.strong("Название (опционально)");
            ui.strong("Значение");
            ui.strong("Задано численно");
            ui.end_row();
            for (index, item) in items.iter_mut().enumerate() {
                ui.label((index + 1).to_string());
                ui.add(egui::TextEdit::singleline(&mut item.name).desired_width(100.0));
                ui.horizontal(|ui| {
                    if item.value.numeric {
                        ui.add(DragValue::new(&mut item.value.mantissa).speed(0.1));
                        ui.label("× 10^(");
                        ui.add(DragValue::new(&mut item.value.exponent).range(-300..=300));
                        ui.label(")");
                    } else {
                        ui.add(DragValue::new(&mut item.value.symbolic_factor).speed(0.1));
                        ui.label(format!("· {symbol}"));
                    }
                });
                ui.checkbox(&mut item.value.numeric, "");
                if ui
                    .small_button("−")
                    .on_hover_text("Удалить строку")
                    .clicked()
                {
                    remove = Some(item.id);
                }
                ui.end_row();
            }
        });
    if let Some(id) = remove {
        if used.contains(&id) || items.len() == 1 {
            *message = "Нельзя удалить последнюю или назначенную стержню строку".into();
        } else {
            items.retain(|item| item.id != id);
            message.clear();
        }
    }
}

fn preset_selector(
    ui: &mut egui::Ui,
    key: (usize, usize),
    selected: &mut Option<u32>,
    items: &[(u32, String)],
) {
    let label = selected
        .and_then(|id| items.iter().find(|(item_id, _)| *item_id == id))
        .map_or("—", |(_, name)| name.as_str());
    ComboBox::from_id_salt(("property_selector", key))
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (id, name) in items {
                ui.selectable_value(selected, Some(*id), name);
            }
        });
}

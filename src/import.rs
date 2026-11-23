//! Табличный импорт модели. Никаких изменений открытого проекта до применения.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use calamine::{open_workbook_auto, Reader};

use crate::{
    fem::{FemProject, PointLoad, SupportKind},
    next_node_id, rod_id_from_nodes, Diagram, DiagramKind, FiberSide, Node, Ordinate, Point,
    PropertyPreset, PropertyValue, Rod, StructuralModel, SupportDirection, SupportMotion,
    TemperatureLoad,
};

pub const TABLE_NAMES: &[&str] = &[
    "Points",
    "Rods",
    "EI",
    "Alpha",
    "Settings",
    "Supports",
    "NodeLoads",
    "MemberLoads",
    "PointLoads",
    "Diagrams",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportIssue {
    pub table: String,
    pub row: usize,
    pub column: String,
    pub message: String,
}

impl std::fmt::Display for ImportIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} [{}]: {}",
            self.table, self.row, self.column, self.message
        )
    }
}

#[derive(Debug, Clone)]
pub struct ImportedModel {
    pub model: StructuralModel,
    pub fem: FemProject,
    pub counts: Vec<(String, usize)>,
}

#[derive(Debug, Clone)]
struct Table {
    name: String,
    headers: Vec<String>,
    rows: Vec<(usize, Vec<String>)>,
}

impl Table {
    fn column(&self, name: &str) -> Option<usize> {
        self.headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(name))
    }

    fn value<'a>(&self, row: &'a [String], name: &str) -> &'a str {
        self.column(name)
            .and_then(|index| row.get(index))
            .map_or("", |value| value.trim())
    }
}

#[derive(Default)]
struct Tables {
    entries: HashMap<String, Table>,
}

impl Tables {
    fn insert(&mut self, table: Table) -> Result<(), ImportIssue> {
        let key = table.name.to_ascii_lowercase();
        if self.entries.insert(key, table.clone()).is_some() {
            return Err(issue(&table.name, 1, "лист", "таблица повторяется"));
        }
        Ok(())
    }

    fn get(&self, name: &str) -> Option<&Table> {
        self.entries.get(&name.to_ascii_lowercase())
    }
}

fn issue(table: &str, row: usize, column: &str, message: impl Into<String>) -> ImportIssue {
    ImportIssue {
        table: table.into(),
        row,
        column: column.into(),
        message: message.into(),
    }
}

fn normalize_cell(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('\u{feff}')
        .trim()
        .to_string()
}

fn table_from_rows(
    name: &str,
    rows: impl IntoIterator<Item = (usize, Vec<String>)>,
) -> Result<Table, ImportIssue> {
    let mut rows = rows.into_iter();
    let (_, header) = rows
        .next()
        .ok_or_else(|| issue(name, 1, "заголовок", "пустая таблица"))?;
    let headers = header.iter().map(|cell| normalize_cell(cell)).collect();
    Ok(Table {
        name: name.into(),
        headers,
        rows: rows
            .filter(|(_, row)| row.iter().any(|cell| !cell.trim().is_empty()))
            .collect(),
    })
}

pub fn import_xlsx(path: &Path) -> Result<ImportedModel, Vec<ImportIssue>> {
    if !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx"))
    {
        return Err(vec![issue("Файл", 0, "расширение", "нужен файл .xlsx")]);
    }
    let mut workbook = open_workbook_auto(path)
        .map_err(|error| vec![issue("Файл", 0, "открытие", error.to_string())])?;
    let names = workbook.sheet_names().to_vec();
    let mut tables = Tables::default();
    let mut issues = Vec::new();
    for expected in TABLE_NAMES {
        let Some(name) = names
            .iter()
            .find(|name| name.eq_ignore_ascii_case(expected))
        else {
            continue;
        };
        let range = match workbook.worksheet_range(name) {
            Ok(range) => range,
            Err(error) => {
                issues.push(issue(name, 0, "лист", error.to_string()));
                continue;
            }
        };
        if range.start() != Some((0, 0)) {
            issues.push(issue(name, 1, "A1", "заголовки должны начинаться в A1"));
            continue;
        }
        let formulas = workbook.worksheet_formula(name).ok();
        let mut rows = Vec::new();
        for (row_index, row) in range.rows().enumerate() {
            let mut cells = Vec::new();
            for (column_index, value) in row.iter().enumerate() {
                let value = value.to_string();
                if value.trim().is_empty()
                    && formulas.as_ref().is_some_and(|formula_range| {
                        formula_range
                            .get_value((row_index as u32, column_index as u32))
                            .is_some_and(|formula| !formula.is_empty())
                    })
                {
                    issues.push(issue(
                        name,
                        row_index + 1,
                        &column_letters(column_index),
                        "формула не содержит сохранённого вычисленного значения",
                    ));
                }
                cells.push(value);
            }
            rows.push((row_index + 1, cells));
        }
        match table_from_rows(name, rows) {
            Ok(table) => {
                if let Err(error) = tables.insert(table) {
                    issues.push(error);
                }
            }
            Err(error) => issues.push(error),
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    build_model(tables)
}

fn column_letters(mut index: usize) -> String {
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

/// Читает согласованный набор таблиц из каталога Points.csv, Rods.csv и т. д.
pub fn import_csv_directory(path: &Path) -> Result<ImportedModel, Vec<ImportIssue>> {
    let mut tables = Tables::default();
    let mut issues = Vec::new();
    let entries =
        fs::read_dir(path).map_err(|error| vec![issue("Каталог", 0, "путь", error.to_string())])?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                issues.push(issue("Каталог", 0, "файл", error.to_string()));
                continue;
            }
        };
        let file = entry.path();
        if !file
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
        {
            continue;
        }
        let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Some(name) = TABLE_NAMES
            .iter()
            .find(|name| name.eq_ignore_ascii_case(stem))
        else {
            continue;
        };
        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => {
                issues.push(issue(name, 0, "файл", error.to_string()));
                continue;
            }
        };
        let first_line = bytes.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
        let delimiter = if first_line.iter().filter(|byte| **byte == b';').count()
            > first_line.iter().filter(|byte| **byte == b',').count()
        {
            b';'
        } else {
            b','
        };
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .delimiter(delimiter)
            .from_reader(bytes.as_slice());
        let mut rows = Vec::new();
        for (index, record) in reader.records().enumerate() {
            match record {
                Ok(record) => rows.push((
                    index + 1,
                    record.iter().map(normalize_cell).collect::<Vec<_>>(),
                )),
                Err(error) => issues.push(issue(name, index + 1, "CSV", error.to_string())),
            }
        }
        match table_from_rows(name, rows) {
            Ok(table) => {
                if let Err(error) = tables.insert(table) {
                    issues.push(error);
                }
            }
            Err(error) => issues.push(error),
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    build_model(tables)
}

fn require_headers(table: &Table, headers: &[&str], issues: &mut Vec<ImportIssue>) {
    for header in headers {
        if table.column(header).is_none() {
            issues.push(issue(
                &table.name,
                1,
                header,
                "отсутствует обязательный столбец",
            ));
        }
    }
}

fn number(
    table: &Table,
    row_number: usize,
    row: &[String],
    field: &str,
    default: Option<f64>,
    issues: &mut Vec<ImportIssue>,
) -> Option<f64> {
    let value = table.value(row, field);
    if value.is_empty() {
        return default.or_else(|| {
            issues.push(issue(&table.name, row_number, field, "число не задано"));
            None
        });
    }
    let normalized = value.replace(',', ".");
    match normalized.parse::<f64>() {
        Ok(number) if number.is_finite() => Some(number),
        _ => {
            issues.push(issue(
                &table.name,
                row_number,
                field,
                format!("некорректное конечное число: {value}"),
            ));
            None
        }
    }
}

fn integer(
    table: &Table,
    row_number: usize,
    row: &[String],
    field: &str,
    default: Option<i32>,
    issues: &mut Vec<ImportIssue>,
) -> Option<i32> {
    let value = table.value(row, field);
    if value.is_empty() {
        return default.or_else(|| {
            issues.push(issue(
                &table.name,
                row_number,
                field,
                "целое число не задано",
            ));
            None
        });
    }
    match value.parse::<i32>() {
        Ok(number) => Some(number),
        Err(_) => {
            issues.push(issue(
                &table.name,
                row_number,
                field,
                "ожидалось целое число",
            ));
            None
        }
    }
}

fn boolean(
    table: &Table,
    row_number: usize,
    row: &[String],
    field: &str,
    issues: &mut Vec<ImportIssue>,
) -> Option<bool> {
    let value = table.value(row, field);
    match value.to_lowercase().as_str() {
        "" | "0" | "false" | "нет" | "no" => Some(false),
        "1" | "true" | "да" | "yes" => Some(true),
        _ => {
            issues.push(issue(
                &table.name,
                row_number,
                field,
                format!("ожидалось да/нет или true/false: {value}"),
            ));
            None
        }
    }
}

fn named_index(
    table: &Table,
    row_number: usize,
    row: &[String],
    field: &str,
    names: &[String],
    issues: &mut Vec<ImportIssue>,
) -> Option<usize> {
    let value = table.value(row, field);
    names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(value))
        .or_else(|| {
            issues.push(issue(
                &table.name,
                row_number,
                field,
                format!("неизвестный ID: {value}"),
            ));
            None
        })
}

fn parse_presets(table: Option<&Table>, issues: &mut Vec<ImportIssue>) -> Vec<PropertyPreset> {
    let Some(table) = table else {
        return Vec::new();
    };
    require_headers(
        table,
        &["id", "name", "numeric", "factor", "mantissa", "exponent"],
        issues,
    );
    let mut presets = Vec::new();
    for (row_number, row) in &table.rows {
        let Some(id) = integer(table, *row_number, row, "id", None, issues) else {
            continue;
        };
        if id <= 0 {
            issues.push(issue(
                &table.name,
                *row_number,
                "id",
                "ID должен быть положительным",
            ));
            continue;
        }
        let numeric = boolean(table, *row_number, row, "numeric", issues).unwrap_or(false);
        let factor = number(table, *row_number, row, "factor", Some(1.0), issues).unwrap_or(1.0);
        let mantissa =
            number(table, *row_number, row, "mantissa", Some(1.0), issues).unwrap_or(1.0);
        let exponent = integer(table, *row_number, row, "exponent", Some(0), issues).unwrap_or(0);
        let value = PropertyValue {
            numeric,
            symbolic_factor: factor,
            mantissa,
            exponent,
        };
        if let Err(error) = value.validate() {
            issues.push(issue(&table.name, *row_number, "значение", error));
        }
        let name = table.value(row, "name");
        presets.push(PropertyPreset {
            id: id as u32,
            name: if name.is_empty() {
                format!("{} {id}", table.name)
            } else {
                name.into()
            },
            value,
        });
    }
    presets
}

fn preset_reference(
    table: &Table,
    row_number: usize,
    row: &[String],
    field: &str,
    presets: &[PropertyPreset],
    issues: &mut Vec<ImportIssue>,
) -> Option<u32> {
    let value = table.value(row, field);
    if value.is_empty() {
        return None;
    }
    let selected = presets
        .iter()
        .find(|preset| preset.id.to_string() == value || preset.name.eq_ignore_ascii_case(value));
    match selected {
        Some(preset) => Some(preset.id),
        None => {
            issues.push(issue(
                &table.name,
                row_number,
                field,
                format!("нет строки справочника {value}"),
            ));
            None
        }
    }
}

fn support_kind(value: &str) -> Option<SupportKind> {
    match value.to_lowercase().as_str() {
        "" | "free" | "нет" => Some(SupportKind::Free),
        "fixed" | "заделка" => Some(SupportKind::Fixed),
        "pin" | "шарнир" => Some(SupportKind::Pin),
        "roller" | "подвижная" => Some(SupportKind::Roller),
        "guided" | "скользящая" => Some(SupportKind::Guided),
        _ => None,
    }
}

fn fiber(value: &str) -> Option<FiberSide> {
    match value.to_lowercase().as_str() {
        "" | "0" | "axis" | "ось" => Some(FiberSide::OnAxis),
        "-" | "upper" | "left" | "верх" | "лево" => Some(FiberSide::UpperOrLeft),
        "+" | "lower" | "right" | "низ" | "право" => Some(FiberSide::LowerOrRight),
        _ => None,
    }
}

fn diagram_kind(value: &str) -> Option<DiagramKind> {
    match value.to_lowercase().as_str() {
        "constant" | "константа" => Some(DiagramKind::Constant),
        "" | "linear" | "линейная" => Some(DiagramKind::Linear),
        "parabolic" | "параболическая" => Some(DiagramKind::Parabolic),
        _ => None,
    }
}

fn parse_diagram(
    table: &Table,
    row_number: usize,
    row: &[String],
    issues: &mut Vec<ImportIssue>,
) -> Option<Diagram> {
    let kind = diagram_kind(table.value(row, "kind")).or_else(|| {
        issues.push(issue(
            &table.name,
            row_number,
            "kind",
            "неизвестный тип эпюры",
        ));
        None
    })?;
    let left = number(table, row_number, row, "left", None, issues)?;
    let right = number(table, row_number, row, "right", None, issues)?;
    let left_side = fiber(table.value(row, "left_side")).or_else(|| {
        issues.push(issue(
            &table.name,
            row_number,
            "left_side",
            "неизвестная сторона",
        ));
        None
    })?;
    let right_side = fiber(table.value(row, "right_side")).or_else(|| {
        issues.push(issue(
            &table.name,
            row_number,
            "right_side",
            "неизвестная сторона",
        ));
        None
    })?;
    let q = if kind == DiagramKind::Parabolic {
        Some(number(table, row_number, row, "q", None, issues)?)
    } else {
        None
    };
    let q_side = if kind == DiagramKind::Parabolic {
        fiber(table.value(row, "q_side")).or_else(|| {
            issues.push(issue(
                &table.name,
                row_number,
                "q_side",
                "неизвестная выпуклость",
            ));
            None
        })
    } else {
        None
    };
    let extremum = boolean(table, row_number, row, "has_extremum", issues)?;
    let left = Ordinate::new(left, left_side)
        .map_err(|error| {
            issues.push(issue(&table.name, row_number, "left", error));
        })
        .ok()?;
    let right = Ordinate::new(right, right_side)
        .map_err(|error| {
            issues.push(issue(&table.name, row_number, "right", error));
        })
        .ok()?;
    Diagram::new(kind, left, right, q, q_side, extremum)
        .map_err(|error| issues.push(issue(&table.name, row_number, "kind", error)))
        .ok()
}

fn build_model(tables: Tables) -> Result<ImportedModel, Vec<ImportIssue>> {
    let mut issues = Vec::new();
    let Some(points) = tables.get("Points") else {
        return Err(vec![issue(
            "Points",
            1,
            "лист",
            "обязательная таблица отсутствует",
        )]);
    };
    let Some(rods) = tables.get("Rods") else {
        return Err(vec![issue(
            "Rods",
            1,
            "лист",
            "обязательная таблица отсутствует",
        )]);
    };
    require_headers(points, &["id", "x", "y"], &mut issues);
    require_headers(rods, &["id", "start", "end"], &mut issues);
    let mut model = StructuralModel {
        rigidity_presets: parse_presets(tables.get("EI"), &mut issues),
        thermal_presets: parse_presets(tables.get("Alpha"), &mut issues),
        ..Default::default()
    };
    for (row_number, row) in &points.rows {
        let Some(x) = number(points, *row_number, row, "x", None, &mut issues) else {
            continue;
        };
        let Some(y) = number(points, *row_number, row, "y", None, &mut issues) else {
            continue;
        };
        let id = points.value(row, "id");
        let id = if id.is_empty() {
            next_node_id(&model.nodes)
        } else {
            id.into()
        };
        if model
            .nodes
            .iter()
            .any(|node| node.id.eq_ignore_ascii_case(&id))
        {
            issues.push(issue(
                &points.name,
                *row_number,
                "id",
                "название точки повторяется",
            ));
            continue;
        }
        if model
            .nodes
            .iter()
            .any(|node| node.position.x == x && node.position.y == y)
        {
            issues.push(issue(
                &points.name,
                *row_number,
                "x/y",
                "координаты точки повторяются",
            ));
            continue;
        }
        model.nodes.push(Node {
            id,
            position: Point { x, y },
        });
    }
    let node_names = model
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    for (row_number, row) in &rods.rows {
        let Some(start) = named_index(rods, *row_number, row, "start", &node_names, &mut issues)
        else {
            continue;
        };
        let Some(end) = named_index(rods, *row_number, row, "end", &node_names, &mut issues) else {
            continue;
        };
        let id = rods.value(row, "id");
        let id = if id.is_empty() {
            rod_id_from_nodes(&model.nodes, start, end).unwrap_or_default()
        } else {
            id.into()
        };
        if model
            .rods
            .iter()
            .any(|rod| rod.id.eq_ignore_ascii_case(&id))
        {
            issues.push(issue(
                &rods.name,
                *row_number,
                "id",
                "название стержня повторяется",
            ));
            continue;
        }
        if start == end
            || model.rods.iter().any(|rod| {
                (rod.start_node == start && rod.end_node == end)
                    || (rod.start_node == end && rod.end_node == start)
            })
        {
            issues.push(issue(
                &rods.name,
                *row_number,
                "start/end",
                "стержень повторяется или имеет нулевую длину",
            ));
            continue;
        }
        let mut rod = Rod::blank(id, start, end);
        rod.rigidity_preset_id = preset_reference(
            rods,
            *row_number,
            row,
            "ei_preset",
            &model.rigidity_presets,
            &mut issues,
        );
        rod.thermal_preset_id = preset_reference(
            rods,
            *row_number,
            row,
            "alpha_preset",
            &model.thermal_presets,
            &mut issues,
        );
        rod.section_height =
            number(rods, *row_number, row, "h", Some(0.5), &mut issues).unwrap_or(0.5);
        rod.release_start_moment =
            boolean(rods, *row_number, row, "hinge_start", &mut issues).unwrap_or(false);
        rod.release_end_moment =
            boolean(rods, *row_number, row, "hinge_end", &mut issues).unwrap_or(false);
        let top = rods.value(row, "t_upper");
        let bottom = rods.value(row, "t_lower");
        if !top.is_empty() || !bottom.is_empty() {
            let upper_or_left = number(rods, *row_number, row, "t_upper", None, &mut issues);
            let lower_or_right = number(rods, *row_number, row, "t_lower", None, &mut issues);
            if let (Some(upper_or_left), Some(lower_or_right)) = (upper_or_left, lower_or_right) {
                rod.temperature = Some(TemperatureLoad {
                    upper_or_left,
                    lower_or_right,
                });
            }
        }
        model.rods.push(rod);
    }
    if let Some(settings) = tables.get("Settings") {
        require_headers(settings, &["key", "value"], &mut issues);
        for (row_number, row) in &settings.rows {
            if settings
                .value(row, "key")
                .eq_ignore_ascii_case("target_node")
            {
                if let Some(index) = named_index(
                    settings,
                    *row_number,
                    row,
                    "value",
                    &node_names,
                    &mut issues,
                ) {
                    model.target_node = index;
                }
            }
        }
    }
    let mut fem = FemProject::default();
    fem.reconcile(&model);
    let rod_names = model
        .rods
        .iter()
        .map(|rod| rod.id.clone())
        .collect::<Vec<_>>();

    if let Some(table) = tables.get("Supports") {
        require_headers(table, &["node", "kind"], &mut issues);
        let mut seen = HashSet::new();
        for (row_number, row) in &table.rows {
            let Some(node) = named_index(table, *row_number, row, "node", &node_names, &mut issues)
            else {
                continue;
            };
            if !seen.insert(node) {
                issues.push(issue(
                    &table.name,
                    *row_number,
                    "node",
                    "опора точки повторяется",
                ));
                continue;
            }
            let kind = support_kind(table.value(row, "kind")).or_else(|| {
                issues.push(issue(
                    &table.name,
                    *row_number,
                    "kind",
                    "неизвестный тип опоры",
                ));
                None
            });
            let Some(kind) = kind else { continue };
            let normal_deg = number(
                table,
                *row_number,
                row,
                "normal_deg",
                Some(90.0),
                &mut issues,
            )
            .unwrap_or(90.0);
            fem.nodes[node].support = kind;
            fem.nodes[node].normal_deg = normal_deg;
            for (field, direction, allowed) in [
                (
                    "dx",
                    SupportDirection::Horizontal,
                    kind == SupportKind::Fixed || kind == SupportKind::Pin,
                ),
                (
                    "dy",
                    SupportDirection::Vertical,
                    kind == SupportKind::Fixed || kind == SupportKind::Pin,
                ),
                (
                    "dphi",
                    SupportDirection::Rotation,
                    kind == SupportKind::Fixed || kind == SupportKind::Guided,
                ),
                (
                    "dn",
                    SupportDirection::Normal,
                    kind == SupportKind::Roller || kind == SupportKind::Guided,
                ),
            ] {
                let Some(displacement) =
                    number(table, *row_number, row, field, Some(0.0), &mut issues)
                else {
                    continue;
                };
                if displacement.abs() > 0.0 {
                    if !allowed {
                        issues.push(issue(
                            &table.name,
                            *row_number,
                            field,
                            "смещение не соответствует типу опоры",
                        ));
                    } else {
                        model.supports.push(SupportMotion {
                            node,
                            direction,
                            normal_deg,
                            displacement,
                            unit_x_reaction: 0.0,
                            unit_y_reaction: 0.0,
                            unit_rotation_reaction: 0.0,
                        });
                    }
                }
            }
        }
    }
    if let Some(table) = tables.get("NodeLoads") {
        require_headers(table, &["node", "fx", "fy", "mz"], &mut issues);
        for (row_number, row) in &table.rows {
            let Some(node) = named_index(table, *row_number, row, "node", &node_names, &mut issues)
            else {
                continue;
            };
            fem.nodes[node].fx +=
                number(table, *row_number, row, "fx", Some(0.0), &mut issues).unwrap_or(0.0);
            fem.nodes[node].fy +=
                number(table, *row_number, row, "fy", Some(0.0), &mut issues).unwrap_or(0.0);
            fem.nodes[node].mz +=
                number(table, *row_number, row, "mz", Some(0.0), &mut issues).unwrap_or(0.0);
        }
    }
    if let Some(table) = tables.get("MemberLoads") {
        require_headers(table, &["rod", "qx", "qy", "ea", "ei"], &mut issues);
        for (row_number, row) in &table.rows {
            let Some(index) = named_index(table, *row_number, row, "rod", &rod_names, &mut issues)
            else {
                continue;
            };
            fem.members[index].qx +=
                number(table, *row_number, row, "qx", Some(0.0), &mut issues).unwrap_or(0.0);
            fem.members[index].qy +=
                number(table, *row_number, row, "qy", Some(0.0), &mut issues).unwrap_or(0.0);
            fem.members[index].ea =
                number(table, *row_number, row, "ea", Some(0.0), &mut issues).unwrap_or(0.0);
            fem.members[index].ei =
                number(table, *row_number, row, "ei", Some(0.0), &mut issues).unwrap_or(0.0);
        }
    }
    if let Some(table) = tables.get("PointLoads") {
        require_headers(table, &["rod", "xi", "fx", "fy", "mz"], &mut issues);
        for (row_number, row) in &table.rows {
            let Some(index) = named_index(table, *row_number, row, "rod", &rod_names, &mut issues)
            else {
                continue;
            };
            let Some(xi) = number(table, *row_number, row, "xi", None, &mut issues) else {
                continue;
            };
            if !(0.0..=1.0).contains(&xi) {
                issues.push(issue(
                    &table.name,
                    *row_number,
                    "xi",
                    "относительное положение должно быть от 0 до 1",
                ));
                continue;
            }
            fem.members[index].point_loads.push(PointLoad {
                xi,
                fx: number(table, *row_number, row, "fx", Some(0.0), &mut issues).unwrap_or(0.0),
                fy: number(table, *row_number, row, "fy", Some(0.0), &mut issues).unwrap_or(0.0),
                mz: number(table, *row_number, row, "mz", Some(0.0), &mut issues).unwrap_or(0.0),
            });
        }
    }
    if let Some(table) = tables.get("Diagrams") {
        require_headers(
            table,
            &[
                "rod",
                "field",
                "kind",
                "left",
                "left_side",
                "right",
                "right_side",
                "q",
                "q_side",
                "has_extremum",
            ],
            &mut issues,
        );
        let mut seen = HashSet::new();
        for (row_number, row) in &table.rows {
            let Some(index) = named_index(table, *row_number, row, "rod", &rod_names, &mut issues)
            else {
                continue;
            };
            if !seen.insert((index, table.value(row, "field").to_ascii_lowercase())) {
                issues.push(issue(
                    &table.name,
                    *row_number,
                    "field",
                    "эпюра для стержня повторяется",
                ));
                continue;
            }
            let Some(diagram) = parse_diagram(table, *row_number, row, &mut issues) else {
                continue;
            };
            let rod = &mut model.rods[index];
            match table.value(row, "field").to_lowercase().as_str() {
                "m" => rod.load_diagram = diagram,
                "n" => rod.normal_diagram = diagram,
                "q" => rod.shear_diagram = diagram,
                "mx" => rod.unit_x_diagram = diagram,
                "my" => rod.unit_y_diagram = diagram,
                "mphi" | "mφ" => rod.unit_rotation_diagram = diagram,
                "nx" => rod.unit_normal_x_diagram = diagram,
                "ny" => rod.unit_normal_y_diagram = diagram,
                "nphi" | "nφ" => rod.unit_normal_rotation_diagram = diagram,
                _ => issues.push(issue(
                    &table.name,
                    *row_number,
                    "field",
                    "неизвестная эпюра",
                )),
            }
        }
    }
    if let Err(error) = model.validate() {
        issues.push(issue("Модель", 0, "проверка", error));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let counts = TABLE_NAMES
        .iter()
        .filter_map(|name| {
            tables
                .get(name)
                .map(|table| (table.name.clone(), table.rows.len()))
        })
        .collect();
    Ok(ImportedModel { model, fem, counts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tables() -> Tables {
        let mut tables = Tables::default();
        for (name, rows) in [
            (
                "Points",
                vec![
                    vec!["id", "x", "y"],
                    vec!["A", "0", "0"],
                    vec!["B", "2", "0"],
                ],
            ),
            (
                "Rods",
                vec![vec!["id", "start", "end", "h"], vec!["AB", "A", "B", "0,5"]],
            ),
            ("Supports", vec![vec!["node", "kind"], vec!["A", "fixed"]]),
            (
                "NodeLoads",
                vec![vec!["node", "fx", "fy", "mz"], vec!["B", "0", "-10", "0"]],
            ),
        ] {
            tables
                .insert(
                    table_from_rows(
                        name,
                        rows.into_iter()
                            .enumerate()
                            .map(|(i, row)| (i + 1, row.into_iter().map(str::to_string).collect())),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        tables
    }

    #[test]
    fn tables_create_valid_frame_and_fem_loads() {
        let imported = build_model(sample_tables()).unwrap();
        assert_eq!(imported.model.rods[0].section_height, 0.5);
        assert_eq!(imported.fem.nodes[0].support, SupportKind::Fixed);
        assert_eq!(imported.fem.nodes[1].fy, -10.0);
        let result = imported.fem.solve(&imported.model).unwrap();
        assert!((result.actual.reactions[0][1] - 10.0).abs() < 1.0e-9);
    }

    #[test]
    fn bad_reference_reports_table_row_and_field() {
        let mut tables = sample_tables();
        tables.get("Rods").unwrap();
        tables.entries.get_mut("rods").unwrap().rows[0].1[1] = "X".into();
        let errors = build_model(tables).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.table == "Rods" && error.row == 2 && error.column == "start"));
    }

    #[test]
    fn published_csv_template_imports_and_solves() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/csv");
        let imported = import_csv_directory(&directory).unwrap();
        assert_eq!(imported.model.nodes.len(), 3);
        assert_eq!(imported.model.rods.len(), 2);
        assert_eq!(imported.fem.members[1].qx, -5.0);
        imported.fem.solve(&imported.model).unwrap();
    }

    #[test]
    fn published_xlsx_template_imports_and_solves() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/mohr-input.xlsx");
        let imported = import_xlsx(&file).unwrap();
        assert_eq!(imported.model.nodes.len(), 3);
        assert_eq!(imported.fem.members[1].qx, -5.0);
        imported.fem.solve(&imported.model).unwrap();
    }
}

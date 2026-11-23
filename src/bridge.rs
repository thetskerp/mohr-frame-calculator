//! Типизированная граница Rust/C++ для Qt Widgets.
// Команды FFI принимают скалярные поля форм, чтобы не открывать внутренние типы Rust.
#![allow(clippy::too_many_arguments)]

use std::path::{Path, PathBuf};

use crate::{
    calculate_displacement_components_with_rounding,
    calculate_kinematic_displacement_components_with_rounding,
    calculate_thermal_displacement_components_with_rounding,
    docx_report::{build_fem_word_report, build_word_report_with_rounding, FemReportSections},
    fem::{FemMember, FemNode, FemProject, FemResult, PointLoad, SupportKind, UnitComponent},
    import::{import_csv_directory, import_xlsx, ImportedModel},
    next_node_id,
    project::{
        export_for_v050_for_path, load_project_from_path, save_fem_word_report_for_path,
        save_project_to_path, save_word_report_for_path, ProjectData,
    },
    view_state::CanvasViewState,
    Diagram, DiagramKind, EffectsSelection, FiberSide, Node, Ordinate, Point, PropertyPreset,
    PropertyValue, Rigidity, Rod, StructuralModel, SupportDirection, SupportMotion,
    TemperatureLoad, ThermalExpansion,
};

#[cxx::bridge(namespace = "mohr")]
pub mod ffi {
    struct NodeDto {
        index: u32,
        id: String,
        x: f64,
        y: f64,
        support: i32,
        normal_deg: f64,
        fx: f64,
        fy: f64,
        mz: f64,
    }

    struct RodDto {
        index: u32,
        id: String,
        start: u32,
        end: u32,
        hinge_start: bool,
        hinge_end: bool,
        h: f64,
        ei_preset: i32,
        alpha_preset: i32,
        qx: f64,
        qy: f64,
        ea: f64,
        ei: f64,
        has_temperature: bool,
        t_upper: f64,
        t_lower: f64,
    }

    #[derive(Clone)]
    struct GeometryNodeDto {
        id: String,
        x: f64,
        y: f64,
    }

    #[derive(Clone)]
    struct GeometryRodDto {
        id: String,
        start: u32,
        end: u32,
        h: f64,
        hinge_start: bool,
        hinge_end: bool,
    }

    struct PropertyDto {
        id: u32,
        name: String,
        numeric: bool,
        factor: f64,
        mantissa: f64,
        exponent: i32,
    }

    struct CurvePointDto {
        rod_index: u32,
        t: f64,
        value: f64,
    }

    struct DiagramDto {
        kind: u8,
        left: f64,
        right: f64,
        q_signed: f64,
        extremum: bool,
    }

    struct MotionDto {
        node: u32,
        direction: u8,
        normal_deg: f64,
        displacement: f64,
        rx: f64,
        ry: f64,
        rphi: f64,
    }

    struct SettingsDto {
        force: bool,
        thermal: bool,
        kinematic: bool,
        rounding: bool,
        decimals: u8,
        unit_x: bool,
        unit_y: bool,
        unit_phi: bool,
    }

    struct PointLoadDto {
        rod: u32,
        index: u32,
        xi: f64,
        fx: f64,
        fy: f64,
        mz: f64,
    }

    struct ReactionDto {
        node_index: u32,
        x: f64,
        y: f64,
        mz: f64,
    }

    struct ResultLineDto {
        effect: u8,
        component: u8,
        value: String,
        details: String,
    }

    extern "Rust" {
        type Document;
        type ImportDraft;

        fn new_document() -> Box<Document>;
        fn clone_document(document: &Document) -> Box<Document>;
        fn load_document(path: &str) -> Result<Box<Document>>;
        fn document_path(document: &Document) -> String;
        fn document_revision(document: &Document) -> u64;
        fn document_target_node(document: &Document) -> u32;
        fn document_nodes(document: &Document) -> Vec<NodeDto>;
        fn document_rods(document: &Document) -> Vec<RodDto>;
        fn document_properties(document: &Document, alpha: bool) -> Vec<PropertyDto>;
        fn document_replace_properties(
            document: &mut Document,
            rigidity: Vec<PropertyDto>,
            thermal: Vec<PropertyDto>,
            ei_assignments: Vec<i32>,
            alpha_assignments: Vec<i32>,
        ) -> Result<()>;
        fn document_diagram(document: &Document, rod: u32, field: u8) -> Result<DiagramDto>;
        fn document_motions(document: &Document) -> Vec<MotionDto>;
        fn document_settings(document: &Document) -> SettingsDto;
        fn document_point_loads(document: &Document) -> Vec<PointLoadDto>;
        fn document_manual_curve(document: &Document, field: u8) -> Vec<CurvePointDto>;
        fn document_fem_curve(document: &Document, load_case: u8, field: u8) -> Vec<CurvePointDto>;
        fn document_reactions(document: &Document, load_case: u8) -> Vec<ReactionDto>;
        fn document_fem_details(document: &Document) -> String;
        fn document_results(document: &Document) -> Result<Vec<ResultLineDto>>;

        fn document_add_node(document: &mut Document, id: &str, x: f64, y: f64) -> Result<()>;
        fn document_set_node(
            document: &mut Document,
            index: u32,
            id: &str,
            x: f64,
            y: f64,
        ) -> Result<()>;
        fn document_remove_node(document: &mut Document, index: u32) -> Result<()>;
        fn document_add_rod(document: &mut Document, id: &str, start: u32, end: u32) -> Result<()>;
        fn document_set_rod(
            document: &mut Document,
            index: u32,
            id: &str,
            start: u32,
            end: u32,
            h: f64,
            hinge_start: bool,
            hinge_end: bool,
            ei_preset: i32,
            alpha_preset: i32,
        ) -> Result<()>;
        fn document_remove_rod(document: &mut Document, index: u32) -> Result<()>;
        fn document_replace_geometry(
            document: &mut Document,
            nodes: Vec<GeometryNodeDto>,
            rods: Vec<GeometryRodDto>,
        ) -> Result<()>;
        fn document_set_target(document: &mut Document, index: u32) -> Result<()>;
        fn document_set_property(
            document: &mut Document,
            alpha: bool,
            id: u32,
            name: &str,
            numeric: bool,
            factor: f64,
            mantissa: f64,
            exponent: i32,
        ) -> Result<()>;
        fn document_remove_property(document: &mut Document, alpha: bool, id: u32) -> Result<()>;
        fn document_set_fem_node(
            document: &mut Document,
            index: u32,
            support: i32,
            normal_deg: f64,
            fx: f64,
            fy: f64,
            mz: f64,
        ) -> Result<()>;
        fn document_set_fem_rod(
            document: &mut Document,
            index: u32,
            qx: f64,
            qy: f64,
            ea: f64,
            ei: f64,
        ) -> Result<()>;
        fn document_add_point_load(
            document: &mut Document,
            rod: u32,
            xi: f64,
            fx: f64,
            fy: f64,
            mz: f64,
        ) -> Result<()>;
        fn document_remove_point_load(document: &mut Document, rod: u32, index: u32) -> Result<()>;
        fn document_set_temperature(
            document: &mut Document,
            rod: u32,
            active: bool,
            upper: f64,
            lower: f64,
        ) -> Result<()>;
        fn document_set_diagram(
            document: &mut Document,
            rod: u32,
            field: u8,
            kind: u8,
            left: f64,
            right: f64,
            q_signed: f64,
            extremum: bool,
        ) -> Result<()>;
        fn document_set_support_motion(
            document: &mut Document,
            node: u32,
            direction: u8,
            normal_deg: f64,
            displacement: f64,
            rx: f64,
            ry: f64,
            rphi: f64,
        ) -> Result<()>;
        fn document_remove_support_motion(
            document: &mut Document,
            node: u32,
            direction: u8,
        ) -> Result<()>;
        fn document_set_effects(
            document: &mut Document,
            force: bool,
            thermal: bool,
            kinematic: bool,
        );
        fn document_set_rounding(document: &mut Document, enabled: bool, decimals: u8);
        fn document_set_unit_selection(document: &mut Document, x: bool, y: bool, phi: bool);
        fn document_solve_fem(document: &mut Document) -> Result<()>;
        fn document_transfer_fem(document: &mut Document) -> Result<()>;
        fn document_save(document: &mut Document, path: &str) -> Result<String>;
        fn document_export_word(document: &Document) -> Result<String>;
        fn document_export_fem_word(document: &Document, section_flags: u8) -> Result<String>;
        fn document_export_legacy(document: &Document) -> Result<String>;
        fn import_xlsx_draft(path: &str) -> Result<Box<ImportDraft>>;
        fn import_csv_draft(path: &str) -> Result<Box<ImportDraft>>;
        fn import_draft_summary(draft: &ImportDraft) -> String;
        fn import_draft_nodes(draft: &ImportDraft) -> Vec<NodeDto>;
        fn import_draft_rods(draft: &ImportDraft) -> Vec<RodDto>;
        fn document_apply_import(document: &mut Document, draft: Box<ImportDraft>);
    }
}

pub struct Document {
    data: ProjectData,
    path: Option<PathBuf>,
    fem_result: Option<FemResult>,
    revision: u64,
}

pub struct ImportDraft {
    imported: ImportedModel,
}

fn blank_data() -> ProjectData {
    ProjectData {
        model: StructuralModel::default(),
        fem: FemProject::default(),
        rigidity: Rigidity::Symbolic,
        thermal_expansion: ThermalExpansion::Symbolic,
        effects: EffectsSelection::default(),
        show_values: true,
        view_state: CanvasViewState::default(),
        calculation_rounding: false,
        calculation_decimals: 2,
    }
}

fn new_document() -> Box<Document> {
    Box::new(Document {
        data: blank_data(),
        path: None,
        fem_result: None,
        revision: 0,
    })
}

fn clone_document(document: &Document) -> Box<Document> {
    Box::new(Document {
        data: document.data.clone(),
        path: document.path.clone(),
        fem_result: document.fem_result.clone(),
        revision: document.revision,
    })
}

fn load_document(path: &str) -> Result<Box<Document>, String> {
    let loaded = load_project_from_path(Path::new(path))?;
    Ok(Box::new(Document {
        data: loaded.data,
        path: Some(loaded.path),
        fem_result: None,
        revision: 0,
    }))
}

fn document_path(document: &Document) -> String {
    document
        .path
        .as_ref()
        .map_or(String::new(), |path| path.to_string_lossy().into_owned())
}

fn document_revision(document: &Document) -> u64 {
    document.revision
}

fn document_target_node(document: &Document) -> u32 {
    document.data.model.target_node as u32
}

fn document_nodes(document: &Document) -> Vec<ffi::NodeDto> {
    document
        .data
        .model
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let fem = document.data.fem.nodes.get(index);
            ffi::NodeDto {
                index: index as u32,
                id: node.id.clone(),
                x: node.position.x,
                y: node.position.y,
                support: fem.map_or(0, |fem| support_code(fem.support)),
                normal_deg: fem.map_or(90.0, |fem| fem.normal_deg),
                fx: fem.map_or(0.0, |fem| fem.fx),
                fy: fem.map_or(0.0, |fem| fem.fy),
                mz: fem.map_or(0.0, |fem| fem.mz),
            }
        })
        .collect()
}

fn document_rods(document: &Document) -> Vec<ffi::RodDto> {
    document
        .data
        .model
        .rods
        .iter()
        .enumerate()
        .map(|(index, rod)| {
            let fem = document.data.fem.members.get(index);
            ffi::RodDto {
                index: index as u32,
                id: rod.id.clone(),
                start: rod.start_node as u32,
                end: rod.end_node as u32,
                hinge_start: rod.release_start_moment,
                hinge_end: rod.release_end_moment,
                h: rod.section_height,
                ei_preset: rod.rigidity_preset_id.map_or(-1, |id| id as i32),
                alpha_preset: rod.thermal_preset_id.map_or(-1, |id| id as i32),
                qx: fem.map_or(0.0, |fem| fem.qx),
                qy: fem.map_or(0.0, |fem| fem.qy),
                ea: fem.map_or(0.0, |fem| fem.ea),
                ei: fem.map_or(0.0, |fem| fem.ei),
                has_temperature: rod.temperature.is_some(),
                t_upper: rod.temperature.map_or(0.0, |value| value.upper_or_left),
                t_lower: rod.temperature.map_or(0.0, |value| value.lower_or_right),
            }
        })
        .collect()
}

fn document_properties(document: &Document, alpha: bool) -> Vec<ffi::PropertyDto> {
    let presets = if alpha {
        &document.data.model.thermal_presets
    } else {
        &document.data.model.rigidity_presets
    };
    presets
        .iter()
        .map(|preset| ffi::PropertyDto {
            id: preset.id,
            name: preset.name.clone(),
            numeric: preset.value.numeric,
            factor: preset.value.symbolic_factor,
            mantissa: preset.value.mantissa,
            exponent: preset.value.exponent,
        })
        .collect()
}

fn document_replace_properties(
    document: &mut Document,
    rigidity: Vec<ffi::PropertyDto>,
    thermal: Vec<ffi::PropertyDto>,
    ei_assignments: Vec<i32>,
    alpha_assignments: Vec<i32>,
) -> Result<(), String> {
    let mut model = document.data.model.clone();
    if ei_assignments.len() != model.rods.len() || alpha_assignments.len() != model.rods.len() {
        return Err("число назначений не совпадает с числом стержней".into());
    }
    let convert = |items: Vec<ffi::PropertyDto>| -> Result<Vec<PropertyPreset>, String> {
        items
            .into_iter()
            .map(|item| {
                let value = PropertyValue {
                    numeric: item.numeric,
                    symbolic_factor: item.factor,
                    mantissa: item.mantissa,
                    exponent: item.exponent,
                };
                value.validate()?;
                Ok(PropertyPreset {
                    id: item.id,
                    name: item.name,
                    value,
                })
            })
            .collect()
    };
    model.rigidity_presets = convert(rigidity)?;
    model.thermal_presets = convert(thermal)?;
    for (index, rod) in model.rods.iter_mut().enumerate() {
        rod.rigidity_preset_id =
            (ei_assignments[index] >= 0).then_some(ei_assignments[index] as u32);
        rod.thermal_preset_id =
            (alpha_assignments[index] >= 0).then_some(alpha_assignments[index] as u32);
    }
    model.validate()?;
    document.data.model = model;
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

fn document_diagram(document: &Document, rod: u32, field: u8) -> Result<ffi::DiagramDto, String> {
    let rod = document
        .data
        .model
        .rods
        .get(rod as usize)
        .ok_or("стержень не найден")?;
    let diagram = selected_diagram(rod, field).ok_or("неизвестная эпюра")?;
    Ok(ffi::DiagramDto {
        kind: match diagram.kind {
            DiagramKind::Constant => 0,
            DiagramKind::Linear => 1,
            DiagramKind::Parabolic => 2,
        },
        left: diagram.left_value(),
        right: diagram.right_value(),
        q_signed: diagram.q.unwrap_or(0.0) * diagram.q_fiber.map_or(0.0, FiberSide::sign),
        extremum: diagram.has_extremum,
    })
}

fn document_motions(document: &Document) -> Vec<ffi::MotionDto> {
    document
        .data
        .model
        .supports
        .iter()
        .map(|motion| ffi::MotionDto {
            node: motion.node as u32,
            direction: match motion.direction {
                SupportDirection::Horizontal => 0,
                SupportDirection::Vertical => 1,
                SupportDirection::Rotation => 2,
                SupportDirection::Normal => 3,
            },
            normal_deg: motion.normal_deg,
            displacement: motion.displacement,
            rx: motion.unit_x_reaction,
            ry: motion.unit_y_reaction,
            rphi: motion.unit_rotation_reaction,
        })
        .collect()
}

fn document_settings(document: &Document) -> ffi::SettingsDto {
    ffi::SettingsDto {
        force: document.data.effects.force,
        thermal: document.data.effects.thermal,
        kinematic: document.data.effects.kinematic,
        rounding: document.data.calculation_rounding,
        decimals: document.data.calculation_decimals,
        unit_x: document.data.fem.units.x,
        unit_y: document.data.fem.units.y,
        unit_phi: document.data.fem.units.rotation,
    }
}

fn document_point_loads(document: &Document) -> Vec<ffi::PointLoadDto> {
    document
        .data
        .fem
        .members
        .iter()
        .enumerate()
        .flat_map(|(rod, member)| {
            member
                .point_loads
                .iter()
                .enumerate()
                .map(move |(index, load)| ffi::PointLoadDto {
                    rod: rod as u32,
                    index: index as u32,
                    xi: load.xi,
                    fx: load.fx,
                    fy: load.fy,
                    mz: load.mz,
                })
        })
        .collect()
}

fn selected_diagram(rod: &Rod, field: u8) -> Option<&Diagram> {
    match field {
        0 => Some(&rod.load_diagram),
        1 => Some(&rod.normal_diagram),
        2 => Some(&rod.shear_diagram),
        3 => Some(&rod.unit_x_diagram),
        4 => Some(&rod.unit_y_diagram),
        5 => Some(&rod.unit_rotation_diagram),
        6 => Some(&rod.unit_normal_x_diagram),
        7 => Some(&rod.unit_normal_y_diagram),
        8 => Some(&rod.unit_normal_rotation_diagram),
        _ => None,
    }
}

fn selected_diagram_mut(rod: &mut Rod, field: u8) -> Option<&mut Diagram> {
    match field {
        0 => Some(&mut rod.load_diagram),
        1 => Some(&mut rod.normal_diagram),
        2 => Some(&mut rod.shear_diagram),
        3 => Some(&mut rod.unit_x_diagram),
        4 => Some(&mut rod.unit_y_diagram),
        5 => Some(&mut rod.unit_rotation_diagram),
        6 => Some(&mut rod.unit_normal_x_diagram),
        7 => Some(&mut rod.unit_normal_y_diagram),
        8 => Some(&mut rod.unit_normal_rotation_diagram),
        _ => None,
    }
}

fn document_manual_curve(document: &Document, field: u8) -> Vec<ffi::CurvePointDto> {
    document
        .data
        .model
        .rods
        .iter()
        .enumerate()
        .flat_map(|(index, rod)| {
            let length = document.data.model.rod_length(rod).unwrap_or(1.0);
            selected_diagram(rod, field)
                .into_iter()
                .flat_map(move |diagram| {
                    (0..=40).map(move |sample| {
                        let t = sample as f64 / 40.0;
                        ffi::CurvePointDto {
                            rod_index: index as u32,
                            t,
                            value: diagram.value_at(length, t),
                        }
                    })
                })
        })
        .collect()
}

fn fem_case(document: &Document, load_case: u8) -> Option<&crate::fem::LoadCaseResult> {
    let result = document.fem_result.as_ref()?;
    if load_case == 0 {
        return Some(&result.actual);
    }
    let component = match load_case {
        1 => UnitComponent::X,
        2 => UnitComponent::Y,
        3 => UnitComponent::Rotation,
        _ => return None,
    };
    result
        .units
        .iter()
        .find(|unit| unit.component == component)
        .map(|unit| &unit.result)
}

fn document_fem_curve(document: &Document, load_case: u8, field: u8) -> Vec<ffi::CurvePointDto> {
    let Some(case) = fem_case(document, load_case) else {
        return Vec::new();
    };
    case.members
        .iter()
        .enumerate()
        .flat_map(|(index, member)| {
            (0..=80).map(move |sample| {
                let t = sample as f64 / 80.0;
                let section = member.at(member.length * t);
                ffi::CurvePointDto {
                    rod_index: index as u32,
                    t,
                    value: match field {
                        1 => section.n,
                        2 => section.q,
                        _ => section.m,
                    },
                }
            })
        })
        .collect()
}

fn document_reactions(document: &Document, load_case: u8) -> Vec<ffi::ReactionDto> {
    fem_case(document, load_case)
        .into_iter()
        .flat_map(|case| case.reactions.iter().enumerate())
        .map(|(index, reaction)| ffi::ReactionDto {
            node_index: index as u32,
            x: reaction[0],
            y: reaction[1],
            mz: reaction[2],
        })
        .collect()
}

fn document_fem_details(document: &Document) -> String {
    let Some(result) = &document.fem_result else {
        return "Расчёт МКЭ ещё не выполнен".into();
    };
    let mut text = format!(
        "Степеней свободы: {}. Опорных связей: {}. Режим жёсткости: {:?}.\n",
        result.total_dof, result.constraints, result.stiffness_mode
    );
    let mut append_case = |label: &str, case: &crate::fem::LoadCaseResult| {
        text.push_str(&format!("\n{label}\n"));
        for (index, reaction) in case.reactions.iter().enumerate() {
            let [x, y, mz] = *reaction;
            if x.abs() > 1.0e-9 || y.abs() > 1.0e-9 || mz.abs() > 1.0e-9 {
                text.push_str(&format!(
                    "Реакция {}: Rx={}, Ry={}, Mz={}\n",
                    document.data.model.nodes[index].id,
                    crate::format_number(x),
                    crate::format_number(y),
                    crate::format_number(mz)
                ));
            }
        }
        for (index, member) in case.members.iter().enumerate() {
            let name = &document.data.model.rods[index].id;
            let [n, q, m] = member.calculation_formulas();
            text.push_str(&format!(
                "\nСтержень {name}, L={}\nN(x) = {n}\nQ(x) = {q}\nM(x) = {m}\n",
                crate::format_number(member.length)
            ));
            for (point, forces) in member.calculation_samples() {
                text.push_str(&format!(
                    "  {point}: N={}, Q={}, M={}\n",
                    crate::format_number(forces.n),
                    crate::format_number(forces.q),
                    crate::format_number(forces.m)
                ));
            }
        }
    };
    append_case("Грузовая система", &result.actual);
    for unit in &result.units {
        append_case(
            &format!("Единичная система {}", unit.component.label()),
            &unit.result,
        );
    }
    text
}

fn document_results(document: &Document) -> Result<Vec<ffi::ResultLineDto>, String> {
    let decimals = document
        .data
        .calculation_rounding
        .then_some(document.data.calculation_decimals);
    let force = calculate_displacement_components_with_rounding(
        &document.data.model,
        document.data.rigidity,
        decimals,
    )?;
    let thermal = calculate_thermal_displacement_components_with_rounding(
        &document.data.model,
        document.data.thermal_expansion,
        decimals,
    )?;
    let kinematic =
        calculate_kinematic_displacement_components_with_rounding(&document.data.model, decimals)?;
    let mut lines = Vec::new();
    for (component, (f, t, k)) in [
        (force.horizontal, thermal.horizontal, kinematic.horizontal),
        (force.vertical, thermal.vertical, kinematic.vertical),
        (force.rotation, thermal.rotation, kinematic.rotation),
    ]
    .into_iter()
    .enumerate()
    {
        lines.push(ffi::ResultLineDto {
            effect: 0,
            component: component as u8,
            value: f.formatted_displacement(),
            details: f
                .contributions
                .iter()
                .map(|item| {
                    format!(
                        "{}: ∫M·m ds = {}, вклад = {}",
                        item.segment_id,
                        crate::format_number(item.raw_integral),
                        crate::format_number(item.integral)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        });
        lines.push(ffi::ResultLineDto {
            effect: 1,
            component: component as u8,
            value: t.formatted_displacement(),
            details: t
                .contributions
                .iter()
                .map(|item| {
                    format!(
                        "{}: изгиб = {}, осевая часть = {}",
                        item.rod_id,
                        crate::format_number(item.bending_term),
                        crate::format_number(item.axial_term)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        });
        lines.push(ffi::ResultLineDto {
            effect: 2,
            component: component as u8,
            value: crate::format_number(k.total),
            details: k
                .contributions
                .iter()
                .map(|item| {
                    format!(
                        "{} {}: −({})·({}) = {}",
                        item.node_id,
                        item.direction.symbol(),
                        crate::format_number(item.unit_reaction),
                        crate::format_number(item.displacement),
                        crate::format_number(item.value)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        });
    }
    Ok(lines)
}

fn changed(document: &mut Document) {
    document.revision += 1;
    document.fem_result = None;
}

fn validate_draft(model: &StructuralModel) -> Result<(), String> {
    model.validate_unique_geometry()?;
    model.validate_hinges()?;
    Ok(())
}

fn document_add_node(document: &mut Document, id: &str, x: f64, y: f64) -> Result<(), String> {
    let old = document.data.model.clone();
    let id = if id.trim().is_empty() {
        next_node_id(&old.nodes)
    } else {
        id.trim().to_string()
    };
    document.data.model.nodes.push(Node {
        id,
        position: Point { x, y },
    });
    if let Err(error) = validate_draft(&document.data.model) {
        document.data.model = old;
        return Err(error);
    }
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

fn document_set_node(
    document: &mut Document,
    index: u32,
    id: &str,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let old = document.data.model.clone();
    let node = document
        .data
        .model
        .nodes
        .get_mut(index as usize)
        .ok_or("точка не найдена")?;
    node.id = id.trim().into();
    node.position = Point { x, y };
    if let Err(error) = validate_draft(&document.data.model) {
        document.data.model = old;
        return Err(error);
    }
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

fn document_remove_node(document: &mut Document, index: u32) -> Result<(), String> {
    let index = index as usize;
    if index >= document.data.model.nodes.len() {
        return Err("точка не найдена".into());
    }
    document
        .data
        .model
        .rods
        .retain(|rod| rod.start_node != index && rod.end_node != index);
    for rod in &mut document.data.model.rods {
        if rod.start_node > index {
            rod.start_node -= 1;
        }
        if rod.end_node > index {
            rod.end_node -= 1;
        }
    }
    document
        .data
        .model
        .supports
        .retain(|support| support.node != index);
    for support in &mut document.data.model.supports {
        if support.node > index {
            support.node -= 1;
        }
    }
    document.data.model.nodes.remove(index);
    document.data.model.target_node = document
        .data
        .model
        .target_node
        .min(document.data.model.nodes.len().saturating_sub(1));
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

fn document_add_rod(document: &mut Document, id: &str, start: u32, end: u32) -> Result<(), String> {
    let start = start as usize;
    let end = end as usize;
    if start >= document.data.model.nodes.len() || end >= document.data.model.nodes.len() {
        return Err("точка стержня не найдена".into());
    }
    let id = if id.trim().is_empty() {
        crate::rod_id_from_nodes(&document.data.model.nodes, start, end).unwrap_or_default()
    } else {
        id.trim().into()
    };
    document.data.model.rods.push(Rod::blank(id, start, end));
    if let Err(error) = validate_draft(&document.data.model) {
        document.data.model.rods.pop();
        return Err(error);
    }
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

fn document_set_rod(
    document: &mut Document,
    index: u32,
    id: &str,
    start: u32,
    end: u32,
    h: f64,
    hinge_start: bool,
    hinge_end: bool,
    ei_preset: i32,
    alpha_preset: i32,
) -> Result<(), String> {
    let old = document.data.model.clone();
    let rod = document
        .data
        .model
        .rods
        .get_mut(index as usize)
        .ok_or("стержень не найден")?;
    rod.id = id.trim().into();
    rod.start_node = start as usize;
    rod.end_node = end as usize;
    rod.section_height = h;
    rod.release_start_moment = hinge_start;
    rod.release_end_moment = hinge_end;
    rod.rigidity_preset_id = (ei_preset >= 0).then_some(ei_preset as u32);
    rod.thermal_preset_id = (alpha_preset >= 0).then_some(alpha_preset as u32);
    if let Err(error) = document.data.model.validate() {
        document.data.model = old;
        return Err(error);
    }
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

fn document_remove_rod(document: &mut Document, index: u32) -> Result<(), String> {
    let index = index as usize;
    if index >= document.data.model.rods.len() {
        return Err("стержень не найден".into());
    }
    document.data.model.rods.remove(index);
    document.data.fem.reconcile(&document.data.model);
    changed(document);
    Ok(())
}

/// Применяет обе таблицы за одну операцию. Сохранённые инженерные свойства
/// строк и нагрузки МКЭ остаются привязаны к своим строкам.
fn document_replace_geometry(
    document: &mut Document,
    nodes: Vec<ffi::GeometryNodeDto>,
    rods: Vec<ffi::GeometryRodDto>,
) -> Result<(), String> {
    if nodes.is_empty() && !rods.is_empty() {
        return Err("для стержней сначала задайте точки".into());
    }
    if let Some((index, _)) = rods
        .iter()
        .enumerate()
        .find(|(_, rod)| rod.start as usize >= nodes.len() || rod.end as usize >= nodes.len())
    {
        return Err(format!(
            "стержень в строке {} ссылается на отсутствующую точку",
            index + 1
        ));
    }
    let old = &document.data.model;
    if old.supports.iter().any(|item| item.node >= nodes.len()) {
        return Err(
            "удаляемая точка содержит заданное смещение опоры; сначала удалите эту связь".into(),
        );
    }
    let mut model = old.clone();
    model.nodes = nodes
        .iter()
        .map(|node| Node {
            id: node.id.trim().into(),
            position: Point {
                x: node.x,
                y: node.y,
            },
        })
        .collect();
    model.rods = rods
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut rod = old.rods.get(index).cloned().unwrap_or_else(|| {
                Rod::blank(String::new(), item.start as usize, item.end as usize)
            });
            rod.id = item.id.trim().into();
            rod.start_node = item.start as usize;
            rod.end_node = item.end as usize;
            rod.section_height = item.h;
            rod.release_start_moment = item.hinge_start;
            rod.release_end_moment = item.hinge_end;
            if index >= old.rods.len() {
                rod.rigidity_preset_id = model.rigidity_presets.first().map(|value| value.id);
                rod.thermal_preset_id = model.thermal_presets.first().map(|value| value.id);
            }
            rod
        })
        .collect();
    if model.nodes.is_empty() {
        model.target_node = 0;
    } else {
        model.target_node = model.target_node.min(model.nodes.len() - 1);
    }
    validate_draft(&model)?;
    let old_fem = &document.data.fem;
    let mut fem = old_fem.clone();
    fem.nodes = model
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let mut item = old_fem
                .nodes
                .get(index)
                .cloned()
                .unwrap_or_else(|| FemNode {
                    id: node.id.clone(),
                    support: SupportKind::Free,
                    normal_deg: 90.0,
                    fx: 0.0,
                    fy: 0.0,
                    mz: 0.0,
                });
            item.id = node.id.clone();
            item
        })
        .collect();
    fem.members = model
        .rods
        .iter()
        .enumerate()
        .map(|(index, rod)| {
            let start = &model.nodes[rod.start_node].id;
            let end = &model.nodes[rod.end_node].id;
            let mut item = old_fem
                .members
                .get(index)
                .cloned()
                .unwrap_or_else(|| FemMember {
                    id: rod.id.clone(),
                    start_id: start.clone(),
                    end_id: end.clone(),
                    ea: 0.0,
                    ei: 0.0,
                    qx: 0.0,
                    qy: 0.0,
                    point_loads: Vec::new(),
                });
            item.id = rod.id.clone();
            item.start_id = start.clone();
            item.end_id = end.clone();
            item
        })
        .collect();
    fem.target_node_id = model
        .nodes
        .get(model.target_node)
        .map_or(String::new(), |node| node.id.clone());
    document.data.model = model;
    document.data.fem = fem;
    changed(document);
    Ok(())
}

fn document_set_target(document: &mut Document, index: u32) -> Result<(), String> {
    if index as usize >= document.data.model.nodes.len() {
        return Err("искомая точка не найдена".into());
    }
    document.data.model.target_node = index as usize;
    document.data.fem.target_node_id = document.data.model.nodes[index as usize].id.clone();
    changed(document);
    Ok(())
}

fn document_set_property(
    document: &mut Document,
    alpha: bool,
    id: u32,
    name: &str,
    numeric: bool,
    factor: f64,
    mantissa: f64,
    exponent: i32,
) -> Result<(), String> {
    let value = PropertyValue {
        numeric,
        symbolic_factor: factor,
        mantissa,
        exponent,
    };
    value.validate()?;
    if name.trim().is_empty() {
        return Err("название свойства пусто".into());
    }
    let model = &mut document.data.model;
    let presets = if alpha {
        &mut model.thermal_presets
    } else {
        &mut model.rigidity_presets
    };
    if presets
        .iter()
        .any(|item| item.id != id && item.name.eq_ignore_ascii_case(name.trim()))
    {
        return Err("название свойства повторяется".into());
    }
    if let Some(preset) = presets.iter_mut().find(|item| item.id == id) {
        preset.name = name.trim().into();
        preset.value = value;
    } else {
        presets.push(PropertyPreset {
            id,
            name: name.trim().into(),
            value,
        });
    }
    changed(document);
    Ok(())
}

fn document_remove_property(document: &mut Document, alpha: bool, id: u32) -> Result<(), String> {
    let model = &mut document.data.model;
    if model.rods.iter().any(|rod| {
        if alpha {
            rod.thermal_preset_id == Some(id)
        } else {
            rod.rigidity_preset_id == Some(id)
        }
    }) {
        return Err("свойство назначено стержню".into());
    }
    let presets = if alpha {
        &mut model.thermal_presets
    } else {
        &mut model.rigidity_presets
    };
    let before = presets.len();
    presets.retain(|item| item.id != id);
    if presets.len() == before {
        return Err("свойство не найдено".into());
    }
    changed(document);
    Ok(())
}

fn support_code(value: SupportKind) -> i32 {
    match value {
        SupportKind::Free => 0,
        SupportKind::Fixed => 1,
        SupportKind::Pin => 2,
        SupportKind::Roller => 3,
        SupportKind::Guided => 4,
    }
}

fn support_from_code(value: i32) -> Result<SupportKind, String> {
    match value {
        0 => Ok(SupportKind::Free),
        1 => Ok(SupportKind::Fixed),
        2 => Ok(SupportKind::Pin),
        3 => Ok(SupportKind::Roller),
        4 => Ok(SupportKind::Guided),
        _ => Err("неизвестный тип опоры".into()),
    }
}

fn document_set_fem_node(
    document: &mut Document,
    index: u32,
    support: i32,
    normal_deg: f64,
    fx: f64,
    fy: f64,
    mz: f64,
) -> Result<(), String> {
    if ![normal_deg, fx, fy, mz]
        .iter()
        .all(|value| value.is_finite())
    {
        return Err("параметры узла должны быть конечными числами".into());
    }
    document.data.fem.reconcile(&document.data.model);
    let node = document
        .data
        .fem
        .nodes
        .get_mut(index as usize)
        .ok_or("точка не найдена")?;
    node.support = support_from_code(support)?;
    node.normal_deg = normal_deg;
    node.fx = fx;
    node.fy = fy;
    node.mz = mz;
    changed(document);
    Ok(())
}

fn document_set_fem_rod(
    document: &mut Document,
    index: u32,
    qx: f64,
    qy: f64,
    ea: f64,
    ei: f64,
) -> Result<(), String> {
    if ![qx, qy, ea, ei].iter().all(|value| value.is_finite()) || ea < 0.0 || ei < 0.0 {
        return Err("нагрузки должны быть конечными, жёсткости неотрицательными".into());
    }
    document.data.fem.reconcile(&document.data.model);
    let rod = document
        .data
        .fem
        .members
        .get_mut(index as usize)
        .ok_or("стержень не найден")?;
    rod.qx = qx;
    rod.qy = qy;
    rod.ea = ea;
    rod.ei = ei;
    changed(document);
    Ok(())
}

fn document_add_point_load(
    document: &mut Document,
    rod: u32,
    xi: f64,
    fx: f64,
    fy: f64,
    mz: f64,
) -> Result<(), String> {
    if !(0.0..=1.0).contains(&xi) || ![fx, fy, mz].iter().all(|value| value.is_finite()) {
        return Err("положение должно быть от 0 до 1, нагрузки конечными".into());
    }
    document.data.fem.reconcile(&document.data.model);
    let member = document
        .data
        .fem
        .members
        .get_mut(rod as usize)
        .ok_or("стержень не найден")?;
    member.point_loads.push(PointLoad { xi, fx, fy, mz });
    changed(document);
    Ok(())
}

fn document_remove_point_load(document: &mut Document, rod: u32, index: u32) -> Result<(), String> {
    let member = document
        .data
        .fem
        .members
        .get_mut(rod as usize)
        .ok_or("стержень не найден")?;
    if index as usize >= member.point_loads.len() {
        return Err("нагрузка не найдена".into());
    }
    member.point_loads.remove(index as usize);
    changed(document);
    Ok(())
}

fn document_set_temperature(
    document: &mut Document,
    rod: u32,
    active: bool,
    upper: f64,
    lower: f64,
) -> Result<(), String> {
    if !upper.is_finite() || !lower.is_finite() {
        return Err("температура некорректна".into());
    }
    let rod = document
        .data
        .model
        .rods
        .get_mut(rod as usize)
        .ok_or("стержень не найден")?;
    rod.temperature = active.then_some(TemperatureLoad {
        upper_or_left: upper,
        lower_or_right: lower,
    });
    changed(document);
    Ok(())
}

fn document_set_diagram(
    document: &mut Document,
    rod: u32,
    field: u8,
    kind: u8,
    left: f64,
    right: f64,
    q_signed: f64,
    extremum: bool,
) -> Result<(), String> {
    let kind = match kind {
        0 => DiagramKind::Constant,
        1 => DiagramKind::Linear,
        2 => DiagramKind::Parabolic,
        _ => return Err("тип эпюры неизвестен".into()),
    };
    if !q_signed.is_finite() {
        return Err("q некорректно".into());
    }
    let q = (kind == DiagramKind::Parabolic).then_some(q_signed.abs());
    let q_fiber = if kind == DiagramKind::Parabolic {
        Some(if q_signed < 0.0 {
            FiberSide::UpperOrLeft
        } else {
            FiberSide::LowerOrRight
        })
    } else {
        None
    };
    let diagram = Diagram::new(
        kind,
        Ordinate::from_signed(left)?,
        Ordinate::from_signed(right)?,
        q,
        q_fiber,
        extremum && kind == DiagramKind::Parabolic,
    )?;
    let rod = document
        .data
        .model
        .rods
        .get_mut(rod as usize)
        .ok_or("стержень не найден")?;
    *selected_diagram_mut(rod, field).ok_or("неизвестная эпюра")? = diagram;
    changed(document);
    Ok(())
}

fn document_set_support_motion(
    document: &mut Document,
    node: u32,
    direction: u8,
    normal_deg: f64,
    displacement: f64,
    rx: f64,
    ry: f64,
    rphi: f64,
) -> Result<(), String> {
    if node as usize >= document.data.model.nodes.len() {
        return Err("точка не найдена".into());
    }
    if ![normal_deg, displacement, rx, ry, rphi]
        .iter()
        .all(|value| value.is_finite())
    {
        return Err("параметры связи должны быть конечными".into());
    }
    let direction = match direction {
        0 => SupportDirection::Horizontal,
        1 => SupportDirection::Vertical,
        2 => SupportDirection::Rotation,
        3 => SupportDirection::Normal,
        _ => return Err("неизвестное направление опорной связи".into()),
    };
    let supports = &mut document.data.model.supports;
    supports.retain(|item| item.node != node as usize || item.direction != direction);
    supports.push(SupportMotion {
        node: node as usize,
        direction,
        normal_deg,
        displacement,
        unit_x_reaction: rx,
        unit_y_reaction: ry,
        unit_rotation_reaction: rphi,
    });
    changed(document);
    Ok(())
}

fn document_remove_support_motion(
    document: &mut Document,
    node: u32,
    direction: u8,
) -> Result<(), String> {
    let before = document.data.model.supports.len();
    document.data.model.supports.retain(|motion| {
        motion.node != node as usize
            || match motion.direction {
                SupportDirection::Horizontal => 0,
                SupportDirection::Vertical => 1,
                SupportDirection::Rotation => 2,
                SupportDirection::Normal => 3,
            } != direction
    });
    if document.data.model.supports.len() == before {
        return Err("связь не найдена".into());
    }
    changed(document);
    Ok(())
}

fn document_set_effects(document: &mut Document, force: bool, thermal: bool, kinematic: bool) {
    document.data.effects = EffectsSelection {
        force,
        thermal,
        kinematic,
    };
    document.revision += 1;
}

fn document_set_rounding(document: &mut Document, enabled: bool, decimals: u8) {
    document.data.calculation_rounding = enabled;
    document.data.calculation_decimals = decimals.min(10);
    document.revision += 1;
}

fn document_set_unit_selection(document: &mut Document, x: bool, y: bool, phi: bool) {
    document.data.fem.units.x = x;
    document.data.fem.units.y = y;
    document.data.fem.units.rotation = phi;
    changed(document);
}

fn document_solve_fem(document: &mut Document) -> Result<(), String> {
    document.data.fem.reconcile(&document.data.model);
    let result = document.data.fem.solve(&document.data.model)?;
    document.fem_result = Some(result);
    Ok(())
}

fn document_transfer_fem(document: &mut Document) -> Result<(), String> {
    let result = document
        .fem_result
        .as_ref()
        .ok_or("сначала выполните расчёт МКЭ")?;
    let decimals = document
        .data
        .calculation_rounding
        .then_some(document.data.calculation_decimals);
    document.data.model =
        result.apply_to_model_with_rounding(&document.data.model, &document.data.fem, decimals)?;
    document.revision += 1;
    Ok(())
}

fn document_save(document: &mut Document, path: &str) -> Result<String, String> {
    let path = if path.is_empty() {
        document.path.clone().ok_or("выберите файл проекта")?
    } else {
        PathBuf::from(path)
    };
    let saved = save_project_to_path(&path, &document.data)?;
    document.path = Some(saved.clone());
    Ok(saved.to_string_lossy().into_owned())
}

fn document_export_word(document: &Document) -> Result<String, String> {
    let path = document.path.as_ref().ok_or("сначала сохраните проект")?;
    let decimals = document
        .data
        .calculation_rounding
        .then_some(document.data.calculation_decimals);
    let bytes = build_word_report_with_rounding(
        &document.data.model,
        document.data.rigidity,
        document.data.thermal_expansion,
        document.data.effects,
        decimals,
    )?;
    let output = save_word_report_for_path(path, &bytes)?;
    Ok(output.to_string_lossy().into_owned())
}

fn document_export_fem_word(document: &Document, section_flags: u8) -> Result<String, String> {
    let path = document.path.as_ref().ok_or("сначала сохраните проект")?;
    let result = document
        .fem_result
        .as_ref()
        .ok_or("сначала выполните расчёт МКЭ")?;
    let sections = FemReportSections {
        geometry: section_flags & 1 != 0,
        loads: section_flags & 2 != 0,
        stability: section_flags & 4 != 0,
        reactions: section_flags & 8 != 0,
        actual_diagrams: section_flags & 16 != 0,
        unit_diagrams: section_flags & 32 != 0,
    };
    let bytes = build_fem_word_report(&document.data.model, &document.data.fem, result, sections)?;
    let output = save_fem_word_report_for_path(path, &bytes)?;
    Ok(output.to_string_lossy().into_owned())
}

fn document_export_legacy(document: &Document) -> Result<String, String> {
    let path = document.path.as_ref().ok_or("сначала сохраните проект")?;
    let output = export_for_v050_for_path(path, &document.data)?;
    Ok(output.to_string_lossy().into_owned())
}

fn import_xlsx_draft(path: &str) -> Result<Box<ImportDraft>, String> {
    import_xlsx(Path::new(path))
        .map(|imported| Box::new(ImportDraft { imported }))
        .map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })
}

fn import_csv_draft(path: &str) -> Result<Box<ImportDraft>, String> {
    import_csv_directory(Path::new(path))
        .map(|imported| Box::new(ImportDraft { imported }))
        .map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })
}

fn import_draft_summary(draft: &ImportDraft) -> String {
    let mut text = format!(
        "Точек: {}, стержней: {}.\n",
        draft.imported.model.nodes.len(),
        draft.imported.model.rods.len()
    );
    for (name, count) in &draft.imported.counts {
        text.push_str(&format!("{name}: {count} строк\n"));
    }
    text
}

fn import_draft_nodes(draft: &ImportDraft) -> Vec<ffi::NodeDto> {
    let temp = Document {
        data: ProjectData {
            model: draft.imported.model.clone(),
            fem: draft.imported.fem.clone(),
            ..blank_data()
        },
        path: None,
        fem_result: None,
        revision: 0,
    };
    document_nodes(&temp)
}

fn import_draft_rods(draft: &ImportDraft) -> Vec<ffi::RodDto> {
    let temp = Document {
        data: ProjectData {
            model: draft.imported.model.clone(),
            fem: draft.imported.fem.clone(),
            ..blank_data()
        },
        path: None,
        fem_result: None,
        revision: 0,
    };
    document_rods(&temp)
}

fn document_apply_import(document: &mut Document, draft: Box<ImportDraft>) {
    document.data.model = draft.imported.model;
    document.data.fem = draft.imported.fem;
    document.data.rigidity = Rigidity::Symbolic;
    document.data.thermal_expansion = ThermalExpansion::Symbolic;
    document.path = None;
    changed(document);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_table_replaces_both_catalogues_and_assignments_atomically() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/example-frame.mohr.ron");
        let mut document = load_document(path.to_str().unwrap()).unwrap();
        let rod_count = document.data.model.rods.len();
        let rigidity = vec![
            ffi::PropertyDto {
                id: 10,
                name: "Основная EI".into(),
                numeric: false,
                factor: 2.0,
                mantissa: 1.0,
                exponent: 0,
            },
            ffi::PropertyDto {
                id: 11,
                name: "Численная EI".into(),
                numeric: true,
                factor: 1.0,
                mantissa: 3.0,
                exponent: 5,
            },
        ];
        let thermal = vec![
            ffi::PropertyDto {
                id: 20,
                name: "Основная α".into(),
                numeric: false,
                factor: 1.5,
                mantissa: 1.0,
                exponent: -5,
            },
            ffi::PropertyDto {
                id: 21,
                name: "Численная α".into(),
                numeric: true,
                factor: 1.0,
                mantissa: 1.2,
                exponent: -5,
            },
        ];
        let ei_ids = (0..rod_count)
            .map(|index| if index % 2 == 0 { 10 } else { 11 })
            .collect();
        let alpha_ids = (0..rod_count)
            .map(|index| if index % 2 == 0 { 21 } else { 20 })
            .collect();
        document_replace_properties(&mut document, rigidity, thermal, ei_ids, alpha_ids).unwrap();
        assert_eq!(
            document.data.model.rigidity_presets[0]
                .value
                .symbolic_factor,
            2.0
        );
        assert_eq!(document.data.model.thermal_presets[1].value.exponent, -5);
        assert_eq!(document.data.model.rods[0].rigidity_preset_id, Some(10));
        assert_eq!(document.data.model.rods[0].thermal_preset_id, Some(21));

        let unchanged = document.data.clone();
        let revision = document.revision;
        let bad_rigidity = vec![
            ffi::PropertyDto {
                id: 10,
                name: "Дубликат".into(),
                numeric: false,
                factor: 1.0,
                mantissa: 1.0,
                exponent: 0,
            },
            ffi::PropertyDto {
                id: 10,
                name: "Дубликат".into(),
                numeric: false,
                factor: 1.0,
                mantissa: 1.0,
                exponent: 0,
            },
        ];
        let thermal = document_properties(&document, true);
        assert!(document_replace_properties(
            &mut document,
            bad_rigidity,
            thermal,
            vec![10; rod_count],
            vec![20; rod_count]
        )
        .is_err());
        assert_eq!(document.data, unchanged);
        assert_eq!(document.revision, revision);
    }

    #[test]
    fn legacy_example_survives_bridge_save_and_reopen() {
        let original =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/example-frame.mohr.ron");
        let mut document = load_document(original.to_str().unwrap()).unwrap();
        let before = document_results(&document).unwrap();
        assert_eq!(before.len(), 9);
        let horizontal = before
            .iter()
            .find(|line| line.effect == 0 && line.component == 0)
            .unwrap();
        assert_eq!(horizontal.value, "310 / 6EI");
        assert!(horizontal.details.contains("AB"));
        assert!(horizontal.details.contains("BC"));
        assert!(horizontal.details.contains("CD"));
        let middle = document_manual_curve(&document, 0)
            .into_iter()
            .find(|point| point.rod_index == 1 && (point.t - 0.5).abs() < 1e-12)
            .unwrap();
        assert!((middle.value + 27.5).abs() < 1e-12);
        let folder = std::env::temp_dir().join(format!("mohr-qt-bridge-{}", std::process::id()));
        let target = folder.join("roundtrip.mohr.ron");
        let saved = document_save(&mut document, target.to_str().unwrap()).unwrap();
        let reopened = load_document(&saved).unwrap();
        let after = document_results(&reopened).unwrap();
        for (left, right) in before.iter().zip(&after) {
            assert_eq!(
                (left.effect, left.component),
                (right.effect, right.component)
            );
            assert_eq!(left.value, right.value);
            assert_eq!(left.details, right.details);
        }
        assert_eq!(
            document_nodes(&document).len(),
            document_nodes(&reopened).len()
        );
        assert_eq!(
            document_rods(&document).len(),
            document_rods(&reopened).len()
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn combined_geometry_edit_is_atomic_and_preserves_engineering_rows() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/example-frame.mohr.ron");
        let mut document = load_document(path.to_str().unwrap()).unwrap();
        document_set_fem_node(&mut document, 0, 1, 90.0, 12.0, 0.0, 0.0).unwrap();
        document_add_point_load(&mut document, 0, 0.4, 2.0, 0.0, 0.0).unwrap();
        let original = document.data.clone();
        let revision = document.revision;
        let mut nodes = document_nodes(&document)
            .into_iter()
            .map(|node| ffi::GeometryNodeDto {
                id: node.id,
                x: node.x,
                y: node.y,
            })
            .collect::<Vec<_>>();
        let rods = document_rods(&document)
            .into_iter()
            .map(|rod| ffi::GeometryRodDto {
                id: rod.id,
                start: rod.start,
                end: rod.end,
                h: rod.h,
                hinge_start: rod.hinge_start,
                hinge_end: rod.hinge_end,
            })
            .collect::<Vec<_>>();
        nodes[1].x = nodes[0].x;
        nodes[1].y = nodes[0].y;
        assert!(document_replace_geometry(&mut document, nodes.clone(), rods.clone()).is_err());
        assert_eq!(document.data, original);
        assert_eq!(document.revision, revision);
        nodes[1].id = "B2".into();
        nodes[1].x = 0.0;
        nodes[1].y = 2.0;
        document_replace_geometry(&mut document, nodes, rods).unwrap();
        assert_eq!(document.data.model.nodes[1].id, "B2");
        assert_eq!(
            document.data.model.rods[1].load_diagram,
            original.model.rods[1].load_diagram
        );
        assert_eq!(document.data.fem.nodes[0].fx, 12.0);
        assert_eq!(document.data.fem.members[0].point_loads.len(), 1);
        assert_eq!(document.data.fem.members[0].end_id, "B2");
        assert!(document.data.fem.is_aligned(&document.data.model));
    }
}

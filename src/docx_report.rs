//! Формирование переносимого отчёта Microsoft Word без внешнего рантайма.
//!
//! DOCX является ZIP-контейнером с XML Open Packaging Convention. Здесь
//! используется несжатый ZIP: файл немного больше, зато одинаково создаётся
//! на macOS и Windows и не требует установленного Word или Python.

use std::io::Cursor;

use crate::{
    calculate_displacement_components_with_rounding,
    calculate_kinematic_displacement_components_with_rounding,
    calculate_thermal_displacement_components_with_rounding,
    fem::{
        FemProject, FemResult, LoadCaseResult, SectionForces, StiffnessMode, SupportKind,
        UnitComponent,
    },
    format_number, CalculationResult, Diagram, DisplacementComponents, EffectsSelection, FiberSide,
    IntegrationMethod, KinematicDisplacementComponents, KinematicResult, Rigidity, StructuralModel,
    SupportDirection, ThermalCalculationResult, ThermalDisplacementComponents, ThermalExpansion,
};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

const REPORT_IMAGE_WIDTH: u32 = 1000;
const REPORT_IMAGE_HEIGHT: u32 = 650;

/// Независимый состав отчёта МКЭ; выбор сохраняется в открытом окне калькулятора.
#[derive(Debug, Clone, Copy)]
pub struct FemReportSections {
    pub geometry: bool,
    pub loads: bool,
    pub stability: bool,
    pub reactions: bool,
    pub actual_diagrams: bool,
    pub unit_diagrams: bool,
}

impl Default for FemReportSections {
    fn default() -> Self {
        Self {
            geometry: true,
            loads: true,
            stability: true,
            reactions: true,
            actual_diagrams: true,
            unit_diagrams: true,
        }
    }
}

impl FemReportSections {
    fn any(self) -> bool {
        self.geometry
            || self.loads
            || self.stability
            || self.reactions
            || self.actual_diagrams
            || self.unit_diagrams
    }
}

/// Отдельный отчёт МКЭ с независимым выбором разделов.
pub fn build_fem_word_report(
    model: &StructuralModel,
    input: &FemProject,
    result: &FemResult,
    sections: FemReportSections,
) -> Result<Vec<u8>, String> {
    model.validate()?;
    if !input.is_aligned(model) {
        return Err("геометрия МКЭ не совпадает с текущей рамой".into());
    }
    if !sections.any() {
        return Err("отметьте хотя бы один раздел отчёта МКЭ".into());
    }
    let mut body = String::new();
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    body.push_str(&paragraph(
        "Расчёт плоской стержневой рамы методом конечных элементов",
        Some("Title"),
        true,
    ));
    body.push_str(&paragraph(
        &format!("Целевая точка: {}. Плоский элемент Эйлера — Бернулли; три степени свободы в узле (uₓ, uᵧ, φ).", input.target_node_id),
        None, false,
    ));
    body.push_str(&paragraph(
        "Реакции в таблицах приведены как знаковые проекции на глобальные оси X, Y и Mz. Настройка стрелок меняет только схематическое представление.",
        None, false,
    ));

    if sections.geometry {
        body.push_str(&heading("Геометрия рамы", 1));
        let rows = model
            .nodes
            .iter()
            .map(|n| {
                vec![
                    n.id.clone(),
                    format_number(n.position.x),
                    format_number(n.position.y),
                ]
            })
            .collect::<Vec<_>>();
        body.push_str(&table(&["Точка", "X", "Y"], &rows));
        let rows = model
            .rods
            .iter()
            .map(|r| {
                vec![
                    r.id.clone(),
                    model.nodes[r.start_node].id.clone(),
                    model.nodes[r.end_node].id.clone(),
                    format_number(model.rod_length(r).unwrap_or(0.0)),
                    if r.release_start_moment {
                        "шарнир".into()
                    } else {
                        "жёстко".into()
                    },
                    if r.release_end_moment {
                        "шарнир".into()
                    } else {
                        "жёстко".into()
                    },
                ]
            })
            .collect::<Vec<_>>();
        body.push_str(&table(
            &["Стержень", "Начало", "Конец", "L", "Начало M", "Конец M"],
            &rows,
        ));
        let rid = images.len() + 2;
        images.push(("frame.png".into(), render_fem_scheme_png(model, input)?));
        body.push_str(&fem_image_paragraph(rid, "Геометрия рамы"));
    }
    if sections.loads {
        body.push_str(&heading("Опоры, жёсткости и нагрузки", 1));
        if result.stiffness_mode == StiffnessMode::Determinate {
            body.push_str(&paragraph("Рама статически определима: эпюры и реакции получены без задания EA и EI. Аналитическая жёсткость остаётся в основном расчёте перемещений.", None, false));
        }
        let rows = input
            .nodes
            .iter()
            .map(|n| {
                vec![
                    n.id.clone(),
                    n.support.label().into(),
                    if matches!(n.support, SupportKind::Roller | SupportKind::Guided) {
                        format_number(n.normal_deg)
                    } else {
                        "—".into()
                    },
                    format_number(n.fx),
                    format_number(n.fy),
                    format_number(n.mz),
                ]
            })
            .collect::<Vec<_>>();
        body.push_str(&table(
            &["Точка", "Опора", "Нормаль, °", "Fₓ", "Fᵧ", "Mᶻ"],
            &rows,
        ));
        let rows = input
            .members
            .iter()
            .map(|m| {
                vec![
                    m.id.clone(),
                    if m.ea > 0.0 {
                        format_number(m.ea)
                    } else {
                        "—".into()
                    },
                    if m.ei > 0.0 {
                        format_number(m.ei)
                    } else {
                        "—".into()
                    },
                    format_number(m.qx),
                    format_number(m.qy),
                ]
            })
            .collect::<Vec<_>>();
        body.push_str(&table(&["Стержень", "EA", "EI", "qₓ", "qᵧ"], &rows));
        let rows = input
            .members
            .iter()
            .flat_map(|m| {
                m.point_loads.iter().map(move |p| {
                    vec![
                        m.id.clone(),
                        format_number(p.xi),
                        format_number(p.fx),
                        format_number(p.fy),
                        format_number(p.mz),
                    ]
                })
            })
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            body.push_str(&caption("Сосредоточенные воздействия на стержнях"));
            body.push_str(&table(&["Стержень", "x/L", "Fₓ", "Fᵧ", "Mᶻ"], &rows));
        }
    }
    if sections.stability {
        body.push_str(&heading("Проверка расчётной модели", 1));
        body.push_str(&paragraph(&format!(
            "Всего степеней свободы: {}. Связей: {}. Свободных степеней свободы: {}. Матрица расчёта невырождена для заданных опор.",
            result.total_dof, result.constraints, result.total_dof - result.constraints,
        ), None, false));
    }
    let actual_active = input
        .nodes
        .iter()
        .any(|n| n.fx != 0.0 || n.fy != 0.0 || n.mz != 0.0)
        || input.members.iter().any(|m| {
            m.qx != 0.0
                || m.qy != 0.0
                || m.point_loads
                    .iter()
                    .any(|p| p.fx != 0.0 || p.fy != 0.0 || p.mz != 0.0)
        });
    if sections.reactions && (actual_active || !result.units.is_empty()) {
        body.push_str(&heading("Опорные реакции", 1));
        if actual_active {
            append_fem_reactions(
                &mut body,
                model,
                input,
                "Грузовая система",
                &result.actual,
                None,
            );
        }
        for unit in &result.units {
            append_fem_reactions(
                &mut body,
                model,
                input,
                &format!("Единичная система {}", unit.component.label()),
                &unit.result,
                Some(unit.component),
            );
        }
    }
    if sections.actual_diagrams && actual_active {
        body.push_str(&heading("Эпюры грузовой системы", 1));
        for quantity in [FemQuantity::Moment, FemQuantity::Normal, FemQuantity::Shear] {
            append_fem_diagram(
                &mut body,
                &mut images,
                model,
                &result.actual,
                quantity,
                "Грузовая система",
            )?;
        }
    }
    if sections.unit_diagrams && !result.units.is_empty() {
        body.push_str(&heading("Единичные эпюры для основного расчёта", 1));
        for unit in &result.units {
            for quantity in [FemQuantity::Moment, FemQuantity::Normal] {
                append_fem_diagram(
                    &mut body,
                    &mut images,
                    model,
                    &unit.result,
                    quantity,
                    &format!("Единичная система {}", unit.component.label()),
                )?;
            }
        }
    }
    body.push_str(r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1008" w:right="1008" w:bottom="1008" w:left="1008"/></w:sectPr>"#);
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}</w:body></w:document>"#
    );
    let mut relationships = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    );
    for (i, (name, _)) in images.iter().enumerate() {
        relationships.push_str(&format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/{}"/>"#, i + 2, name));
    }
    relationships.push_str("</Relationships>");
    let mut archive = StoredZip::default();
    archive.add("[Content_Types].xml", content_types_xml().as_bytes());
    archive.add("_rels/.rels", package_relationships_xml().as_bytes());
    archive.add("docProps/core.xml", core_properties_xml().as_bytes());
    archive.add("docProps/app.xml", app_properties_xml().as_bytes());
    archive.add("word/document.xml", document_xml.as_bytes());
    archive.add("word/_rels/document.xml.rels", relationships.as_bytes());
    archive.add("word/styles.xml", styles_xml().as_bytes());
    for (name, bytes) in images {
        archive.add(&format!("word/media/{name}"), &bytes);
    }
    archive.finish()
}

#[derive(Clone, Copy)]
enum FemQuantity {
    Moment,
    Normal,
    Shear,
}

impl FemQuantity {
    fn label(self) -> &'static str {
        match self {
            Self::Moment => "M",
            Self::Normal => "N",
            Self::Shear => "Q",
        }
    }
    fn value(self, section: SectionForces) -> f64 {
        match self {
            Self::Moment => section.m,
            Self::Normal => section.n,
            Self::Shear => section.q,
        }
    }
    fn color(self) -> Rgb<u8> {
        match self {
            Self::Moment => Rgb([202, 96, 32]),
            Self::Normal => Rgb([128, 73, 170]),
            Self::Shear => Rgb([155, 125, 12]),
        }
    }
}

fn append_fem_reactions(
    body: &mut String,
    model: &StructuralModel,
    input: &FemProject,
    title: &str,
    case: &LoadCaseResult,
    unit: Option<UnitComponent>,
) {
    body.push_str(&heading(title, 2));
    let rows = model
        .nodes
        .iter()
        .zip(&input.nodes)
        .zip(&case.reactions)
        .filter(|((_, data), _)| data.support != SupportKind::Free)
        .map(|((node, data), r)| {
            vec![
                node.id.clone(),
                format_number(r[0]),
                format_number(r[1]),
                format_number(r[2]),
                if matches!(data.support, SupportKind::Roller | SupportKind::Guided) {
                    let angle = data.normal_deg.to_radians();
                    format_number(r[0] * angle.cos() + r[1] * angle.sin())
                } else {
                    "—".into()
                },
            ]
        })
        .collect::<Vec<_>>();
    body.push_str(&table(&["Опора", "Rₓ", "Rᵧ", "Mᶻ", "Rₙ"], &rows));
    body.push_str(&paragraph("Подробный баланс каждого узла: R = Σ усилий на концах примыкающих стержней − P. Нулевые слагаемые показаны.", None, false));
    let rows = case
        .reaction_steps(model, input, unit)
        .into_iter()
        .map(|step| {
            let mut row = vec![step.node_id];
            for i in 0..3 {
                row.push(format!(
                    "{} − {} = {}",
                    format_number(step.member_end_forces[i]),
                    format_number(step.applied_load[i]),
                    format_number(step.reaction[i])
                ));
            }
            row
        })
        .collect::<Vec<_>>();
    body.push_str(&table(
        &["Узел", "Rₓ = ΣFₓ − Pₓ", "Rᵧ = ΣFᵧ − Pᵧ", "Mᶻ = ΣM − Pᶻ"],
        &rows,
    ));
}

fn append_fem_diagram(
    body: &mut String,
    images: &mut Vec<(String, Vec<u8>)>,
    model: &StructuralModel,
    case: &LoadCaseResult,
    quantity: FemQuantity,
    title: &str,
) -> Result<(), String> {
    let heading_text = format!("{title}: эпюра {}", quantity.label());
    body.push_str(&heading(&heading_text, 2));
    let rid = images.len() + 2;
    let name = format!("fem_epure_{}.png", rid);
    images.push((name, render_fem_epure_png(model, case, quantity)?));
    body.push_str(&fem_image_paragraph(rid, &heading_text));
    let rows = model
        .rods
        .iter()
        .zip(&case.members)
        .map(|(rod, member)| {
            vec![
                rod.id.clone(),
                format_number(quantity.value(member.at(0.0))),
                format_number(quantity.value(member.at(member.length / 2.0))),
                format_number(quantity.value(member.at(member.length))),
            ]
        })
        .collect::<Vec<_>>();
    body.push_str(&table(&["Стержень", "Начало", "Середина", "Конец"], &rows));
    body.push_str(&paragraph("Подробные формулы усилий в локальной координате x: H(x−a)=0 слева от нагрузки и 1 справа. Приведены характерные сечения и внутренние экстремумы момента.", None, false));
    for (rod, member) in model.rods.iter().zip(&case.members) {
        body.push_str(&heading(
            &format!("Стержень {} · эпюра {}", rod.id, quantity.label()),
            3,
        ));
        body.push_str(&paragraph(
            &format!(
                "L = {}; qₓ = {}; qᵧ = {}; концевые усилия [N₀,Q₀,M₀,Nₗ,Qₗ,Mₗ] = [{}].",
                format_number(member.length),
                format_number(member.qx_local),
                format_number(member.qy_local),
                member
                    .end_forces
                    .iter()
                    .map(|v| format_number(*v))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            None,
            false,
        ));
        body.push_str(&calculation_paragraph(
            &member.calculation_formulas()[match quantity {
                FemQuantity::Normal => 0,
                FemQuantity::Shear => 1,
                FemQuantity::Moment => 2,
            }],
        ));
        let rows = member
            .calculation_samples()
            .into_iter()
            .map(|(x, value)| vec![x, format_number(quantity.value(value))])
            .collect::<Vec<_>>();
        body.push_str(&table(&["Сечение x", quantity.label()], &rows));
    }
    Ok(())
}

fn fem_image_paragraph(rid: usize, description: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="5943600" cy="3863340"/><wp:docPr id="{rid}" name="Эпюра" descr="{}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{rid}" name="fem.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="5943600" cy="3863340"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
        xml_escape(description)
    )
}

fn render_fem_epure_png(
    model: &StructuralModel,
    case: &LoadCaseResult,
    quantity: FemQuantity,
) -> Result<Vec<u8>, String> {
    let mut image = RgbImage::from_pixel(
        REPORT_IMAGE_WIDTH,
        REPORT_IMAGE_HEIGHT,
        Rgb([255, 255, 255]),
    );
    let min_x = model
        .nodes
        .iter()
        .map(|n| n.position.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = model
        .nodes
        .iter()
        .map(|n| n.position.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = model
        .nodes
        .iter()
        .map(|n| n.position.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = model
        .nodes
        .iter()
        .map(|n| n.position.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);
    let scale = ((REPORT_IMAGE_WIDTH as f64 - 300.0) / range_x)
        .min((REPORT_IMAGE_HEIGHT as f64 - 240.0) / range_y);
    let ox = (REPORT_IMAGE_WIDTH as f64 - (max_x - min_x) * scale) / 2.0;
    let oy = (REPORT_IMAGE_HEIGHT as f64 - (max_y - min_y) * scale) / 2.0;
    let point = |x: f64, y: f64| -> (i32, i32) {
        (
            (ox + (x - min_x) * scale).round() as i32,
            (REPORT_IMAGE_HEIGHT as f64 - oy - (y - min_y) * scale).round() as i32,
        )
    };
    let max = case
        .members
        .iter()
        .flat_map(|m| (0..=80).map(move |i| quantity.value(m.at(m.length * i as f64 / 80.0)).abs()))
        .fold(0.0_f64, f64::max);
    let ordinate_scale = if max < 1e-12 { 0.0 } else { 95.0 / max };
    let ink = Rgb([45, 54, 64]);
    let color = quantity.color();
    for (rod, member) in model.rods.iter().zip(&case.members) {
        let a = model.nodes[rod.start_node].position;
        let b = model.nodes[rod.end_node].position;
        let start = point(a.x, a.y);
        let end = point(b.x, b.y);
        draw_line(&mut image, start, end, ink, 3);
        let dx = (end.0 - start.0) as f64;
        let dy = (end.1 - start.1) as f64;
        let length = dx.hypot(dy).max(1.0);
        let normal = (-dy / length, dx / length);
        let ordinate_point = |t: f64, value: f64| -> (i32, i32) {
            (
                (start.0 as f64 + dx * t + normal.0 * value * ordinate_scale).round() as i32,
                (start.1 as f64 + dy * t + normal.1 * value * ordinate_scale).round() as i32,
            )
        };
        let mut samples = (0..=80).map(|i| i as f64 / 80.0).collect::<Vec<_>>();
        for load in &member.point_loads_local {
            samples.push((load.xi - 1e-8).max(0.0));
            samples.push((load.xi + 1e-8).min(1.0));
        }
        samples.sort_by(|a, b| a.total_cmp(b));
        let mut previous = None;
        for t in samples {
            let value = quantity.value(member.at(t * member.length));
            let p = ordinate_point(t, value);
            if let Some(last) = previous {
                draw_line(&mut image, last, p, color, 2);
            }
            previous = Some(p);
        }
        draw_text_centered(
            &mut image,
            &rod.id,
            ((start.0 + end.0) / 2, (start.1 + end.1) / 2 - 18),
            2,
            ink,
        );
    }
    for (index, node) in model.nodes.iter().enumerate() {
        let p = point(node.position.x, node.position.y);
        draw_filled_circle(&mut image, p, 5, Rgb([255, 255, 255]));
        draw_circle(&mut image, p, 5, ink, 2);
        if model.is_internal_hinge_node(index) {
            draw_filled_circle(&mut image, p, 8, Rgb([255, 255, 255]));
            draw_circle(&mut image, p, 8, ink, 2);
        }
        draw_text(&mut image, &node.id, (p.0 + 9, p.1 + 5), 2, ink);
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|error| format!("не удалось подготовить эпюру для Word: {error}"))?;
    Ok(bytes.into_inner())
}

fn render_fem_scheme_png(model: &StructuralModel, input: &FemProject) -> Result<Vec<u8>, String> {
    let base = render_scheme_png(model)?;
    let mut image = image::load_from_memory(&base)
        .map_err(|error| format!("не удалось открыть изображение схемы: {error}"))?
        .to_rgb8();
    let min_x = model
        .nodes
        .iter()
        .map(|n| n.position.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = model
        .nodes
        .iter()
        .map(|n| n.position.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = model
        .nodes
        .iter()
        .map(|n| n.position.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = model
        .nodes
        .iter()
        .map(|n| n.position.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);
    let scale = ((REPORT_IMAGE_WIDTH as f64 - 260.0) / range_x)
        .min((REPORT_IMAGE_HEIGHT as f64 - 190.0) / range_y);
    let ox = (REPORT_IMAGE_WIDTH as f64 - range_x * scale) / 2.0;
    let oy = (REPORT_IMAGE_HEIGHT as f64 - range_y * scale) / 2.0;
    let ink = Rgb([42, 99, 145]);
    for (index, (node, data)) in model.nodes.iter().zip(&input.nodes).enumerate() {
        let p = (
            (ox + (node.position.x - min_x) * scale).round() as i32,
            (REPORT_IMAGE_HEIGHT as f64 - oy - (node.position.y - min_y) * scale).round() as i32,
        );
        if data.support == SupportKind::Free {
            continue;
        }
        if data.support == SupportKind::Fixed {
            let direction = model
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
                    let q = (
                        ox + (neighbor.position.x - min_x) * scale,
                        REPORT_IMAGE_HEIGHT as f64 - oy - (neighbor.position.y - min_y) * scale,
                    );
                    Some((p.0 as f64 - q.0, p.1 as f64 - q.1))
                })
                .fold((0.0, 0.0), |sum, d| (sum.0 + d.0, sum.1 + d.1));
            let length = direction.0.hypot(direction.1);
            let outward = if length > 1e-9 {
                (direction.0 / length, direction.1 / length)
            } else {
                (-1.0, 0.0)
            };
            let tangent = (-outward.1, outward.0);
            let at = |along: f64, away: f64| -> (i32, i32) {
                (
                    (p.0 as f64 + tangent.0 * along + outward.0 * away).round() as i32,
                    (p.1 as f64 + tangent.1 * along + outward.1 * away).round() as i32,
                )
            };
            draw_line(&mut image, at(-25.0, 11.0), at(25.0, 11.0), ink, 4);
            for y in [-24.0, -12.0, 0.0, 12.0, 24.0] {
                draw_line(&mut image, at(y, 11.0), at(y + 8.0, 23.0), ink, 2);
            }
            continue;
        }
        let a = if data.support == SupportKind::Pin {
            90.0
        } else {
            data.normal_deg
        }
        .to_radians();
        let v = (-a.cos(), a.sin());
        let u = (v.1, -v.0);
        let at = |along: f64, away: f64| -> (i32, i32) {
            (
                (p.0 as f64 + u.0 * along + v.0 * away).round() as i32,
                (p.1 as f64 + u.1 * along + v.1 * away).round() as i32,
            )
        };
        if data.support == SupportKind::Guided {
            draw_line(&mut image, at(-13.0, 9.0), at(13.0, 9.0), ink, 4);
            draw_line(&mut image, at(-13.0, 9.0), at(-13.0, 20.0), ink, 2);
            draw_line(&mut image, at(13.0, 9.0), at(13.0, 20.0), ink, 2);
        } else {
            draw_line(&mut image, at(0.0, 4.0), at(-17.0, 24.0), ink, 3);
            draw_line(&mut image, at(0.0, 4.0), at(17.0, 24.0), ink, 3);
            draw_line(&mut image, at(-17.0, 24.0), at(17.0, 24.0), ink, 3);
            if data.support == SupportKind::Roller {
                for side in [-9.0, 9.0] {
                    draw_circle(&mut image, at(side, 29.0), 4, ink, 2);
                }
            }
        }
        let distance = if data.support == SupportKind::Pin {
            27.0
        } else if data.support == SupportKind::Roller {
            35.0
        } else {
            23.0
        };
        draw_line(&mut image, at(-22.0, distance), at(22.0, distance), ink, 2);
        for s in [-18.0, -9.0, 0.0, 9.0, 18.0] {
            draw_line(
                &mut image,
                at(s, distance),
                at(s - 6.0, distance + 8.0),
                ink,
                1,
            );
        }
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|error| format!("не удалось подготовить схему МКЭ: {error}"))?;
    Ok(bytes.into_inner())
}

/// Создаёт готовый DOCX-отчёт в памяти.
#[cfg(test)]
pub fn build_word_report(
    model: &StructuralModel,
    rigidity: Rigidity,
    expansion: ThermalExpansion,
    effects: EffectsSelection,
) -> Result<Vec<u8>, String> {
    build_word_report_with_rounding(model, rigidity, expansion, effects, None)
}

pub fn build_word_report_with_rounding(
    model: &StructuralModel,
    rigidity: Rigidity,
    expansion: ThermalExpansion,
    effects: EffectsSelection,
    decimals: Option<u8>,
) -> Result<Vec<u8>, String> {
    model.validate()?;
    let results = calculate_displacement_components_with_rounding(model, rigidity, decimals)?;
    let thermal =
        calculate_thermal_displacement_components_with_rounding(model, expansion, decimals)?;
    let kinematic = calculate_kinematic_displacement_components_with_rounding(model, decimals)?;
    let scheme_png = render_scheme_png(model)?;
    let mut document_xml = build_document_xml(
        model, rigidity, expansion, effects, &results, &thermal, &kinematic,
    )?;
    if let Some(decimals) = decimals {
        let note = paragraph(&format!("Учебный режим: расчётные ординаты и вклады округлены до {decimals} знаков после запятой. Исходные свойства и коэффициенты сохранены с полной точностью."), None, false);
        document_xml = document_xml.replacen("<w:body>", &format!("<w:body>{note}"), 1);
    }

    let mut archive = StoredZip::default();
    archive.add("[Content_Types].xml", content_types_xml().as_bytes());
    archive.add("_rels/.rels", package_relationships_xml().as_bytes());
    archive.add("docProps/core.xml", core_properties_xml().as_bytes());
    archive.add("docProps/app.xml", app_properties_xml().as_bytes());
    archive.add("word/document.xml", document_xml.as_bytes());
    archive.add(
        "word/_rels/document.xml.rels",
        document_relationships_xml().as_bytes(),
    );
    archive.add("word/styles.xml", styles_xml().as_bytes());
    archive.add("word/media/frame.png", &scheme_png);
    archive.finish()
}

fn build_document_xml(
    model: &StructuralModel,
    rigidity: Rigidity,
    expansion: ThermalExpansion,
    effects: EffectsSelection,
    force: &DisplacementComponents,
    thermal: &ThermalDisplacementComponents,
    kinematic: &KinematicDisplacementComponents,
) -> Result<String, String> {
    let has_unit = |select: fn(&crate::Rod) -> &Diagram| {
        model
            .rods
            .iter()
            .any(|rod| *select(rod) != Diagram::default())
    };
    let has_load = model
        .rods
        .iter()
        .any(|rod| rod.load_diagram != Diagram::default());
    let has_temperature = model.rods.iter().any(|rod| rod.temperature.is_some());
    let has_support_motion = model
        .supports
        .iter()
        .any(|support| support.displacement.abs() > 1e-12);
    let selected = [
        EffectsSelection {
            force: effects.force && has_load && has_unit(|r| &r.unit_x_diagram),
            thermal: effects.thermal
                && has_temperature
                && (has_unit(|r| &r.unit_x_diagram) || has_unit(|r| &r.unit_normal_x_diagram)),
            kinematic: effects.kinematic
                && has_support_motion
                && model
                    .supports
                    .iter()
                    .any(|s| s.unit_x_reaction.abs() > 1e-12),
        },
        EffectsSelection {
            force: effects.force && has_load && has_unit(|r| &r.unit_y_diagram),
            thermal: effects.thermal
                && has_temperature
                && (has_unit(|r| &r.unit_y_diagram) || has_unit(|r| &r.unit_normal_y_diagram)),
            kinematic: effects.kinematic
                && has_support_motion
                && model
                    .supports
                    .iter()
                    .any(|s| s.unit_y_reaction.abs() > 1e-12),
        },
        EffectsSelection {
            force: effects.force && has_load && has_unit(|r| &r.unit_rotation_diagram),
            thermal: effects.thermal
                && has_temperature
                && (has_unit(|r| &r.unit_rotation_diagram)
                    || has_unit(|r| &r.unit_normal_rotation_diagram)),
            kinematic: effects.kinematic
                && has_support_motion
                && model
                    .supports
                    .iter()
                    .any(|s| s.unit_rotation_reaction.abs() > 1e-12),
        },
    ];
    let available = |i: usize| selected[i].force || selected[i].thermal || selected[i].kinematic;
    let force_present = selected.iter().any(|s| s.force);
    let thermal_present = selected.iter().any(|s| s.thermal);
    let kinematic_present = selected.iter().any(|s| s.kinematic);
    let horizontal = &force.horizontal;
    let vertical = &force.vertical;
    let rotation = &force.rotation;
    let target = &model.nodes[model.target_node].id;
    let mut body = String::new();

    body.push_str(&paragraph(
        if available(2) {
            "Расчёт перемещений и поворота точки плоской стержневой рамы"
        } else {
            "Расчёт перемещений точки плоской стержневой рамы"
        },
        Some("Title"),
        true,
    ));
    let included_effects = [
        force_present.then_some("силового"),
        thermal_present.then_some("температурного"),
        kinematic_present.then_some("кинематического"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let report_scope = if included_effects.is_empty() {
        "только исходные данные".to_string()
    } else {
        format!("расчёт {} воздействия", included_effects.join(" и "))
    };
    let mut totals = Vec::new();
    for (index, symbol, force, temp, kin) in [
        (
            0,
            "Δx",
            horizontal,
            &thermal.horizontal,
            kinematic.horizontal.total,
        ),
        (
            1,
            "Δy",
            vertical,
            &thermal.vertical,
            kinematic.vertical.total,
        ),
        (
            2,
            "φ",
            rotation,
            &thermal.rotation,
            kinematic.rotation.total,
        ),
    ] {
        if available(index) {
            totals.push(format!(
                "{symbol} = {}",
                combined_value(force, temp, kin, selected[index])
            ));
        }
    }
    body.push_str(&paragraph(
        &format!(
            "Отчёт содержит {report_scope} для точки {target}. Итоговые значения: {}.",
            if totals.is_empty() {
                "нет заданных расчётов".into()
            } else {
                totals.join("; ")
            }
        ),
        None,
        false,
    ));

    body.push_str(&heading("Исходные данные", 1));
    if force_present {
        let description = if model
            .rods
            .iter()
            .any(|rod| rod.rigidity_preset_id.is_some())
        {
            "Жёсткости заданы независимо в таблице EI. Без отметки «Задано численно» строка содержит коэффициент перед EI; с отметкой — полное числовое значение.".to_string()
        } else {
            format!(
                "Базовая изгибная жёсткость: {}. Жёсткость участка равна k·EI.",
                rigidity_text(rigidity)
            )
        };
        body.push_str(&paragraph(&description, None, false));
    }
    if thermal_present {
        let description = if model.rods.iter().any(|rod| rod.thermal_preset_id.is_some()) {
            "Коэффициенты температурного расширения заданы независимо в таблице α. Без отметки «Задано численно» строка содержит коэффициент перед α; с отметкой — полное числовое значение. Для каждого стержня: αᵢ·[sₜ·t′/h·∫m ds + t₀·∫n ds], где t′ = |t₁−t₂|.".to_string()
        } else {
            format!("Базовый коэффициент температурного расширения: {}. Для стержня αᵢ = kα·α; t′ = |t₁−t₂|.", expansion_text(expansion))
        };
        body.push_str(&paragraph(&description, None, false));
    }

    let node_rows = model
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            vec![
                (index + 1).to_string(),
                node.id.clone(),
                format_number(node.position.x),
                format_number(node.position.y),
                if index == model.target_node {
                    "искомая точка".into()
                } else {
                    String::new()
                },
            ]
        })
        .collect::<Vec<_>>();
    body.push_str(&caption("Таблица 1  Координаты точек"));
    body.push_str(&table(&["№", "Точка", "x", "y", "Назначение"], &node_rows));

    let mut rod_rows = Vec::with_capacity(model.rods.len());
    let mut diagram_rows = Vec::with_capacity(model.rods.len());
    for rod in &model.rods {
        let start = &model.nodes[rod.start_node].id;
        let end = &model.nodes[rod.end_node].id;
        let length = model.rod_length(rod)?;
        let temperature = rod.temperature.unwrap_or_default();
        let mut row = vec![
            rod.id.clone(),
            format!("{start}–{end}"),
            format_number(length),
        ];
        if force_present {
            row.push(model.rigidity_value(rod, rigidity)?.formatted("EI"));
        }
        if thermal_present {
            row.extend([
                format_number(rod.section_height),
                model.thermal_value(rod, expansion)?.formatted("α"),
                if rod.temperature.is_some() {
                    format!(
                        "{}; {}",
                        format_number(temperature.upper_or_left),
                        format_number(temperature.lower_or_right)
                    )
                } else {
                    "нет".into()
                },
            ]);
        }
        rod_rows.push(row);
        diagram_rows.push(vec![
            rod.id.clone(),
            diagram_values(&rod.load_diagram, length),
            diagram_values(&rod.normal_diagram, length),
            diagram_values(&rod.shear_diagram, length),
        ]);
    }
    body.push_str(&caption("Таблица 2  Геометрия и параметры стержней"));
    let mut rod_headers = vec!["Стержень", "Узлы", "L"];
    if force_present {
        rod_headers.push("EI стержня");
    }
    if thermal_present {
        rod_headers.extend(["h", "α стержня", "t₁; t₂"]);
    }
    body.push_str(&table(&rod_headers, &rod_rows));
    body.push_str(&caption("Таблица 3  Заданные эпюры M, N и Q"));
    body.push_str(&table(
        &["Стержень", "M  L; L/2; R", "N  L; L/2; R", "Q  L; L/2; R"],
        &diagram_rows,
    ));

    body.push_str(page_break());
    body.push_str(&heading("Расчётная схема", 1));
    body.push_str(&keep_next_paragraph(
        "Размерные линии выполнены засечками. Жёлтым кольцом отмечена точка, перемещение которой определяется.",
    ));
    body.push_str(image_paragraph());
    body.push_str(&caption("Рисунок 1  Геометрия рамы и размеры стержней"));

    if force_present {
        body.push_str(&heading(
            if selected[2].force {
                "Расчёт силовых перемещений и поворота"
            } else {
                "Расчёт силовых перемещений"
            },
            1,
        ));
        body.push_str(&paragraph(
            if effects.force {
                "Силовое воздействие включено в итог."
            } else {
                "Силовое воздействие рассчитано, но не включено в итог."
            },
            None,
            false,
        ));
        body.push_str(&math_paragraph("Δ = Σ ∫ Mₚ m₁ / (EIᵢ) dx"));
        body.push_str(empty_paragraph());
        body.push_str(&paragraph(
        "На линейных и постоянных участках применяется аналитическое перемножение трапеций. Если хотя бы одна эпюра параболическая, используется трёхточечная формула Симпсона. Вклад каждого стержня делится на его EI.",
        None,
        false,
    ));

        if selected[0].force {
            append_component(
                &mut body,
                model,
                "Горизонтальное перемещение",
                "Δx",
                horizontal,
                4,
            );
        }
        if selected[1].force {
            append_component(
                &mut body,
                model,
                "Вертикальное перемещение",
                "Δy",
                vertical,
                5,
            );
        }
        if selected[2].force {
            append_component(&mut body, model, "Поворот точки", "φ", rotation, 6);
        }
    }

    if thermal_present {
        body.push_str(page_break());
        body.push_str(&heading("Расчёт температурных перемещений", 1));
        body.push_str(&math_paragraph(
            "ΔT = Σ αᵢ·[sₜ·t′·∫m ds/h + t₀·∫n ds],  t′=|t₁−t₂|",
        ));
        body.push_str(&paragraph(
        if effects.thermal {
            "Температурное воздействие включено в итог. t′ всегда берётся по модулю; знак изгиба определяется стороной наиболее нагретых волокон."
        } else {
            "Температурное воздействие рассчитано, но не включено в итог. t′ всегда берётся по модулю; знак изгиба определяется стороной наиболее нагретых волокон."
        },
        None,
        false,
    ));
        if selected[0].thermal {
            append_thermal_component(
                &mut body,
                "Горизонтальное температурное перемещение",
                "Δx(T)",
                &thermal.horizontal,
                7,
            );
        }
        if selected[1].thermal {
            append_thermal_component(
                &mut body,
                "Вертикальное температурное перемещение",
                "Δy(T)",
                &thermal.vertical,
                8,
            );
        }
        if selected[2].thermal {
            append_thermal_component(
                &mut body,
                "Температурный поворот точки",
                "φ(T)",
                &thermal.rotation,
                9,
            );
        }
    }

    if kinematic_present {
        body.push_str(page_break());
        body.push_str(&heading("Расчёт кинематических перемещений", 1));
        body.push_str(&math_paragraph("ΔK = −Σ Rᵢ⁽¹⁾·cᵢ"));
        body.push_str(&paragraph("Положительные X и Y направлены вправо и вверх; положительный поворот — против часовой стрелки. Реакции заданы для соответствующих единичных систем.", None, false));
        if selected[0].kinematic {
            append_kinematic_component(
                &mut body,
                "Горизонтальное перемещение",
                "Δx(K)",
                &kinematic.horizontal,
                10,
            );
        }
        if selected[1].kinematic {
            append_kinematic_component(
                &mut body,
                "Вертикальное перемещение",
                "Δy(K)",
                &kinematic.vertical,
                11,
            );
        }
        if selected[2].kinematic {
            append_kinematic_component(&mut body, "Поворот точки", "φ(K)", &kinematic.rotation, 12);
        }
    }

    if available(0) || available(1) || available(2) {
        body.push_str(&heading("Итог", 1));
        let headings = ["Δx", "Δy", "φ"];
        let forces = [horizontal, vertical, rotation];
        let temperatures = [&thermal.horizontal, &thermal.vertical, &thermal.rotation];
        let kinematics = [
            kinematic.horizontal.total,
            kinematic.vertical.total,
            kinematic.rotation.total,
        ];
        let mut headers = vec!["Воздействие"];
        for (i, heading) in headings.iter().enumerate() {
            if available(i) {
                headers.push(*heading);
            }
        }
        let mut rows = Vec::new();
        for (name, active) in [
            ("Силовое", force_present),
            ("Температурное", thermal_present),
            ("Кинематическое", kinematic_present),
        ] {
            if !active {
                continue;
            }
            let mut row = vec![name.to_string()];
            for i in 0..3 {
                if !available(i) {
                    continue;
                }
                let value = match name {
                    "Силовое" if selected[i].force => displacement_views(forces[i]),
                    "Температурное" if selected[i].thermal => {
                        temperatures[i].formatted_displacement()
                    }
                    "Кинематическое" if selected[i].kinematic => {
                        format_number(kinematics[i])
                    }
                    _ => "—".into(),
                };
                row.push(value);
            }
            rows.push(row);
        }
        let mut total_row = vec!["ИТОГО".to_string()];
        for i in 0..3 {
            if available(i) {
                total_row.push(combined_value(
                    forces[i],
                    temperatures[i],
                    kinematics[i],
                    selected[i],
                ));
            }
        }
        rows.push(total_row);
        body.push_str(&table(&headers, &rows));
        body.push_str(&final_result_paragraph(&format!(
            "Точка {target}: {}",
            totals.join("; ")
        )));
        body.push_str(&paragraph(if available(2) { "Знаки Δx и Δy соответствуют направлениям глобальных осей X и Y. Положительный поворот φ направлен против часовой стрелки." } else { "Знаки Δx и Δy соответствуют направлениям глобальных осей X и Y." }, None, false));
    }

    body.push_str(
        r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1008" w:right="1008" w:bottom="1008" w:left="1008" w:header="500" w:footer="500" w:gutter="0"/></w:sectPr>"#,
    );

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>{body}</w:body></w:document>"#
    ))
}

fn append_component(
    body: &mut String,
    model: &StructuralModel,
    title: &str,
    symbol: &str,
    result: &CalculationResult,
    table_number: usize,
) {
    body.push_str(&heading(title, 2));
    let rows = result
        .contributions
        .iter()
        .map(|item| {
            vec![
                item.segment_id.clone(),
                item.method.to_string(),
                triple(item.load_values),
                triple(item.unit_values),
                format_number(item.raw_integral),
                item.property.map_or_else(
                    || format!("{} · EI", format_number(item.rigidity_factor)),
                    |value| value.formatted("EI"),
                ),
                force_contribution_text(item),
            ]
        })
        .collect::<Vec<_>>();
    body.push_str(&caption(&format!(
        "Таблица {table_number}  Вклады стержней в {symbol}"
    )));
    body.push_str(&table(
        &[
            "Стержень",
            "Метод",
            "Mₚ  L; L/2; R",
            "m₁  L; L/2; R",
            "Интеграл",
            "EI стержня",
            "Вклад",
        ],
        &rows,
    ));
    body.push_str(&keep_next_paragraph(
        "Подробная подстановка по каждому стержню, включая нулевые слагаемые:",
    ));
    for (index, item) in result.contributions.iter().enumerate() {
        let length = model
            .rods
            .get(index)
            .and_then(|rod| model.rod_length(rod).ok())
            .unwrap_or(0.0);
        body.push_str(&calculation_paragraph(&format!(
            "Стержень {}. {}",
            item.segment_id,
            integration_formula(item.method)
        )));
        body.push_str(&calculation_paragraph(&format!(
            "Подстановка: I = {} = {}; I/EIᵢ = {} / {} = {}",
            substituted_integral(length, item),
            format_number(item.raw_integral),
            format_number(item.raw_integral),
            item.property.map_or_else(
                || format!("{} · EI", format_number(item.rigidity_factor)),
                |value| value.formatted("EI")
            ),
            force_contribution_text(item)
        )));
    }
    let sum = result
        .contributions
        .iter()
        .map(force_contribution_text)
        .collect::<Vec<_>>()
        .join(" + ");
    let sum = if sum.is_empty() { "0" } else { &sum };
    body.push_str(&calculation_paragraph(&format!(
        "Сумма без сокращений: Σ(∫Mₚ·m₁ dx / EIᵢ) = {sum} = {}",
        result.formatted_displacement()
    )));
    body.push_str(empty_paragraph());
    body.push_str(&result_paragraph(&format!(
        "{symbol} = {}",
        displacement_views(result)
    )));
}

fn append_thermal_component(
    body: &mut String,
    title: &str,
    symbol: &str,
    result: &ThermalCalculationResult,
    table_number: usize,
) {
    body.push_str(&heading(title, 2));
    let rows = result
        .contributions
        .iter()
        .map(|item| {
            vec![
                item.rod_id.clone(),
                if item.active {
                    "да".into()
                } else {
                    "нет".into()
                },
                format_number(item.section_height),
                item.property.map_or_else(
                    || format!("{} · α", format_number(item.expansion_factor)),
                    |value| value.formatted("α"),
                ),
                format!(
                    "{}; {}",
                    format_number(item.upper_or_left),
                    format_number(item.lower_or_right)
                ),
                format!(
                    "{}; {}",
                    format_number(item.temperature_difference),
                    format_number(item.mean_temperature)
                ),
                format_number(item.bending_term),
                format_number(item.axial_term),
                thermal_contribution_text(item),
            ]
        })
        .collect::<Vec<_>>();
    body.push_str(&caption(&format!(
        "Таблица {table_number}  Температурные вклады стержней в {symbol}"
    )));
    body.push_str(&table(
        &[
            "Стержень",
            "T",
            "h",
            "α стержня",
            "t₁; t₂",
            "t′; t₀",
            "Изгиб",
            "Ось",
            "Сумма",
        ],
        &rows,
    ));
    body.push_str(&keep_next_paragraph(
        "Подробная подстановка по каждому стержню, включая нулевые слагаемые:",
    ));
    for item in &result.contributions {
        body.push_str(&calculation_paragraph(&format!(
            "Стержень {}: t′=|{}−{}|={}; наиболее нагретая сторона={}; sₜ={}; t₀=({}+{})/2={}",
            item.rod_id,
            format_number(item.upper_or_left),
            format_number(item.lower_or_right),
            format_number(item.temperature_difference),
            hotter_side_text(item.hotter_side),
            format_number(item.hotter_side_sign),
            format_number(item.upper_or_left),
            format_number(item.lower_or_right),
            format_number(item.mean_temperature),
        )));
        body.push_str(&calculation_paragraph(&format!(
            "{}·[{}·{}/{}·{} + {}·{}] = {}·[{} + {}] = {}{}",
            item.property.map_or_else(
                || format!("{} · α", format_number(item.expansion_factor)),
                |value| value.formatted("α")
            ),
            format_number(item.hotter_side_sign),
            format_number(item.temperature_difference),
            format_number(item.section_height),
            format_number(item.unit_moment_area),
            format_number(item.mean_temperature),
            format_number(item.unit_normal_area),
            item.property.map_or_else(
                || format!("{} · α", format_number(item.expansion_factor)),
                |value| value.formatted("α")
            ),
            format_number(item.bending_term),
            format_number(item.axial_term),
            thermal_contribution_text(item),
            if item.active {
                ""
            } else {
                " (температурное воздействие выключено)"
            },
        )));
    }
    let sum = result
        .contributions
        .iter()
        .map(thermal_contribution_text)
        .collect::<Vec<_>>()
        .join(" + ");
    body.push_str(&calculation_paragraph(&format!(
        "Сумма: {sum} = {}",
        result.formatted_displacement()
    )));
    body.push_str(&result_paragraph(&format!(
        "{symbol} = {}",
        result.formatted_displacement()
    )));
}

fn append_kinematic_component(
    body: &mut String,
    title: &str,
    symbol: &str,
    result: &KinematicResult,
    table_number: usize,
) {
    body.push_str(&heading(title, 2));
    let rows = result
        .contributions
        .iter()
        .map(|item| {
            vec![
                item.node_id.clone(),
                if item.direction == SupportDirection::Normal {
                    format!("N ({}°)", format_number(item.normal_deg))
                } else {
                    item.direction.symbol().into()
                },
                format_number(item.displacement),
                format_number(item.unit_reaction),
                format_number(item.value),
            ]
        })
        .collect::<Vec<_>>();
    body.push_str(&caption(&format!(
        "Таблица {table_number}  Опорные связи для {symbol}"
    )));
    body.push_str(&table(
        &["Точка", "Направление", "cᵢ", "Rᵢ⁽¹⁾", "−Rᵢ⁽¹⁾·cᵢ"],
        &rows,
    ));
    body.push_str(&keep_next_paragraph(
        "Подстановка по всем опорным связям, включая нулевые слагаемые:",
    ));
    for item in &result.contributions {
        body.push_str(&calculation_paragraph(&format!(
            "{} {}: −({})·({}) = {}",
            item.node_id,
            item.direction.symbol(),
            format_number(item.unit_reaction),
            format_number(item.displacement),
            format_number(item.value)
        )));
    }
    let sum = result
        .contributions
        .iter()
        .map(|item| format_number(item.value))
        .collect::<Vec<_>>()
        .join(" + ");
    let sum = if sum.is_empty() { "0" } else { &sum };
    body.push_str(&calculation_paragraph(&format!(
        "{symbol} = {sum} = {}",
        format_number(result.total)
    )));
    body.push_str(&result_paragraph(&format!(
        "{symbol} = {}",
        format_number(result.total)
    )));
}

fn combined_value(
    force: &CalculationResult,
    thermal: &ThermalCalculationResult,
    kinematic: f64,
    effects: EffectsSelection,
) -> String {
    let force_numeric = effects
        .force
        .then(|| force.numeric_displacement())
        .flatten();
    let thermal_numeric = effects
        .thermal
        .then(|| thermal.numeric_displacement())
        .flatten();
    let all_selected_numeric = (!effects.force || force_numeric.is_some())
        && (!effects.thermal || thermal_numeric.is_some());
    if all_selected_numeric {
        return format_number(
            force_numeric.unwrap_or(0.0)
                + thermal_numeric.unwrap_or(0.0)
                + if effects.kinematic { kinematic } else { 0.0 },
        );
    }
    let mut terms = Vec::new();
    if effects.force && force_numeric.is_none() && force.numerator.abs() > 1.0e-12 {
        terms.push(crate::format_symbolic_force(force.numerator));
    }
    if effects.thermal && thermal_numeric.is_none() && thermal.coefficient.abs() > 1.0e-12 {
        terms.push(format!("α · {}", format_number(thermal.coefficient)));
    }
    let numeric = if effects.force {
        force_numeric.unwrap_or(force.numeric_part)
    } else {
        0.0
    } + if effects.thermal {
        thermal_numeric.unwrap_or(thermal.numeric_part)
    } else {
        0.0
    } + if effects.kinematic { kinematic } else { 0.0 };
    if numeric.abs() > 1.0e-12 {
        terms.push(format_number(numeric));
    }
    if terms.is_empty() {
        "0".into()
    } else {
        terms.join(" + ").replace(" + -", " - ")
    }
}

fn integration_formula(method: IntegrationMethod) -> &'static str {
    match method {
        IntegrationMethod::TrapezoidalProduct => "I = L/6 · [Mₚ,L·(2·mL + mR) + Mₚ,R·(mL + 2·mR)]",
        IntegrationMethod::SimpsonThreePoint => "I = L/6 · [Mₚ,L·mL + 4·Mₚ,m·mm + Mₚ,R·mR]",
    }
}

fn substituted_integral(length: f64, item: &crate::SegmentContribution) -> String {
    let load = item.load_values.map(format_number);
    let unit = item.unit_values.map(format_number);
    match item.method {
        IntegrationMethod::TrapezoidalProduct => format!(
            "{}/6 · [{}·(2·{} + {}) + {}·({} + 2·{})]",
            format_number(length),
            load[0],
            unit[0],
            unit[2],
            load[2],
            unit[0],
            unit[2]
        ),
        IntegrationMethod::SimpsonThreePoint => format!(
            "{}/6 · [{}·{} + 4·{}·{} + {}·{}]",
            format_number(length),
            load[0],
            unit[0],
            load[1],
            unit[1],
            load[2],
            unit[2]
        ),
    }
}

fn force_contribution_text(item: &crate::SegmentContribution) -> String {
    match item.property {
        Some(value) if value.numeric => format_number(item.integral),
        _ => format!("{} / EI", format_number(item.integral)),
    }
}

fn thermal_contribution_text(item: &crate::ThermalRodContribution) -> String {
    match item.property {
        Some(value) if value.numeric => format_number(item.coefficient),
        _ => format!("α · {}", format_number(item.coefficient)),
    }
}

fn displacement_views(result: &CalculationResult) -> String {
    let fraction = result.formatted_displacement();
    let decimal = result.formatted_decimal_displacement();
    if fraction == decimal {
        fraction
    } else {
        format!("{fraction} ≈ {decimal}")
    }
}

fn rigidity_text(rigidity: Rigidity) -> String {
    match rigidity {
        Rigidity::Symbolic => "аналитическая константа EI".into(),
        Rigidity::Numeric(value) => format!("EI = {}", format_number(value)),
    }
}

fn expansion_text(expansion: ThermalExpansion) -> String {
    match expansion {
        ThermalExpansion::Symbolic => "аналитическая константа α".into(),
        ThermalExpansion::Numeric(value) => {
            format!("α = {} 1/°C", format_number(value))
        }
    }
}

fn hotter_side_text(side: Option<FiberSide>) -> &'static str {
    match side {
        Some(FiberSide::UpperOrLeft) => "верх/лево",
        Some(FiberSide::LowerOrRight) => "низ/право",
        Some(FiberSide::OnAxis) | None => "нет (t₁=t₂)",
    }
}

fn diagram_values(diagram: &Diagram, length: f64) -> String {
    format!(
        "{}; {}; {}",
        format_number(diagram.left_value()),
        format_number(diagram.midpoint_value(length)),
        format_number(diagram.right_value())
    )
}

fn triple(values: [f64; 3]) -> String {
    format!(
        "{}; {}; {}",
        format_number(values[0]),
        format_number(values[1]),
        format_number(values[2])
    )
}

fn paragraph(text: &str, style: Option<&str>, centered: bool) -> String {
    let style_xml = style.map_or(String::new(), |value| {
        format!(r#"<w:pStyle w:val="{}"/>"#, xml_escape(value))
    });
    let align = if centered {
        r#"<w:jc w:val="center"/>"#
    } else {
        ""
    };
    format!(
        r#"<w:p><w:pPr>{style_xml}{align}<w:spacing w:after="120"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn keep_next_paragraph(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:keepNext/><w:spacing w:after="120"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn empty_paragraph() -> &'static str {
    r#"<w:p><w:pPr><w:spacing w:after="80"/></w:pPr></w:p>"#
}

fn page_break() -> &'static str {
    r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#
}

fn heading(text: &str, level: u8) -> String {
    paragraph(
        text,
        Some(if level == 1 { "Heading1" } else { "Heading2" }),
        false,
    )
}

fn caption(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:before="120" w:after="80"/><w:keepNext/></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t>{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn result_paragraph(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:before="160" w:after="120"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="24"/></w:rPr><w:t>{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn final_result_paragraph(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="160" w:after="120"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="24"/></w:rPr><w:t>{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn calculation_paragraph(text: &str) -> String {
    format!(
        r#"<m:oMathPara><m:oMathParaPr><m:jc m:val="left"/></m:oMathParaPr><m:oMath><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>{}</m:t></m:r></m:oMath></m:oMathPara>"#,
        xml_escape(text)
    )
}

fn math_paragraph(text: &str) -> String {
    format!(
        r#"<m:oMathPara><m:oMathParaPr><m:jc m:val="centerGroup"/></m:oMathParaPr><m:oMath><m:r><m:rPr><m:sty m:val="p"/></m:rPr><m:t>{}</m:t></m:r></m:oMath></m:oMathPara>"#,
        xml_escape(text)
    )
}

fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut xml = String::from(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblLayout w:type="autofit"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="D9D9D9"/><w:left w:val="single" w:sz="4" w:color="D9D9D9"/><w:bottom w:val="single" w:sz="4" w:color="D9D9D9"/><w:right w:val="single" w:sz="4" w:color="D9D9D9"/><w:insideH w:val="single" w:sz="4" w:color="D9D9D9"/><w:insideV w:val="single" w:sz="4" w:color="D9D9D9"/></w:tblBorders><w:tblCellMar><w:top w:w="90" w:type="dxa"/><w:left w:w="90" w:type="dxa"/><w:bottom w:w="90" w:type="dxa"/><w:right w:w="90" w:type="dxa"/></w:tblCellMar></w:tblPr>"#,
    );
    xml.push_str("<w:tr><w:trPr><w:tblHeader/></w:trPr>");
    for header in headers {
        xml.push_str(&table_cell(header, true, false));
    }
    xml.push_str("</w:tr>");
    for (row_index, row) in rows.iter().enumerate() {
        xml.push_str("<w:tr>");
        for value in row {
            xml.push_str(&table_cell(value, false, row_index % 2 == 1));
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    xml
}

fn table_cell(text: &str, header: bool, shaded: bool) -> String {
    let fill = if header {
        r#"<w:shd w:val="clear" w:color="auto" w:fill="1F4E78"/>"#
    } else if shaded {
        r#"<w:shd w:val="clear" w:color="auto" w:fill="EAF2F8"/>"#
    } else {
        ""
    };
    let run_properties = if header {
        r#"<w:rPr><w:b/><w:color w:val="FFFFFF"/><w:sz w:val="18"/></w:rPr>"#
    } else {
        r#"<w:rPr><w:sz w:val="18"/></w:rPr>"#
    };
    format!(
        r#"<w:tc><w:tcPr><w:vAlign w:val="center"/>{fill}</w:tcPr><w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr><w:r>{run_properties}<w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
        xml_escape(text)
    )
}

fn image_paragraph() -> &'static str {
    r#"<w:p><w:pPr><w:jc w:val="center"/><w:keepNext/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="5943600" cy="3863340"/><wp:docPr id="1" name="Расчётная схема" descr="Геометрия рамы с размерами и выбранной точкой"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="frame.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="5943600" cy="3863340"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn content_types_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#
}

fn package_relationships_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#
}

fn document_relationships_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/frame.png"/></Relationships>"#
}

fn core_properties_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Расчёт перемещений плоской стержневой рамы</dc:title><dc:creator>Mohr Frame</dc:creator><dc:subject>Строительная механика</dc:subject></cp:coreProperties>"#
}

fn app_properties_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Mohr Frame</Application><AppVersion>0.8.0</AppVersion></Properties>"#
}

fn styles_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="24"/><w:szCs w:val="24"/><w:color w:val="000000"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/><w:jc w:val="both"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:jc w:val="center"/><w:spacing w:before="0" w:after="280"/></w:pPr><w:rPr><w:b/><w:color w:val="000000"/><w:sz w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="280" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:color w:val="000000"/><w:sz w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="220" w:after="100"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:color w:val="000000"/><w:sz w:val="25"/></w:rPr></w:style></w:styles>"#
}

fn render_scheme_png(model: &StructuralModel) -> Result<Vec<u8>, String> {
    let mut image = RgbImage::from_pixel(
        REPORT_IMAGE_WIDTH,
        REPORT_IMAGE_HEIGHT,
        Rgb([255, 255, 255]),
    );
    let min_x = model
        .nodes
        .iter()
        .map(|node| node.position.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = model
        .nodes
        .iter()
        .map(|node| node.position.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = model
        .nodes
        .iter()
        .map(|node| node.position.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = model
        .nodes
        .iter()
        .map(|node| node.position.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);
    let scale = ((REPORT_IMAGE_WIDTH as f64 - 260.0) / range_x)
        .min((REPORT_IMAGE_HEIGHT as f64 - 190.0) / range_y);
    let content_width = range_x * scale;
    let content_height = range_y * scale;
    let offset_x = (REPORT_IMAGE_WIDTH as f64 - content_width) / 2.0;
    let offset_y = (REPORT_IMAGE_HEIGHT as f64 - content_height) / 2.0;
    let map = |x: f64, y: f64| -> (i32, i32) {
        (
            (offset_x + (x - min_x) * scale).round() as i32,
            (REPORT_IMAGE_HEIGHT as f64 - offset_y - (y - min_y) * scale).round() as i32,
        )
    };
    let center = (
        REPORT_IMAGE_WIDTH as f64 / 2.0,
        REPORT_IMAGE_HEIGHT as f64 / 2.0,
    );
    let ink = Rgb([30, 36, 44]);
    let dimension = Rgb([95, 105, 115]);
    let accent = Rgb([230, 169, 25]);

    for rod in &model.rods {
        let start_node = &model.nodes[rod.start_node];
        let end_node = &model.nodes[rod.end_node];
        let a = map(start_node.position.x, start_node.position.y);
        let b = map(end_node.position.x, end_node.position.y);
        draw_line(&mut image, a, b, ink, 4);

        let dx = (b.0 - a.0) as f64;
        let dy = (b.1 - a.1) as f64;
        let pixel_length = dx.hypot(dy).max(1.0);
        let ux = dx / pixel_length;
        let uy = dy / pixel_length;
        let mut px = -uy;
        let mut py = ux;
        let midpoint = ((a.0 + b.0) as f64 / 2.0, (a.1 + b.1) as f64 / 2.0);
        if (midpoint.0 - center.0) * px + (midpoint.1 - center.1) * py < 0.0 {
            px = -px;
            py = -py;
        }
        let offset = 42.0;
        let da = (
            (a.0 as f64 + px * offset).round() as i32,
            (a.1 as f64 + py * offset).round() as i32,
        );
        let db = (
            (b.0 as f64 + px * offset).round() as i32,
            (b.1 as f64 + py * offset).round() as i32,
        );
        draw_line(
            &mut image,
            a,
            (
                (a.0 as f64 + px * (offset + 10.0)).round() as i32,
                (a.1 as f64 + py * (offset + 10.0)).round() as i32,
            ),
            dimension,
            1,
        );
        draw_line(
            &mut image,
            b,
            (
                (b.0 as f64 + px * (offset + 10.0)).round() as i32,
                (b.1 as f64 + py * (offset + 10.0)).round() as i32,
            ),
            dimension,
            1,
        );
        draw_line(&mut image, da, db, dimension, 1);
        for point in [da, db] {
            let tick = (ux + px, uy + py);
            draw_line(
                &mut image,
                (
                    (point.0 as f64 - tick.0 * 8.0).round() as i32,
                    (point.1 as f64 - tick.1 * 8.0).round() as i32,
                ),
                (
                    (point.0 as f64 + tick.0 * 8.0).round() as i32,
                    (point.1 as f64 + tick.1 * 8.0).round() as i32,
                ),
                dimension,
                2,
            );
        }
        let length_text = format_number(model.rod_length(rod)?);
        draw_text_centered(
            &mut image,
            &length_text,
            (
                (da.0 + db.0) / 2 + (px * 13.0) as i32,
                (da.1 + db.1) / 2 + (py * 13.0) as i32,
            ),
            3,
            dimension,
        );
        draw_text_centered(
            &mut image,
            &rod.id,
            (
                (a.0 + b.0) / 2 - (px * 18.0) as i32,
                (a.1 + b.1) / 2 - (py * 18.0) as i32,
            ),
            3,
            ink,
        );
    }

    for (index, node) in model.nodes.iter().enumerate() {
        let point = map(node.position.x, node.position.y);
        if index == model.target_node {
            draw_circle(&mut image, point, 13, accent, 3);
        }
        let radius = if model.is_internal_hinge_node(index) {
            9
        } else {
            6
        };
        draw_filled_circle(&mut image, point, radius, Rgb([255, 255, 255]));
        draw_circle(&mut image, point, radius, ink, 2);
        draw_text(&mut image, &node.id, (point.0 + 10, point.1 + 8), 3, ink);
    }

    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|error| format!("не удалось подготовить изображение схемы: {error}"))?;
    Ok(bytes.into_inner())
}

fn set_pixel(image: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>) {
    if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
        image.put_pixel(x as u32, y as u32, color);
    }
}

fn draw_line(image: &mut RgbImage, start: (i32, i32), end: (i32, i32), color: Rgb<u8>, width: i32) {
    let (mut x0, mut y0) = start;
    let (x1, y1) = end;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        let radius = width.saturating_sub(1) / 2;
        for ox in -radius..=radius {
            for oy in -radius..=radius {
                set_pixel(image, x0 + ox, y0 + oy, color);
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x0 += sx;
        }
        if twice <= dx {
            error += dx;
            y0 += sy;
        }
    }
}

fn draw_filled_circle(image: &mut RgbImage, center: (i32, i32), radius: i32, color: Rgb<u8>) {
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x * x + y * y <= radius * radius {
                set_pixel(image, center.0 + x, center.1 + y, color);
            }
        }
    }
}

fn draw_circle(image: &mut RgbImage, center: (i32, i32), radius: i32, color: Rgb<u8>, width: i32) {
    let outer = radius * radius;
    let inner_radius = (radius - width).max(0);
    let inner = inner_radius * inner_radius;
    for y in -radius..=radius {
        for x in -radius..=radius {
            let distance = x * x + y * y;
            if distance <= outer && distance >= inner {
                set_pixel(image, center.0 + x, center.1 + y, color);
            }
        }
    }
}

fn draw_text_centered(
    image: &mut RgbImage,
    text: &str,
    center: (i32, i32),
    scale: i32,
    color: Rgb<u8>,
) {
    let width = text_width(text, scale);
    draw_text(
        image,
        text,
        (center.0 - width / 2, center.1 - 4 * scale),
        scale,
        color,
    );
}

fn draw_text(image: &mut RgbImage, text: &str, origin: (i32, i32), scale: i32, color: Rgb<u8>) {
    let mut cursor = origin.0;
    for character in text.chars() {
        let glyph = glyph(character.to_ascii_uppercase());
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            set_pixel(
                                image,
                                cursor + column * scale + sx,
                                origin.1 + row as i32 * scale + sy,
                                color,
                            );
                        }
                    }
                }
            }
        }
        cursor += 6 * scale;
    }
}

fn text_width(text: &str, scale: i32) -> i32 {
    (text.chars().count() as i32 * 6 - 1).max(0) * scale
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 0, 12, 8],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        ' ' => [0; 7],
        _ => [14, 17, 1, 2, 4, 0, 4],
    }
}

#[derive(Default)]
struct StoredZip {
    bytes: Vec<u8>,
    entries: Vec<ZipEntry>,
}

struct ZipEntry {
    name: String,
    crc32: u32,
    size: u32,
    offset: u32,
}

impl StoredZip {
    fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.bytes.len() as u32;
        let crc32 = crc32(data);
        let size = data.len() as u32;
        write_u32(&mut self.bytes, 0x0403_4b50);
        write_u16(&mut self.bytes, 20);
        write_u16(&mut self.bytes, 0);
        write_u16(&mut self.bytes, 0);
        write_u16(&mut self.bytes, 0);
        write_u16(&mut self.bytes, 0);
        write_u32(&mut self.bytes, crc32);
        write_u32(&mut self.bytes, size);
        write_u32(&mut self.bytes, size);
        write_u16(&mut self.bytes, name.len() as u16);
        write_u16(&mut self.bytes, 0);
        self.bytes.extend_from_slice(name.as_bytes());
        self.bytes.extend_from_slice(data);
        self.entries.push(ZipEntry {
            name: name.into(),
            crc32,
            size,
            offset,
        });
    }

    fn finish(mut self) -> Result<Vec<u8>, String> {
        let entry_count = u16::try_from(self.entries.len())
            .map_err(|_| "в отчёте слишком много файлов".to_string())?;
        let central_offset = self.bytes.len() as u32;
        for entry in &self.entries {
            write_u32(&mut self.bytes, 0x0201_4b50);
            write_u16(&mut self.bytes, 20);
            write_u16(&mut self.bytes, 20);
            write_u16(&mut self.bytes, 0);
            write_u16(&mut self.bytes, 0);
            write_u16(&mut self.bytes, 0);
            write_u16(&mut self.bytes, 0);
            write_u32(&mut self.bytes, entry.crc32);
            write_u32(&mut self.bytes, entry.size);
            write_u32(&mut self.bytes, entry.size);
            write_u16(&mut self.bytes, entry.name.len() as u16);
            write_u16(&mut self.bytes, 0);
            write_u16(&mut self.bytes, 0);
            write_u16(&mut self.bytes, 0);
            write_u16(&mut self.bytes, 0);
            write_u32(&mut self.bytes, 0);
            write_u32(&mut self.bytes, entry.offset);
            self.bytes.extend_from_slice(entry.name.as_bytes());
        }
        let central_size = self.bytes.len() as u32 - central_offset;
        write_u32(&mut self.bytes, 0x0605_4b50);
        write_u16(&mut self.bytes, 0);
        write_u16(&mut self.bytes, 0);
        write_u16(&mut self.bytes, entry_count);
        write_u16(&mut self.bytes, entry_count);
        write_u32(&mut self.bytes, central_size);
        write_u32(&mut self.bytes, central_offset);
        write_u16(&mut self.bytes, 0);
        Ok(self.bytes)
    }
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fem::{FemProject, SupportKind};
    use crate::{
        DiagramKind, FiberSide, Node, Ordinate, Point, Rod, SupportDirection, SupportMotion,
    };

    fn zero() -> Diagram {
        Diagram::new(
            DiagramKind::Linear,
            Ordinate::from_signed(0.0).unwrap(),
            Ordinate::from_signed(0.0).unwrap(),
            None,
            None,
            false,
        )
        .unwrap()
    }

    fn model() -> StructuralModel {
        StructuralModel {
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
                normal_diagram: zero(),
                shear_diagram: zero(),
                load_diagram: Diagram::new(
                    DiagramKind::Linear,
                    Ordinate::new(1.0, FiberSide::LowerOrRight).unwrap(),
                    Ordinate::new(1.0, FiberSide::LowerOrRight).unwrap(),
                    None,
                    None,
                    false,
                )
                .unwrap(),
                unit_x_diagram: zero(),
                unit_y_diagram: zero(),
                unit_rotation_diagram: zero(),
                unit_normal_x_diagram: zero(),
                unit_normal_y_diagram: zero(),
                unit_normal_rotation_diagram: zero(),
            }],
            target_node: 1,
            rigidity_presets: Vec::new(),
            thermal_presets: Vec::new(),
            supports: Vec::new(),
        }
    }

    #[test]
    fn report_is_a_docx_container_with_document_and_image() {
        let mut model = model();
        model.rods[0].unit_rotation_diagram = model.rods[0].load_diagram.clone();
        model.rods[0].temperature = Some(crate::TemperatureLoad {
            upper_or_left: 20.0,
            lower_or_right: 10.0,
        });
        let bytes = build_word_report(
            &model,
            Rigidity::Symbolic,
            ThermalExpansion::Symbolic,
            EffectsSelection {
                force: true,
                thermal: true,
                kinematic: false,
            },
        )
        .unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert!(bytes
            .windows("word/document.xml".len())
            .any(|part| part == b"word/document.xml"));
        assert!(bytes
            .windows("word/media/frame.png".len())
            .any(|part| part == b"word/media/frame.png"));
        assert!(bytes
            .windows("Поворот точки".len())
            .any(|part| part == "Поворот точки".as_bytes()));
        assert!(bytes
            .windows("Температурное".len())
            .any(|part| part == "Температурное".as_bytes()));
        assert!(bytes
            .windows("ИТОГО".len())
            .any(|part| part == "ИТОГО".as_bytes()));
        assert!(bytes
            .windows("kα".len())
            .any(|part| part == "kα".as_bytes()));
    }

    #[test]
    fn word_report_marks_classroom_rounding_mode() {
        let mut model = model();
        model.rods[0].unit_x_diagram = model.rods[0].load_diagram.clone();
        let bytes = build_word_report_with_rounding(
            &model,
            Rigidity::Symbolic,
            ThermalExpansion::Symbolic,
            EffectsSelection {
                force: true,
                thermal: false,
                kinematic: false,
            },
            Some(2),
        )
        .unwrap();
        assert!(bytes
            .windows("Учебный режим".len())
            .any(|part| part == "Учебный режим".as_bytes()));
    }

    #[test]
    fn empty_rotation_and_temperature_are_omitted_from_report() {
        let mut model = model();
        model.rods[0].unit_x_diagram = model.rods[0].load_diagram.clone();
        let bytes = build_word_report(
            &model,
            Rigidity::Symbolic,
            ThermalExpansion::Symbolic,
            EffectsSelection {
                force: true,
                thermal: true,
                kinematic: false,
            },
        )
        .unwrap();
        assert!(bytes
            .windows("Горизонтальное перемещение".len())
            .any(|w| w == "Горизонтальное перемещение".as_bytes()));
        assert!(!bytes
            .windows("Поворот точки".len())
            .any(|w| w == "Поворот точки".as_bytes()));
        assert!(!bytes
            .windows("Расчёт температурных перемещений".len())
            .any(|w| w == "Расчёт температурных перемещений".as_bytes()));
        assert!(!bytes.windows("φ(T)".len()).any(|w| w == "φ(T)".as_bytes()));
    }

    #[test]
    fn total_respects_selected_effects_and_symbolic_parameters() {
        let force = CalculationResult {
            contributions: Vec::new(),
            numerator: 6.0,
            numeric_part: 0.0,
            rigidity: Rigidity::Symbolic,
        };
        let thermal = ThermalCalculationResult {
            contributions: Vec::new(),
            coefficient: 2.0,
            numeric_part: 0.0,
            expansion: ThermalExpansion::Symbolic,
        };
        assert_eq!(
            combined_value(
                &force,
                &thermal,
                0.0,
                EffectsSelection {
                    force: true,
                    thermal: true,
                    kinematic: false,
                }
            ),
            "6 / EI + α · 2"
        );
        assert_eq!(
            combined_value(
                &force,
                &thermal,
                0.0,
                EffectsSelection {
                    force: false,
                    thermal: true,
                    kinematic: false,
                }
            ),
            "α · 2"
        );
    }

    #[test]
    fn disabled_mixed_effect_does_not_contribute_numeric_part() {
        let force = CalculationResult {
            contributions: vec![],
            numerator: 6.0,
            numeric_part: 0.5,
            rigidity: Rigidity::Symbolic,
        };
        let thermal = ThermalCalculationResult {
            contributions: vec![],
            coefficient: 2.0,
            numeric_part: 0.25,
            expansion: ThermalExpansion::Symbolic,
        };
        assert_eq!(
            combined_value(
                &force,
                &thermal,
                0.0,
                EffectsSelection {
                    force: false,
                    thermal: true,
                    kinematic: false
                }
            ),
            "α · 2 + 0.25"
        );
        assert_eq!(
            combined_value(
                &force,
                &thermal,
                0.0,
                EffectsSelection {
                    force: true,
                    thermal: false,
                    kinematic: false
                }
            ),
            "6 / EI + 0.5"
        );
    }

    #[test]
    fn mixed_symbolic_and_numeric_effects_are_summed_once() {
        let force = CalculationResult {
            contributions: Vec::new(),
            numerator: 6.0,
            numeric_part: 0.0,
            rigidity: Rigidity::numeric(2.0).unwrap(),
        };
        let thermal = ThermalCalculationResult {
            contributions: Vec::new(),
            coefficient: 2.0,
            numeric_part: 0.0,
            expansion: ThermalExpansion::Symbolic,
        };
        assert_eq!(
            combined_value(
                &force,
                &thermal,
                -1.0,
                EffectsSelection {
                    force: true,
                    thermal: true,
                    kinematic: true
                }
            ),
            "α · 2 + 2"
        );
    }

    #[test]
    fn report_includes_kinematic_section_only_when_selected() {
        let mut model = model();
        model.supports.push(SupportMotion {
            node: 0,
            direction: SupportDirection::Horizontal,
            normal_deg: 90.0,
            displacement: 0.5,
            unit_x_reaction: -2.0,
            unit_y_reaction: 0.0,
            unit_rotation_reaction: 0.0,
        });
        let selected = build_word_report(
            &model,
            Rigidity::Symbolic,
            ThermalExpansion::Symbolic,
            EffectsSelection {
                force: false,
                thermal: false,
                kinematic: true,
            },
        )
        .unwrap();
        let excluded = build_word_report(
            &model,
            Rigidity::Symbolic,
            ThermalExpansion::Symbolic,
            EffectsSelection {
                force: true,
                thermal: false,
                kinematic: false,
            },
        )
        .unwrap();
        let heading = "Расчёт кинематических перемещений".as_bytes();
        assert!(selected.windows(heading.len()).any(|part| part == heading));
        assert!(!excluded.windows(heading.len()).any(|part| part == heading));
        let thermal_heading = "Расчёт температурных перемещений".as_bytes();
        assert!(!selected
            .windows(thermal_heading.len())
            .any(|part| part == thermal_heading));
    }

    #[test]
    fn xml_escaping_preserves_valid_markup() {
        assert_eq!(xml_escape("A&B<1>"), "A&amp;B&lt;1&gt;");
    }

    #[test]
    fn fem_report_respects_section_checkboxes_and_contains_unit_images() {
        let model = model();
        let mut input = FemProject::default();
        input.reconcile(&model);
        input.target_node_id = "B".into();
        input.nodes[0].support = SupportKind::Fixed;
        input.nodes[1].fy = -1.0;
        input.members[0].ea = 1000.0;
        input.members[0].ei = 10.0;
        let result = input.solve(&model).unwrap();
        let full =
            build_fem_word_report(&model, &input, &result, FemReportSections::default()).unwrap();
        assert!(full.starts_with(b"PK"));
        assert!(full
            .windows("word/media/fem_epure_".len())
            .any(|w| w == b"word/media/fem_epure_"));
        assert!(full
            .windows("Единичные эпюры".len())
            .any(|w| w == "Единичные эпюры".as_bytes()));
        assert!(!full
            .windows("Узловые перемещения грузовой системы".len())
            .any(|w| w == "Узловые перемещения грузовой системы".as_bytes()));
        let reactions_only = FemReportSections {
            geometry: false,
            loads: false,
            stability: false,
            reactions: true,
            actual_diagrams: false,
            unit_diagrams: false,
        };
        let selected = build_fem_word_report(&model, &input, &result, reactions_only).unwrap();
        assert!(selected
            .windows("Опорные реакции".len())
            .any(|w| w == "Опорные реакции".as_bytes()));
        assert!(!selected
            .windows("Геометрия рамы".len())
            .any(|w| w == "Геометрия рамы".as_bytes()));
        assert!(!selected
            .windows("word/media/".len())
            .any(|w| w == b"word/media/"));

        input.members[0].ea = 0.0;
        input.members[0].ei = 0.0;
        let analytical = input.solve(&model).unwrap();
        let report =
            build_fem_word_report(&model, &input, &analytical, FemReportSections::default())
                .unwrap();
        assert!(report
            .windows("Рама статически определима".len())
            .any(|w| w == "Рама статически определима".as_bytes()));
    }

    #[test]
    fn fem_report_omits_unloaded_case_but_keeps_zero_diagram_in_loaded_case() {
        let model = model();
        let mut input = FemProject::default();
        input.reconcile(&model);
        input.target_node_id = "B".into();
        input.nodes[0].support = SupportKind::Fixed;
        let empty = input.solve(&model).unwrap();
        let sections = FemReportSections {
            geometry: false,
            loads: false,
            stability: false,
            reactions: true,
            actual_diagrams: true,
            unit_diagrams: false,
        };
        let bytes = build_fem_word_report(&model, &input, &empty, sections).unwrap();
        assert!(!bytes
            .windows("Грузовая система: эпюра".len())
            .any(|w| w == "Грузовая система: эпюра".as_bytes()));
        input.nodes[1].fx = 1.0;
        let loaded = input.solve(&model).unwrap();
        let bytes = build_fem_word_report(&model, &input, &loaded, sections).unwrap();
        assert!(bytes
            .windows("Грузовая система: эпюра Q".len())
            .any(|w| w == "Грузовая система: эпюра Q".as_bytes()));
        assert!(bytes.windows("N(x)".len()).any(|w| w == "N(x)".as_bytes()));
    }
}

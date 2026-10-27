//! Формирование переносимого отчёта Microsoft Word без внешнего рантайма.
//!
//! DOCX является ZIP-контейнером с XML Open Packaging Convention. Здесь
//! используется несжатый ZIP: файл немного больше, зато одинаково создаётся
//! на macOS и Windows и не требует установленного Word или Python.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use mohr_frame::{
    calculate_displacement_components, format_number, CalculationResult, Diagram,
    IntegrationMethod, Rigidity, StructuralModel,
};

const REPORT_IMAGE_WIDTH: u32 = 1000;
const REPORT_IMAGE_HEIGHT: u32 = 650;

/// Создаёт готовый DOCX-отчёт в памяти.
pub fn build_word_report(model: &StructuralModel, rigidity: Rigidity) -> Result<Vec<u8>, String> {
    model.validate()?;
    let results = calculate_displacement_components(model, rigidity)?;
    let scheme_png = render_scheme_png(model)?;
    let document_xml = build_document_xml(
        model,
        rigidity,
        &results.horizontal,
        &results.vertical,
        &results.rotation,
    )?;

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
    horizontal: &CalculationResult,
    vertical: &CalculationResult,
    rotation: &CalculationResult,
) -> Result<String, String> {
    let target = &model.nodes[model.target_node].id;
    let mut body = String::new();

    body.push_str(&paragraph(
        "Расчёт силовых перемещений и поворота точки плоской стержневой рамы методом Мора",
        Some("Title"),
        true,
    ));
    body.push_str(&paragraph(
        &format!(
            "Отчёт содержит исходную геометрию, заданные эпюры и вычисление горизонтального, вертикального перемещений и поворота точки {}. Итоговые значения: Δx = {}; Δy = {}; φ = {}.",
            xml_escape(target),
            xml_escape(&displacement_views(horizontal)),
            xml_escape(&displacement_views(vertical)),
            xml_escape(&displacement_views(rotation))
        ),
        None,
        false,
    ));

    body.push_str(&heading("Исходные данные", 1));
    body.push_str(&paragraph(
        &format!(
            "Базовая изгибная жёсткость: {}. Для каждого стержня EI участка равно коэффициенту k, умноженному на базовое EI.",
            xml_escape(&rigidity_text(rigidity))
        ),
        None,
        false,
    ));

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
    for rod in &model.rods {
        let start = &model.nodes[rod.start_node].id;
        let end = &model.nodes[rod.end_node].id;
        let length = model.rod_length(rod)?;
        rod_rows.push(vec![
            rod.id.clone(),
            format!("{start}–{end}"),
            format_number(length),
            format_number(rod.rigidity_factor),
            diagram_values(&rod.load_diagram, length),
            diagram_values(&rod.normal_diagram, length),
            diagram_values(&rod.shear_diagram, length),
        ]);
    }
    body.push_str(&caption("Таблица 2  Стержни и эпюры"));
    body.push_str(&table(
        &[
            "Стержень",
            "Узлы",
            "L",
            "k",
            "M  L; L/2; R",
            "N  L; L/2; R",
            "Q  L; L/2; R",
        ],
        &rod_rows,
    ));

    body.push_str(page_break());
    body.push_str(&heading("Расчётная схема", 1));
    body.push_str(&keep_next_paragraph(
        "Размерные линии выполнены засечками. Жёлтым кольцом отмечена точка, перемещение которой определяется.",
    ));
    body.push_str(image_paragraph());
    body.push_str(&caption("Рисунок 1  Геометрия рамы и размеры стержней"));

    body.push_str(&heading("Расчёт силовых перемещений и поворота", 1));
    body.push_str(&math_paragraph("Δx, Δy, φ = Σ ∫ Mₚ m₁ / (EIᵢ) dx"));
    body.push_str(empty_paragraph());
    body.push_str(&paragraph(
        "На линейных и постоянных участках применяется аналитическое перемножение трапеций. Если хотя бы одна эпюра параболическая, используется трёхточечная формула Симпсона. Различие жёсткостей учитывается делением вклада участка на коэффициент k.",
        None,
        false,
    ));

    append_component(
        &mut body,
        model,
        "Горизонтальное перемещение",
        "Δx",
        horizontal,
        3,
    );
    append_component(
        &mut body,
        model,
        "Вертикальное перемещение",
        "Δy",
        vertical,
        4,
    );
    append_component(&mut body, model, "Поворот точки", "φ", rotation, 5);

    body.push_str(&heading("Итог", 1));
    body.push_str(&final_result_paragraph(&format!(
        "Точка {}: Δx = {}",
        target,
        displacement_views(horizontal)
    )));
    body.push_str(&final_result_paragraph(&format!(
        "Точка {}: Δy = {}",
        target,
        displacement_views(vertical)
    )));
    body.push_str(&final_result_paragraph(&format!(
        "Точка {}: φ = {}",
        target,
        displacement_views(rotation)
    )));
    body.push_str(&paragraph(
        "Знаки Δx и Δy соответствуют направлениям глобальных осей X и Y. Положительный поворот φ направлен против часовой стрелки.",
        None,
        false,
    ));

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
                format_number(item.rigidity_factor),
                format_number(item.integral),
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
            "k",
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
            "Подстановка: {} = {} / {} = {}",
            substituted_integral(length, item),
            format_number(item.raw_integral),
            format_number(item.rigidity_factor),
            format_number(item.integral)
        )));
    }
    let sum = result
        .contributions
        .iter()
        .map(|item| format_number(item.integral))
        .collect::<Vec<_>>()
        .join(" + ");
    body.push_str(&calculation_paragraph(&format!(
        "Сумма без сокращений: Σ(∫Mₚ·m₁ dx / k) = {sum} = {}",
        format_number(result.numerator)
    )));
    body.push_str(empty_paragraph());
    body.push_str(&result_paragraph(&format!(
        "{symbol} = {}",
        displacement_views(result)
    )));
}

fn integration_formula(method: IntegrationMethod) -> &'static str {
    match method {
        IntegrationMethod::TrapezoidalProduct => {
            "I = L/6 · [Mₚ,L·(2·mL + mR) + Mₚ,R·(mL + 2·mR)] / k"
        }
        IntegrationMethod::SimpsonThreePoint => "I = L/6 · [Mₚ,L·mL + 4·Mₚ,m·mm + Mₚ,R·mR] / k",
    }
}

fn substituted_integral(length: f64, item: &mohr_frame::SegmentContribution) -> String {
    let load = item.load_values.map(format_number);
    let unit = item.unit_values.map(format_number);
    match item.method {
        IntegrationMethod::TrapezoidalProduct => format!(
            "{}/6 · [{}·(2·{} + {}) + {}·({} + 2·{})] / {}",
            format_number(length),
            load[0],
            unit[0],
            unit[2],
            load[2],
            unit[0],
            unit[2],
            format_number(item.rigidity_factor)
        ),
        IntegrationMethod::SimpsonThreePoint => format!(
            "{}/6 · [{}·{} + 4·{}·{} + {}·{}] / {}",
            format_number(length),
            load[0],
            unit[0],
            load[1],
            unit[1],
            load[2],
            unit[2],
            format_number(item.rigidity_factor)
        ),
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
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Расчёт силовых перемещений плоской стержневой рамы методом Мора</dc:title><dc:creator>Mohr Frame</dc:creator><dc:subject>Строительная механика</dc:subject></cp:coreProperties>"#
}

fn app_properties_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Mohr Frame</Application><AppVersion>0.5.0</AppVersion></Properties>"#
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
        draw_filled_circle(&mut image, point, 6, Rgb([255, 255, 255]));
        draw_circle(&mut image, point, 6, ink, 2);
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
    use mohr_frame::{DiagramKind, FiberSide, Node, Ordinate, Point, Rod};

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
                rigidity_factor: 1.0,
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
            }],
            target_node: 1,
        }
    }

    #[test]
    fn report_is_a_docx_container_with_document_and_image() {
        let bytes = build_word_report(&model(), Rigidity::Symbolic).unwrap();
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
    }

    #[test]
    fn xml_escaping_preserves_valid_markup() {
        assert_eq!(xml_escape("A&B<1>"), "A&amp;B&lt;1&gt;");
    }
}

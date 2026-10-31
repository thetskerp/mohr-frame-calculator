//! Хранение проектов, миграции формата и совместимый экспорт.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use mohr_frame::{Diagram, EffectsSelection, Node, Rigidity, StructuralModel, ThermalExpansion};
use serde::{Deserialize, Serialize};

use crate::canvas::CanvasViewState;

pub const CURRENT_SCHEMA_VERSION: u32 = 3;
const PROJECT_FORMAT_ID: &str = "mohr-frame-project";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectData {
    pub model: StructuralModel,
    pub rigidity: Rigidity,
    #[serde(default)]
    pub thermal_expansion: ThermalExpansion,
    #[serde(default)]
    pub effects: EffectsSelection,
    pub show_values: bool,
    #[serde(default)]
    pub view_state: CanvasViewState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectFileV2 {
    format: String,
    schema_version: u32,
    created_with: String,
    minimum_reader: String,
    data: ProjectData,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectV1 {
    format_version: u32,
    model: StructuralModel,
    rigidity: Rigidity,
    show_values: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectProbe {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    format_version: u32,
}

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub data: ProjectData,
    pub path: PathBuf,
    pub migrated_from_legacy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPaths {
    pub directory: PathBuf,
    pub scheme: PathBuf,
    pub report: PathBuf,
    pub legacy_export: PathBuf,
    pub backup: PathBuf,
}

pub fn save_project(project_name: &str, data: &ProjectData) -> Result<PathBuf, String> {
    data.model.validate()?;
    let paths = project_paths(project_name)?;
    fs::create_dir_all(&paths.directory).map_err(|error| {
        format!(
            "не удалось создать папку проекта {}: {error}",
            paths.directory.display()
        )
    })?;

    let file = ProjectFileV2 {
        format: PROJECT_FORMAT_ID.into(),
        schema_version: CURRENT_SCHEMA_VERSION,
        created_with: env!("CARGO_PKG_VERSION").into(),
        minimum_reader: "0.6.0".into(),
        data: data.clone(),
    };
    let text = ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
        .map_err(|error| format!("не удалось подготовить файл: {error}"))?;

    if paths.scheme.is_file() {
        if let Some(parent) = paths.backup.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("не удалось создать папку резервных копий: {error}"))?;
        }
        fs::copy(&paths.scheme, &paths.backup).map_err(|error| {
            format!(
                "не удалось создать резервную копию {}: {error}",
                paths.backup.display()
            )
        })?;
    }
    atomic_write(&paths.scheme, text.as_bytes())?;
    Ok(paths.scheme)
}

pub fn load_project(project_name: &str) -> Result<LoadedProject, String> {
    let paths = project_paths(project_name)?;
    // v0.5.0 хранил файл прямо в корне. После открытия обычное
    // сохранение перенесёт его в обособленную папку нового формата.
    let path = if paths.scheme.is_file() {
        paths.scheme
    } else {
        let legacy = legacy_project_path(project_name)?;
        if legacy.is_file() {
            legacy
        } else {
            paths.scheme
        }
    };
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("не удалось открыть {}: {error}", path.display()))?;
    let (data, migrated_from_legacy) = deserialize_project_text(&text)?;
    Ok(LoadedProject {
        data,
        path,
        migrated_from_legacy,
    })
}

pub fn save_word_report(project_name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let paths = project_paths(project_name)?;
    fs::create_dir_all(&paths.directory).map_err(|error| {
        format!(
            "не удалось создать папку проекта {}: {error}",
            paths.directory.display()
        )
    })?;
    atomic_write(&paths.report, bytes)?;
    Ok(paths.report)
}

/// Явный экспорт в старый формат. Поворот и состояние камеры
/// сознательно опускаются, чтобы v0.5.0 гарантированно прочитала файл.
pub fn export_for_v050(project_name: &str, data: &ProjectData) -> Result<PathBuf, String> {
    data.model.validate()?;
    let paths = project_paths(project_name)?;
    fs::create_dir_all(&paths.directory).map_err(|error| {
        format!(
            "не удалось создать папку проекта {}: {error}",
            paths.directory.display()
        )
    })?;
    let legacy = LegacyProjectExport {
        format_version: 1,
        model: LegacyModelExport {
            nodes: &data.model.nodes,
            rods: data.model.rods.iter().map(LegacyRodExport::from).collect(),
            target_node: data.model.target_node,
        },
        rigidity: data.rigidity,
        show_values: data.show_values,
    };
    let text = ron::ser::to_string_pretty(&legacy, ron::ser::PrettyConfig::default())
        .map_err(|error| format!("не удалось подготовить совместимую копию: {error}"))?;
    atomic_write(&paths.legacy_export, text.as_bytes())?;
    Ok(paths.legacy_export)
}

pub(crate) fn deserialize_project_text(text: &str) -> Result<(ProjectData, bool), String> {
    let probe: ProjectProbe =
        ron::from_str(text).map_err(|error| format!("файл проекта повреждён: {error}"))?;
    if probe.schema_version != 0 {
        let schema_version = probe.schema_version;
        if schema_version > CURRENT_SCHEMA_VERSION {
            return Err(format!(
                "файл создан более новой версией программы (schema {schema_version}); обновите Mohr Frame"
            ));
        }
        if schema_version < 2 {
            return Err(format!("версия схемы {schema_version} не поддерживается"));
        }
        let file: ProjectFileV2 =
            ron::from_str(text).map_err(|error| format!("файл проекта повреждён: {error}"))?;
        if file.format != PROJECT_FORMAT_ID {
            return Err(format!("неизвестный формат проекта: {}", file.format));
        }
        file.data.model.validate()?;
        return Ok((file.data, schema_version < CURRENT_SCHEMA_VERSION));
    }

    if probe.format_version == 1 {
        let legacy: LegacyProjectV1 =
            ron::from_str(text).map_err(|error| format!("файл v0.5.0 повреждён: {error}"))?;
        if legacy.format_version != 1 {
            return Err("неподдерживаемая версия файла".into());
        }
        legacy.model.validate()?;
        return Ok((
            ProjectData {
                model: legacy.model,
                rigidity: legacy.rigidity,
                thermal_expansion: ThermalExpansion::Symbolic,
                effects: EffectsSelection::default(),
                show_values: legacy.show_values,
                view_state: CanvasViewState::default(),
            },
            true,
        ));
    }

    Err("не удалось определить версию файла проекта".into())
}

#[derive(Serialize)]
struct LegacyProjectExport<'a> {
    format_version: u32,
    model: LegacyModelExport<'a>,
    rigidity: Rigidity,
    show_values: bool,
}

#[derive(Serialize)]
struct LegacyModelExport<'a> {
    nodes: &'a [Node],
    rods: Vec<LegacyRodExport<'a>>,
    target_node: usize,
}

#[derive(Serialize)]
struct LegacyRodExport<'a> {
    id: &'a str,
    start_node: usize,
    end_node: usize,
    rigidity_factor: f64,
    normal_diagram: &'a Diagram,
    shear_diagram: &'a Diagram,
    load_diagram: &'a Diagram,
    unit_x_diagram: &'a Diagram,
    unit_y_diagram: &'a Diagram,
}

impl<'a> From<&'a mohr_frame::Rod> for LegacyRodExport<'a> {
    fn from(rod: &'a mohr_frame::Rod) -> Self {
        Self {
            id: &rod.id,
            start_node: rod.start_node,
            end_node: rod.end_node,
            rigidity_factor: rod.rigidity_factor,
            normal_diagram: &rod.normal_diagram,
            shear_diagram: &rod.shear_diagram,
            load_diagram: &rod.load_diagram,
            unit_x_diagram: &rod.unit_x_diagram,
            unit_y_diagram: &rod.unit_y_diagram,
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "некорректное имя файла".to_string())?;
    let temporary = path.with_file_name(format!(".{file_name}.tmp"));
    let mut file = File::create(&temporary)
        .map_err(|error| format!("не удалось создать {}: {error}", temporary.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("не удалось записать {}: {error}", temporary.display()))?;

    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("не удалось заменить {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("не удалось сохранить {}: {error}", path.display()))
}

pub fn sanitize_project_name_input(value: &str) -> String {
    let trimmed = value.trim();
    let lowercase = trimmed.to_ascii_lowercase();
    let name = if lowercase.ends_with(".mohr.ron") {
        &trimmed[..trimmed.len() - ".mohr.ron".len()]
    } else if lowercase.ends_with(".ron") {
        &trimmed[..trimmed.len() - ".ron".len()]
    } else {
        trimmed
    };
    name.chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, ' ' | '_' | '-'))
        .take(64)
        .collect()
}

fn validated_project_name(value: &str) -> Result<&str, String> {
    let name = value.trim();
    if name.is_empty() {
        return Err("задайте имя проекта".into());
    }
    if name.len() > 128
        || !name
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, ' ' | '_' | '-'))
    {
        return Err(
            "имя проекта может содержать только буквы, цифры, пробел, дефис и подчёркивание".into(),
        );
    }
    let uppercase = name.to_ascii_uppercase();
    let reserved = matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            uppercase.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        });
    if reserved {
        return Err("это имя зарезервировано Windows; выберите другое".into());
    }
    Ok(name)
}

pub fn project_paths(project_name: &str) -> Result<ProjectPaths, String> {
    let name = validated_project_name(project_name)?;
    let directory = project_root_dir().join(name);
    Ok(ProjectPaths {
        scheme: directory.join(format!("{name}.mohr.ron")),
        report: directory.join(format!("{name}-report.docx")),
        legacy_export: directory.join(format!("{name}-for-v0.5.0.mohr.ron")),
        backup: directory
            .join("backups")
            .join(format!("{name}-previous.mohr.ron")),
        directory,
    })
}

fn legacy_project_path(project_name: &str) -> Result<PathBuf, String> {
    let name = validated_project_name(project_name)?;
    Ok(project_root_dir().join(format!("{name}.mohr.ron")))
}

pub fn project_root_dir() -> PathBuf {
    if let Ok(current) = std::env::current_dir() {
        if current.join("Cargo.toml").is_file() {
            return current;
        }
    }
    let Some(executable_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from(".");
    };
    if executable_dir.ends_with(Path::new("Contents/MacOS")) {
        let fallback = executable_dir.clone();
        return executable_dir
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map_or(fallback, Path::to_path_buf);
    }
    executable_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use mohr_frame::{Diagram, Node, Point};

    fn sample_data() -> ProjectData {
        ProjectData {
            model: StructuralModel {
                nodes: vec![
                    Node {
                        id: "A".into(),
                        position: Point { x: 0.0, y: 0.0 },
                    },
                    Node {
                        id: "B".into(),
                        position: Point { x: 1.0, y: 0.0 },
                    },
                ],
                rods: vec![mohr_frame::Rod {
                    id: "AB".into(),
                    start_node: 0,
                    end_node: 1,
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
                }],
                target_node: 1,
            },
            rigidity: Rigidity::Symbolic,
            thermal_expansion: ThermalExpansion::Symbolic,
            effects: EffectsSelection::default(),
            show_values: true,
            view_state: CanvasViewState::default(),
        }
    }

    #[test]
    fn project_uses_dedicated_folder_and_fixed_extension() {
        let paths = project_paths("first_scheme").unwrap();
        assert_eq!(paths.directory.file_name().unwrap(), "first_scheme");
        assert_eq!(paths.scheme.file_name().unwrap(), "first_scheme.mohr.ron");
        assert_eq!(
            paths.report.file_name().unwrap(),
            "first_scheme-report.docx"
        );
        assert!(project_paths("first_scheme.mohr.ron").is_err());
        assert!(project_paths("../first_scheme").is_err());
        assert!(project_paths("CON").is_err());
        assert!(project_paths("").is_err());
    }

    #[test]
    fn legacy_filename_is_migrated_to_name_only() {
        assert_eq!(sanitize_project_name_input("scheme.mohr.ron"), "scheme");
        assert_eq!(sanitize_project_name_input("Моя схема.ron"), "Моя схема");
        assert_eq!(sanitize_project_name_input("bad/name?.ron"), "badname");
    }

    #[test]
    fn current_format_roundtrip_preserves_view_state() {
        let mut data = sample_data();
        data.view_state.camera.zoom = 2.5;
        data.view_state.ordinate_scales[3] = 1.75;
        let file = ProjectFileV2 {
            format: PROJECT_FORMAT_ID.into(),
            schema_version: CURRENT_SCHEMA_VERSION,
            created_with: "test".into(),
            minimum_reader: "0.5.1".into(),
            data: data.clone(),
        };
        let text = ron::ser::to_string(&file).unwrap();
        let (restored, migrated) = deserialize_project_text(&text).unwrap();
        assert!(!migrated);
        assert_eq!(restored, data);
    }

    #[test]
    fn v050_file_is_migrated_with_default_view_and_rotation() {
        let (restored, migrated) =
            deserialize_project_text(include_str!("../examples/example-frame.mohr.ron")).unwrap();
        assert!(migrated);
        assert_eq!(restored.view_state, CanvasViewState::default());
        assert!(restored
            .model
            .rods
            .iter()
            .all(|rod| rod.unit_rotation_diagram == Diagram::default()));
        assert!(restored.model.rods.iter().all(|rod| {
            rod.section_height == 0.5
                && rod.thermal_expansion_factor == 1.0
                && rod.temperature.is_none()
                && rod.unit_normal_x_diagram == Diagram::default()
                && rod.unit_normal_y_diagram == Diagram::default()
                && rod.unit_normal_rotation_diagram == Diagram::default()
        }));
        assert_eq!(restored.thermal_expansion, ThermalExpansion::Symbolic);
        assert_eq!(restored.effects, EffectsSelection::default());
    }

    #[test]
    fn v051_schema_is_accepted_and_marked_for_migration() {
        let file = ProjectFileV2 {
            format: PROJECT_FORMAT_ID.into(),
            schema_version: 2,
            created_with: "0.5.1".into(),
            minimum_reader: "0.5.1".into(),
            data: sample_data(),
        };
        let text = ron::ser::to_string(&file).unwrap();
        let (_, migrated) = deserialize_project_text(&text).unwrap();
        assert!(migrated);
    }

    #[test]
    fn future_schema_is_rejected_with_clear_message() {
        let text = "(schema_version: 99, format: \"mohr-frame-project\")";
        let error = deserialize_project_text(text).unwrap_err();
        assert!(error.contains("более новой версией"));
    }

    #[test]
    fn legacy_export_omits_new_rotation_and_view_fields() {
        let data = sample_data();
        let legacy = LegacyProjectExport {
            format_version: 1,
            model: LegacyModelExport {
                nodes: &data.model.nodes,
                rods: data.model.rods.iter().map(LegacyRodExport::from).collect(),
                target_node: data.model.target_node,
            },
            rigidity: data.rigidity,
            show_values: data.show_values,
        };
        let text = ron::ser::to_string(&legacy).unwrap();
        assert!(!text.contains("unit_rotation_diagram"));
        assert!(!text.contains("view_state"));
        let (_, migrated) = deserialize_project_text(&text).unwrap();
        assert!(migrated);
    }
}

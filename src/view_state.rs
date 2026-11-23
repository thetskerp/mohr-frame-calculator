//! Сохраняемое состояние инженерного холста, независимое от библиотеки GUI.

use serde::{Deserialize, Serialize};

pub const MIN_CAMERA_ZOOM: f32 = 0.2;
pub const MAX_CAMERA_ZOOM: f32 = 10.0;
pub const MIN_DIAGRAM_SCALE: f32 = 0.1;
pub const MAX_DIAGRAM_SCALE: f32 = 10.0;
pub const DIAGRAM_COUNT: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasCamera {
    pub zoom: f32,
    pub center_offset: [f32; 2],
}

impl Default for CanvasCamera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center_offset: [0.0, 0.0],
        }
    }
}

impl CanvasCamera {
    pub fn pan_from_drag(&mut self, origin: DragOrigin, dx: f32, dy: f32) {
        self.center_offset = origin.center_offset;
        self.center_offset[0] -= dx / origin.screen_scale;
        self.center_offset[1] += dy / origin.screen_scale;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragOrigin {
    pub center_offset: [f32; 2],
    pub screen_scale: f32,
}

fn default_ordinate_scales() -> Vec<f32> {
    vec![1.0; DIAGRAM_COUNT]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasViewState {
    #[serde(default)]
    pub camera: CanvasCamera,
    #[serde(default = "default_ordinate_scales")]
    pub ordinate_scales: Vec<f32>,
    /// Текущий жест не сохраняется в проекте.
    #[serde(skip)]
    pub drag_origin: Option<DragOrigin>,
}

impl Default for CanvasViewState {
    fn default() -> Self {
        Self {
            camera: CanvasCamera::default(),
            ordinate_scales: default_ordinate_scales(),
            drag_origin: None,
        }
    }
}

impl CanvasViewState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn diagram_scale(&self, index: usize) -> f32 {
        self.ordinate_scales.get(index).copied().unwrap_or(1.0)
    }

    pub fn scale_diagram(&mut self, index: usize, factor: f32) {
        if self.ordinate_scales.len() < DIAGRAM_COUNT {
            self.ordinate_scales.resize(DIAGRAM_COUNT, 1.0);
        }
        if let Some(scale) = self.ordinate_scales.get_mut(index) {
            *scale = (*scale * factor).clamp(MIN_DIAGRAM_SCALE, MAX_DIAGRAM_SCALE);
        }
    }

    pub fn set_diagram_scale(&mut self, index: usize, scale: f32) {
        if self.ordinate_scales.len() < DIAGRAM_COUNT {
            self.ordinate_scales.resize(DIAGRAM_COUNT, 1.0);
        }
        if let Some(value) = self.ordinate_scales.get_mut(index) {
            *value = scale.clamp(MIN_DIAGRAM_SCALE, MAX_DIAGRAM_SCALE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gesture_state_is_not_serialized() {
        let mut view = CanvasViewState {
            drag_origin: Some(DragOrigin {
                center_offset: [1.0, 2.0],
                screen_scale: 4.0,
            }),
            ..Default::default()
        };
        view.camera.zoom = 2.0;
        let text = ron::to_string(&view).unwrap();
        let restored: CanvasViewState = ron::from_str(&text).unwrap();
        assert_eq!(restored.camera.zoom, 2.0);
        assert!(restored.drag_origin.is_none());
    }
}

use bevy::prelude::*;
use bevy_inspector_egui::egui;

/// An offscreen image a camera renders to, which is displayed inside a dock tab.
#[derive(Default)]
pub(super) struct ViewTarget {
    pub(super) image: Handle<Image>,
    pub(super) texture: egui::TextureId,
    /// The area the tab occupied this frame, or `None` if the tab is not visible.
    pub(super) rect: Option<egui::Rect>,
}

impl ViewTarget {
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        self.rect = Some(ui.available_rect_before_wrap());
        ui.add(egui::Image::new(egui::load::SizedTexture::new(self.texture, size)));
    }

    pub(super) fn is_visible(&self) -> bool {
        self.rect.is_some()
    }

    pub(super) fn contains(&self, pos: Vec2) -> bool {
        self.rect.is_some_and(|rect| rect.contains(egui::pos2(pos.x, pos.y)))
    }

    pub(super) fn try_resize(&self, scale_factor: f32) -> Option<UVec2> {
        self.rect.map(|rect| {
            let size = rect.size() * scale_factor;
            UVec2::new(size.x as u32, size.y as u32)
        }).filter(|size| size.x > 0 && size.y > 0)
    }
}

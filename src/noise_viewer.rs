mod noise;

use crate::noise::{NoiseSettings, NoiseType};

use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_inspector_egui::bevy_egui::{
    EguiContexts, EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, egui,
};

pub const CHUNK_WIDTH: usize = 32;
pub const CHUNK_HEIGHT: usize = 64;

const SIZE: u32 = 512;

#[derive(Resource)]
struct Viewer {
    settings: NoiseSettings,
    seed: i32,
    image: Handle<Image>,
    texture: Option<egui::TextureId>,
    show_chunks: bool,
    dirty: bool,
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, EguiPlugin::default()))
        .add_systems(Startup, setup)
        .add_systems(EguiPrimaryContextPass, ui)
        .run();
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn(Camera2d);

    let image = images.add(Image::new_fill(
        Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));

    commands.insert_resource(Viewer {
        settings: NoiseSettings { frequency: 0.01, ..default() },
        seed: 0,
        image,
        texture: None,
        show_chunks: false,
        dirty: true,
    });
}

fn render(viewer: &Viewer, image: &mut Image) {
    // Maps the output range [bias - amplitude, bias + amplitude] to [0, 255].
    let range = viewer.settings.amplitude.abs().max(f32::EPSILON);
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let value = viewer.settings.sample2(viewer.seed, x as f32, y as f32);
            let t = ((value - viewer.settings.bias) / range * 0.5 + 0.5).clamp(0.0, 1.0);
            let v = (t * 255.0) as u8;
            data.extend_from_slice(&[v, v, v, 255]);
        }
    }

    image.data = Some(data);
}

fn ui(
    mut contexts: EguiContexts,
    mut viewer: ResMut<Viewer>,
    mut images: ResMut<Assets<Image>>,
) -> Result {
    if viewer.texture.is_none() {
        let handle = viewer.image.clone();
        viewer.texture = Some(contexts.add_image(EguiTextureHandle::Strong(handle)).into());
    }

    if viewer.dirty {
        if let Some(mut image) = images.get_mut(&viewer.image) {
            render(&viewer, &mut image);
            viewer.dirty = false;
        }
    }

    let texture = viewer.texture.unwrap();
    let ctx = contexts.ctx_mut()?;
    let mut changed = false;

    egui::Window::new("Noise settings").default_pos([10.0, 10.0]).show(ctx, |ui| {
        let v = &mut *viewer;
        let s = &mut v.settings;

        egui::ComboBox::from_label("Type")
            .selected_text(format!("{:?}", s.noise_type))
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(&mut s.noise_type, NoiseType::OpenSimplex2S, "OpenSimplex2S")
                    .changed();
                changed |= ui
                    .selectable_value(&mut s.noise_type, NoiseType::Voronoi, "Voronoi")
                    .changed();
            });

        changed |= ui.add(egui::DragValue::new(&mut v.seed).prefix("Seed: ")).changed();
        changed |= ui.add(egui::Slider::new(&mut s.octaves, 1..=10).text("Octaves")).changed();
        changed |= ui.add(egui::Slider::new(&mut s.frequency, 0.0001..=0.2).logarithmic(true).text("Frequency")).changed();
        changed |= ui.add(egui::Slider::new(&mut s.persistence, 0.0..=1.0).text("Persistence")).changed();
        changed |= ui.add(egui::Slider::new(&mut s.lacunarity, 1.0..=4.0).text("Lacunarity")).changed();
        changed |= ui.add(egui::Slider::new(&mut s.amplitude, 0.0..=100.0).text("Amplitude")).changed();
        changed |= ui.add(egui::Slider::new(&mut s.bias, -100.0..=100.0).text("Bias")).changed();

        ui.horizontal(|ui| {
            ui.label("Offset");
            changed |= ui.add(egui::DragValue::new(&mut s.offset.x).speed(1.0)).changed();
            changed |= ui.add(egui::DragValue::new(&mut s.offset.y).speed(1.0)).changed();
        });

        if ui.button("Reset").clicked() {
            *s = NoiseSettings::default();
            changed = true;
        }
    });

    egui::Window::new("Display settings").default_pos([10.0, 400.0]).show(ctx, |ui| {
        ui.checkbox(&mut viewer.show_chunks, "Show chunk borders");
    });

    let screen = ctx.content_rect();
    let side = screen.width().min(screen.height());
    let rect = egui::Rect::from_center_size(screen.center(), egui::vec2(side, side));
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.image(
        texture,
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    if viewer.show_chunks {
        // Pixel (0, 0) samples world (-SIZE / 2, -SIZE / 2); chunk borders lie on multiples of CHUNK_WIDTH.
        let half = (SIZE / 2) as i32;
        let chunk = CHUNK_WIDTH as i32;
        let scale = side / SIZE as f32;
        let stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        let first = (-half).div_euclid(chunk) * chunk;
        for w in (first..=half).step_by(chunk as usize) {
            let t = (w + half) as f32 * scale;
            if t < 0.0 || t > side {
                continue;
            }

            painter.vline(rect.min.x + t, rect.y_range(), stroke);
            painter.hline(rect.x_range(), rect.min.y + t, stroke);
        }
    }

    if changed {
        viewer.dirty = true;
    }

    Ok(())
}

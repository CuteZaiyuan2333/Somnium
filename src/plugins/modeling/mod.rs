use bevy::prelude::*;
use bevy::pbr::Material;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::light::PointLight;
use bevy::camera::Viewport;
use bevy_egui::EguiContexts;
use egui::{Ui, WidgetText, UiKind, Id, Rect};
use crate::{Plugin, TabInstance, AppCommand, Tab};

// --- Bevy Material ---

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SdfMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    #[uniform(0)]
    pub time: f32,
}

impl Material for SdfMaterial {
    fn fragment_shader() -> ShaderRef {
        "plugins/modeling/sdf.wgsl".into()
    }
}

// --- Somnium Plugin ---

#[derive(Default)]
pub struct ModelingPlugin;

pub fn create() -> ModelingPlugin {
    ModelingPlugin::default()
}

impl Plugin for ModelingPlugin {
    fn name(&self) -> &str { "modeling" }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("SDF Base Model").clicked() {
            let tab = Tab::new(Box::new(ModelingTab::default()));
            control.push(AppCommand::OpenTab(tab));
            ui.close_kind(UiKind::Menu);
        }
    }
}

// --- Tab Implementation ---

#[derive(Debug, Clone, Default)]
pub struct ModelingTab;

impl TabInstance for ModelingTab {
    fn title(&self) -> WidgetText { "SDF Modeler".into() }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        ui.vertical(|ui| {
            ui.heading("SDF Viewport");
            ui.label("The 3D view is accurately synced to this area.");
            ui.separator();

            // 1. 获取扣除标题后的剩余可用区域
            let rect = ui.available_rect_before_wrap();
            
            // 2. 在 egui 中占位，防止其他组件侵入
            ui.allocate_rect(rect, egui::Sense::hover());

            // 3. 将精确的矩形区域传递给 Bevy
            ui.ctx().data_mut(|d| {
                d.insert_temp(Id::new("sdf_viewport_rect"), rect);
                d.insert_temp(Id::new("sdf_viewport_active"), true);
            });
        });
    }

    fn box_clone(&self) -> Box<dyn TabInstance> { Box::new(self.clone()) }
}

// --- Bevy Systems ---

#[derive(Component)]
pub struct ModelingCamera;

pub fn setup_modeling_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SdfMaterial>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    // 3D 相机
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            is_active: false,
            ..default()
        },
        ModelingCamera,
        Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // 测试立方体
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(1.0)))),
        MeshMaterial3d(std_materials.add(StandardMaterial {
            base_color: Color::BLACK.into(),
            emissive: LinearRgba::RED,
            ..default()
        })),
        Transform::from_xyz(-1.5, 0.0, 0.0),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(3.0)))),
        MeshMaterial3d(materials.add(SdfMaterial {
            color: LinearRgba::rgb(0.2, 0.7, 1.0),
            time: 0.0,
        })),
        Transform::from_xyz(1.5, 0.0, 0.0),
    ));

    commands.spawn((
        PointLight {
            intensity: 10_000_000.0,
            range: 100.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
}

pub fn sync_modeling_viewport(
    mut contexts: EguiContexts,
    mut query: Query<(&mut Camera, &mut Projection), With<ModelingCamera>>,
    window_query: Query<&Window>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let Some(window) = window_query.iter().next() else { return };
    let scale_factor = window.scale_factor();

    let mut camera_active = false;
    
    let (rect, active) = ctx.data_mut(|d| {
        let r = d.get_temp::<Rect>(Id::new("sdf_viewport_rect"));
        let a = d.get_temp::<bool>(Id::new("sdf_viewport_active")).unwrap_or(false);
        (r, a)
    });

    if let (Some(rect), true) = (rect, active) {
        if let Some((mut camera, mut projection)) = query.iter_mut().next() {
            // --- 修正后的坐标计算 ---
            // Bevy 0.17 Viewport.physical_position 原点是左上角 (Top-Left)
            // egui 的 rect.min 也是左上角偏移
            let min_x = (rect.min.x * scale_factor) as u32;
            let min_y = (rect.min.y * scale_factor) as u32; // 直接使用 min.y
            let width = (rect.width() * scale_factor) as u32;
            let height = (rect.height() * scale_factor) as u32;

            if width > 1 && height > 1 {
                camera.viewport = Some(Viewport {
                    physical_position: UVec2::new(min_x, min_y),
                    physical_size: UVec2::new(width, height),
                    depth: 0.0..1.0,
                });
                camera.is_active = true;
                camera_active = true;
                
                if let Projection::Perspective(ref mut p) = *projection {
                    p.aspect_ratio = width as f32 / height as f32;
                }
            }
        }
    }

    if !camera_active {
        for (mut camera, _) in query.iter_mut() {
            camera.is_active = false;
        }
    }
    
    ctx.data_mut(|d| d.insert_temp(Id::new("sdf_viewport_active"), false));
}

pub fn update_sdf_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<SdfMaterial>>,
) {
    for (_, material) in materials.iter_mut() {
        material.time = time.elapsed_secs();
    }
}

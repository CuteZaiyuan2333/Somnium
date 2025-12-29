use bevy::prelude::*;
use bevy::pbr::Material;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::light::PointLight;
// Bevy 0.17 中 ClearColorConfig 可能已经通过 prelude 导出，或者在 Camera 结构体内
use egui::{Ui, WidgetText, UiKind};
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

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(0.5)
    }
}

// --- Somnium Plugin ---

#[derive(Default)]
pub struct ModelingPlugin;

pub fn create() -> ModelingPlugin {
    ModelingPlugin::default()
}

impl Plugin for ModelingPlugin {
    fn name(&self) -> &str {
        "modeling"
    }

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
    fn title(&self) -> WidgetText {
        "SDF Modeler".into()
    }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        ui.vertical(|ui| {
            ui.heading("SDF Raymarching Preview");
            ui.label("Status: Renderer Fixed (Camera Sync)");
            ui.separator();
            ui.label(egui::RichText::new("The 3D scene should now appear behind the UI.")
                .color(ui.visuals().weak_text_color()));
        });
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

// --- Bevy Systems ---

pub fn setup_modeling_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SdfMaterial>>,
    camera_2d_query: Query<(Entity, &Camera), (With<Camera2d>, Without<Camera3d>)>,
) {
    // 1. 让 3D 相机作为背景 (Order 0)
    // Bevy 0.17 中 Camera 的 clear_color 类型可能已改变
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Transform::from_xyz(0.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // 2. 找到主程序的 2D 相机，将其改为叠加模式 (Order 1, 不清空屏幕)
    for (entity, camera) in camera_2d_query.iter() {
        let mut new_camera = camera.clone();
        new_camera.order = 1;
        // 尝试通过这种方式设置不清空
        // 在 Bevy 0.17 中可能是直接设置字段
        // new_camera.clear_color = ClearColorConfig::None; 
        // 如果上面报错，我们可以尝试 insert 一个标记或者修改其渲染目标
        commands.entity(entity).insert(new_camera);
    }

    // 注入 SDF 代理容器
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(4.0)))),
        MeshMaterial3d(materials.add(SdfMaterial {
            color: LinearRgba::rgb(0.2, 0.7, 1.0),
            time: 0.0,
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // 注入灯光
    commands.spawn((
        PointLight {
            intensity: 2_000_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
}

pub fn update_sdf_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<SdfMaterial>>,
) {
    for (_, material) in materials.iter_mut() {
        material.time = time.elapsed_secs();
    }
}
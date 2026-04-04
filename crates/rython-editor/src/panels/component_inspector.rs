use std::sync::Arc;

use rython_ecs::component::{
    BillboardComponent, ColliderComponent, MeshComponent, RigidBodyComponent, TagComponent,
    TransformComponent,
};
use rython_ecs::{EntityId, Scene};

use crate::panels::asset_browser::{AssetBrowserPanel, AssetCategory};
use crate::state::undo::{AttachComponent, DetachComponent, ModifyComponent, UndoStack};

const ALL_COMPONENT_TYPES: &[&str] = &[
    "TransformComponent",
    "MeshComponent",
    "TagComponent",
    "RigidBodyComponent",
    "ColliderComponent",
    "BillboardComponent",
];

pub struct ComponentInspectorPanel;

impl ComponentInspectorPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        entity: EntityId,
        scene: &Arc<Scene>,
        undo: &mut UndoStack,
        project_dirty: &mut bool,
        asset_browser: &AssetBrowserPanel,
    ) {
        ui.heading("Inspector");
        ui.separator();

        // Build the present-type list cheaply using has::<T>() checks
        // instead of snapshot_entity which serializes all component data.
        let mut present: Vec<&str> = Vec::new();
        if scene.components.has::<TransformComponent>(entity) { present.push("TransformComponent"); }
        if scene.components.has::<MeshComponent>(entity) { present.push("MeshComponent"); }
        if scene.components.has::<TagComponent>(entity) { present.push("TagComponent"); }
        if scene.components.has::<RigidBodyComponent>(entity) { present.push("RigidBodyComponent"); }
        if scene.components.has::<ColliderComponent>(entity) { present.push("ColliderComponent"); }
        if scene.components.has::<BillboardComponent>(entity) { present.push("BillboardComponent"); }

        // Render each present component
        for type_name in &present {
            let type_name = *type_name;

            ui.push_id(type_name, |ui| {
                let header = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id(type_name),
                    true,
                );
                header
                    .show_header(ui, |ui| {
                        ui.strong(type_name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                let cmd = DetachComponent::capture(entity, type_name, scene);
                                undo.push(Box::new(cmd), scene);
                                *project_dirty = true;
                            }
                        });
                    })
                    .body(|ui| {
                        match type_name {
                            "TransformComponent" => {
                                show_transform(ui, entity, scene, undo, project_dirty);
                            }
                            "MeshComponent" => { show_mesh(ui, entity, scene, undo, project_dirty, asset_browser); }
                            "TagComponent" => { show_tag(ui, entity, scene, undo, project_dirty); }
                            "RigidBodyComponent" => {
                                show_rigid_body(ui, entity, scene, undo, project_dirty);
                            }
                            "ColliderComponent" => {
                                show_collider(ui, entity, scene, undo, project_dirty);
                            }
                            "BillboardComponent" => {
                                show_billboard(ui, entity, scene, undo, project_dirty);
                            }
                            _ => {}
                        }
                    });
            });

            ui.add_space(4.0);
        }

        ui.separator();

        // "Add Component" — show types not already present
        let missing: Vec<&str> = ALL_COMPONENT_TYPES
            .iter()
            .copied()
            .filter(|t| !present.contains(t))
            .collect();

        if !missing.is_empty() {
            ui.menu_button("+ Add Component", |ui| {
                for type_name in &missing {
                    if ui.button(*type_name).clicked() {
                        let cmd = AttachComponent::new(entity, type_name);
                        undo.push(Box::new(cmd), scene);
                        *project_dirty = true;
                        ui.close_menu();
                    }
                }
            });
        }
    }
}

// ── Per-component editors ─────────────────────────────────────────────────────

fn show_transform(
    ui: &mut egui::Ui,
    entity: EntityId,
    scene: &Arc<Scene>,
    undo: &mut UndoStack,
    project_dirty: &mut bool,
) -> bool {
    let Some(mut t) = scene.components.get::<TransformComponent>(entity) else {
        return false;
    };
    let old_json = serde_json::to_value(&t).expect("TransformComponent must be serializable");
    let mut changed = false;

    ui.label("Position");
    egui::Grid::new("pos").num_columns(2).show(ui, |ui| {
        ui.label("X");
        changed |= ui.add(egui::DragValue::new(&mut t.x).speed(0.1)).changed();
        ui.end_row();
        ui.label("Y");
        changed |= ui.add(egui::DragValue::new(&mut t.y).speed(0.1)).changed();
        ui.end_row();
        ui.label("Z");
        changed |= ui.add(egui::DragValue::new(&mut t.z).speed(0.1)).changed();
        ui.end_row();
    });

    ui.label("Rotation (degrees)");
    let mut rot_x_deg = t.rot_x.to_degrees();
    let mut rot_y_deg = t.rot_y.to_degrees();
    let mut rot_z_deg = t.rot_z.to_degrees();
    egui::Grid::new("rot").num_columns(2).show(ui, |ui| {
        ui.label("X");
        if ui.add(egui::DragValue::new(&mut rot_x_deg).speed(1.0)).changed() {
            t.rot_x = rot_x_deg.to_radians();
            changed = true;
        }
        ui.end_row();
        ui.label("Y");
        if ui.add(egui::DragValue::new(&mut rot_y_deg).speed(1.0)).changed() {
            t.rot_y = rot_y_deg.to_radians();
            changed = true;
        }
        ui.end_row();
        ui.label("Z");
        if ui.add(egui::DragValue::new(&mut rot_z_deg).speed(1.0)).changed() {
            t.rot_z = rot_z_deg.to_radians();
            changed = true;
        }
        ui.end_row();
    });

    ui.label("Scale");
    // Uniform lock checkbox — stored in transient UI state via egui memory
    let uniform_id = ui.id().with("uniform_scale");
    let mut uniform = ui.data_mut(|d| *d.get_persisted_mut_or_default::<bool>(uniform_id));
    ui.checkbox(&mut uniform, "Uniform");
    ui.data_mut(|d| *d.get_persisted_mut_or_insert_with(uniform_id, || false) = uniform);

    egui::Grid::new("scale").num_columns(2).show(ui, |ui| {
        ui.label("X");
        if ui.add(egui::DragValue::new(&mut t.scale_x).speed(0.01)).changed() {
            if uniform {
                t.scale_y = t.scale_x;
                t.scale_z = t.scale_x;
            }
            changed = true;
        }
        ui.end_row();
        if !uniform {
            ui.label("Y");
            changed |= ui.add(egui::DragValue::new(&mut t.scale_y).speed(0.01)).changed();
            ui.end_row();
            ui.label("Z");
            changed |= ui.add(egui::DragValue::new(&mut t.scale_z).speed(0.01)).changed();
            ui.end_row();
        }
    });

    if changed {
        // Apply live
        scene.components.insert(entity, t.clone());
        let new_json = serde_json::to_value(&t).expect("TransformComponent must be serializable");
        let cmd = ModifyComponent {
            entity,
            type_name: "TransformComponent".to_string(),
            old_value: old_json,
            new_value: new_json,
        };
        undo.push(Box::new(cmd), scene);
        *project_dirty = true;
    }
    changed
}

fn show_mesh(
    ui: &mut egui::Ui,
    entity: EntityId,
    scene: &Arc<Scene>,
    undo: &mut UndoStack,
    project_dirty: &mut bool,
    asset_browser: &AssetBrowserPanel,
) -> bool {
    let Some(mut m) = scene.components.get::<MeshComponent>(entity) else {
        return false;
    };
    let old_json = serde_json::to_value(&m).expect("MeshComponent must be serializable");
    let mut changed = false;

    // Helper: check if a dragged asset should be dropped onto a field.
    // Returns the stem (asset ID without extension) if it's the right category.
    let check_drop = |ui: &egui::Ui, resp: &egui::Response, accept_cat: AssetCategory| -> Option<String> {
        let drag = asset_browser.drag_payload.as_ref()?;
        let ext = drag.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if AssetCategory::from_extension(&ext) != accept_cat {
            return None;
        }
        if resp.hovered() {
            // Highlight the drop target
            ui.painter().rect_stroke(resp.rect, 2.0, egui::Stroke::new(2.0, egui::Color32::YELLOW), egui::StrokeKind::Middle);
            if ui.ctx().input(|i| i.pointer.any_released()) {
                let stem = drag
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                return Some(stem);
            }
        }
        None
    };

    egui::Grid::new("mesh").num_columns(2).show(ui, |ui| {
        ui.label("Mesh ID");
        let mesh_resp = ui.text_edit_singleline(&mut m.mesh_id);
        if let Some(stem) = check_drop(ui, &mesh_resp, AssetCategory::Mesh) {
            m.mesh_id = stem;
            changed = true;
        }
        changed |= mesh_resp.changed();
        ui.end_row();
        ui.label("Texture ID");
        let tex_resp = ui.text_edit_singleline(&mut m.texture_id);
        if let Some(stem) = check_drop(ui, &tex_resp, AssetCategory::Texture) {
            m.texture_id = stem;
            changed = true;
        }
        changed |= tex_resp.changed();
        ui.end_row();
        ui.label("Visible");
        changed |= ui.checkbox(&mut m.visible, "").changed();
        ui.end_row();
        ui.label("Shininess");
        changed |= ui
            .add(egui::Slider::new(&mut m.shininess, 0.0_f32..=128.0).text(""))
            .changed();
        ui.end_row();
        ui.label("Yaw Offset");
        changed |= ui.add(egui::DragValue::new(&mut m.yaw_offset).speed(0.1)).changed();
        ui.end_row();
    });

    if changed {
        scene.components.insert(entity, m.clone());
        let new_json = serde_json::to_value(&m).expect("MeshComponent must be serializable");
        let cmd = ModifyComponent {
            entity,
            type_name: "MeshComponent".to_string(),
            old_value: old_json,
            new_value: new_json,
        };
        undo.push(Box::new(cmd), scene);
        *project_dirty = true;
    }
    changed
}

fn show_tag(
    ui: &mut egui::Ui,
    entity: EntityId,
    scene: &Arc<Scene>,
    undo: &mut UndoStack,
    project_dirty: &mut bool,
) -> bool {
    let Some(mut tag) = scene.components.get::<TagComponent>(entity) else {
        return false;
    };
    let old_json = serde_json::to_value(&tag).expect("TagComponent must be serializable");
    let mut changed = false;

    let mut to_remove: Option<usize> = None;
    for (i, t) in tag.tags.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if ui.small_button("✕").clicked() {
                to_remove = Some(i);
            }
            changed |= ui.text_edit_singleline(t).changed();
        });
    }
    if let Some(idx) = to_remove {
        tag.tags.remove(idx);
        changed = true;
    }

    // New tag input
    let new_tag_id = ui.id().with("new_tag");
    let mut new_tag: String =
        ui.data_mut(|d| d.get_persisted_mut_or_default::<String>(new_tag_id).clone());
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut new_tag);
        if ui.button("Add").clicked() && !new_tag.is_empty() {
            tag.tags.push(new_tag.clone());
            new_tag.clear();
            changed = true;
        }
    });
    ui.data_mut(|d| *d.get_persisted_mut_or_insert_with(new_tag_id, || String::new()) = new_tag);

    if changed {
        scene.components.insert(entity, tag.clone());
        let new_json = serde_json::to_value(&tag).expect("TagComponent must be serializable");
        let cmd = ModifyComponent {
            entity,
            type_name: "TagComponent".to_string(),
            old_value: old_json,
            new_value: new_json,
        };
        undo.push(Box::new(cmd), scene);
        *project_dirty = true;
    }
    changed
}

fn show_rigid_body(
    ui: &mut egui::Ui,
    entity: EntityId,
    scene: &Arc<Scene>,
    undo: &mut UndoStack,
    project_dirty: &mut bool,
) -> bool {
    let Some(mut rb) = scene.components.get::<RigidBodyComponent>(entity) else {
        return false;
    };
    let old_json = serde_json::to_value(&rb).expect("RigidBodyComponent must be serializable");
    let mut changed = false;

    egui::Grid::new("rb").num_columns(2).show(ui, |ui| {
        ui.label("Body Type");
        egui::ComboBox::from_id_salt("rb_type")
            .selected_text(&rb.body_type)
            .show_ui(ui, |ui| {
                for opt in &["dynamic", "static", "kinematic"] {
                    if ui.selectable_value(&mut rb.body_type, opt.to_string(), *opt).changed() {
                        changed = true;
                    }
                }
            });
        ui.end_row();
        ui.label("Mass");
        changed |= ui.add(egui::DragValue::new(&mut rb.mass).speed(0.1)).changed();
        ui.end_row();
        ui.label("Gravity Factor");
        changed |= ui.add(egui::DragValue::new(&mut rb.gravity_factor).speed(0.01)).changed();
        ui.end_row();
    });

    if changed {
        scene.components.insert(entity, rb.clone());
        let new_json = serde_json::to_value(&rb).expect("RigidBodyComponent must be serializable");
        let cmd = ModifyComponent {
            entity,
            type_name: "RigidBodyComponent".to_string(),
            old_value: old_json,
            new_value: new_json,
        };
        undo.push(Box::new(cmd), scene);
        *project_dirty = true;
    }
    changed
}

fn show_collider(
    ui: &mut egui::Ui,
    entity: EntityId,
    scene: &Arc<Scene>,
    undo: &mut UndoStack,
    project_dirty: &mut bool,
) -> bool {
    let Some(mut col) = scene.components.get::<ColliderComponent>(entity) else {
        return false;
    };
    let old_json = serde_json::to_value(&col).expect("ColliderComponent must be serializable");
    let mut changed = false;

    egui::Grid::new("col").num_columns(2).show(ui, |ui| {
        ui.label("Shape");
        egui::ComboBox::from_id_salt("col_shape")
            .selected_text(&col.shape)
            .show_ui(ui, |ui| {
                for opt in &["box", "sphere", "capsule"] {
                    if ui.selectable_value(&mut col.shape, opt.to_string(), *opt).changed() {
                        changed = true;
                    }
                }
            });
        ui.end_row();
        ui.label("Size X");
        changed |= ui.add(egui::DragValue::new(&mut col.size[0]).speed(0.01)).changed();
        ui.end_row();
        ui.label("Size Y");
        changed |= ui.add(egui::DragValue::new(&mut col.size[1]).speed(0.01)).changed();
        ui.end_row();
        ui.label("Size Z");
        changed |= ui.add(egui::DragValue::new(&mut col.size[2]).speed(0.01)).changed();
        ui.end_row();
        ui.label("Is Trigger");
        changed |= ui.checkbox(&mut col.is_trigger, "").changed();
        ui.end_row();
    });

    if changed {
        scene.components.insert(entity, col.clone());
        let new_json = serde_json::to_value(&col).expect("ColliderComponent must be serializable");
        let cmd = ModifyComponent {
            entity,
            type_name: "ColliderComponent".to_string(),
            old_value: old_json,
            new_value: new_json,
        };
        undo.push(Box::new(cmd), scene);
        *project_dirty = true;
    }
    changed
}

fn show_billboard(
    ui: &mut egui::Ui,
    entity: EntityId,
    scene: &Arc<Scene>,
    undo: &mut UndoStack,
    project_dirty: &mut bool,
) -> bool {
    let Some(mut b) = scene.components.get::<BillboardComponent>(entity) else {
        return false;
    };
    let old_json = serde_json::to_value(&b).expect("BillboardComponent must be serializable");
    let mut changed = false;

    egui::Grid::new("bill").num_columns(2).show(ui, |ui| {
        ui.label("Asset ID");
        changed |= ui.text_edit_singleline(&mut b.asset_id).changed();
        ui.end_row();
        ui.label("Width");
        changed |= ui.add(egui::DragValue::new(&mut b.width).speed(0.01)).changed();
        ui.end_row();
        ui.label("Height");
        changed |= ui.add(egui::DragValue::new(&mut b.height).speed(0.01)).changed();
        ui.end_row();
        ui.label("UV Rect X");
        changed |= ui.add(egui::DragValue::new(&mut b.uv_rect[0]).speed(0.01)).changed();
        ui.end_row();
        ui.label("UV Rect Y");
        changed |= ui.add(egui::DragValue::new(&mut b.uv_rect[1]).speed(0.01)).changed();
        ui.end_row();
        ui.label("UV Rect W");
        changed |= ui.add(egui::DragValue::new(&mut b.uv_rect[2]).speed(0.01)).changed();
        ui.end_row();
        ui.label("UV Rect H");
        changed |= ui.add(egui::DragValue::new(&mut b.uv_rect[3]).speed(0.01)).changed();
        ui.end_row();
        ui.label("Alpha");
        changed |= ui
            .add(egui::Slider::new(&mut b.alpha, 0.0_f32..=1.0).text(""))
            .changed();
        ui.end_row();
    });

    if changed {
        scene.components.insert(entity, b.clone());
        let new_json = serde_json::to_value(&b).expect("BillboardComponent must be serializable");
        let cmd = ModifyComponent {
            entity,
            type_name: "BillboardComponent".to_string(),
            old_value: old_json,
            new_value: new_json,
        };
        undo.push(Box::new(cmd), scene);
        *project_dirty = true;
    }
    changed
}

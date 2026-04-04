use std::sync::Arc;

use rython_ecs::component::TagComponent;
use rython_ecs::{EntityId, Scene};

use crate::state::selection::SelectionState;
use crate::state::undo::{DespawnEntity, ReparentEntity, SpawnEntity, UndoStack};

/// Pending drag-and-drop reparent to process after the UI frame.
struct PendingReparent {
    child: EntityId,
    new_parent: Option<EntityId>,
}

#[derive(Default)]
pub struct SceneHierarchyPanel {
    drag_source: Option<EntityId>,
}

impl SceneHierarchyPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        scene: &Arc<Scene>,
        selection: &mut SelectionState,
        undo: &mut UndoStack,
        project_dirty: &mut bool,
    ) {
        ui.heading("Hierarchy");
        ui.separator();

        // Right-click on empty area → Add Entity
        let area_resp = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("hier_area"),
            egui::Sense::click(),
        );
        if area_resp.secondary_clicked() {
            ui.memory_mut(|m| m.open_popup(ui.id().with("hier_ctx_empty")));
        }
        egui::popup::popup_below_widget(
            ui,
            ui.id().with("hier_ctx_empty"),
            &area_resp,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                if ui.button("Add Entity").clicked() {
                    spawn_new_entity(scene, selection, undo);
                    *project_dirty = true;
                }
            },
        );

        // Collect root entities (no parent)
        let mut roots: Vec<EntityId> = scene
            .all_entities()
            .into_iter()
            .filter(|&e| scene.hierarchy.get_parent(e).is_none())
            .collect();
        roots.sort_by_key(|e| e.0);

        let mut pending: Option<PendingReparent> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for root in roots {
                self.show_entity_node(
                    ui,
                    root,
                    scene,
                    selection,
                    undo,
                    project_dirty,
                    &mut pending,
                );
            }
        });

        // Apply any reparent after frame
        if let Some(p) = pending {
            let old_parent = scene.hierarchy.get_parent(p.child);
            if old_parent != p.new_parent {
                let cmd = ReparentEntity {
                    entity: p.child,
                    old_parent,
                    new_parent: p.new_parent,
                };
                undo.push(Box::new(cmd), scene);
                *project_dirty = true;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_entity_node(
        &mut self,
        ui: &mut egui::Ui,
        entity: EntityId,
        scene: &Arc<Scene>,
        selection: &mut SelectionState,
        undo: &mut UndoStack,
        project_dirty: &mut bool,
        pending: &mut Option<PendingReparent>,
    ) {
        let label = entity_label(entity, scene);
        let is_selected = selection.selected_entity() == Some(entity);
        let mut children = scene.hierarchy.get_children(entity);
        let has_children = !children.is_empty();
        // Sort in-place once; no second clone needed.
        children.sort_unstable_by_key(|e| e.0);

        let id = ui.make_persistent_id(entity.0);

        let header = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            has_children,
        );

        let (_, header_response, _) = header
            .show_header(ui, |ui| {
                let r = ui.selectable_label(is_selected, &label);
                if r.clicked() {
                    selection.select_entity(entity);
                }
                r
            })
            .body(|ui| {
                for child in &children {
                    self.show_entity_node(
                        ui,
                        *child,
                        scene,
                        selection,
                        undo,
                        project_dirty,
                        pending,
                    );
                }
            });

        // Context menu
        header_response.inner.context_menu(|ui| {
            if ui.button("Add Child").clicked() {
                spawn_new_entity_with_parent(scene, selection, undo, Some(entity));
                *project_dirty = true;
                ui.close_menu();
            }
            if ui.button("Duplicate").clicked() {
                duplicate_entity(entity, scene, selection, undo);
                *project_dirty = true;
                ui.close_menu();
            }
            if ui.button("Delete").clicked() {
                let cmd = DespawnEntity::capture(entity, scene);
                undo.push(Box::new(cmd), scene);
                if selection.selected_entity() == Some(entity) {
                    selection.clear();
                }
                *project_dirty = true;
                ui.close_menu();
            }
        });

        // Drag source
        if header_response.inner.drag_started() {
            self.drag_source = Some(entity);
        }

        // Drop target — reparent onto this entity
        if header_response.inner.hovered() && ui.input(|i| i.pointer.any_released()) {
            if let Some(src) = self.drag_source.take() {
                if src != entity {
                    *pending = Some(PendingReparent {
                        child: src,
                        new_parent: Some(entity),
                    });
                }
            }
        }
    }
}

fn entity_label(entity: EntityId, scene: &Scene) -> String {
    if let Some(tag) = scene.components.get::<TagComponent>(entity) {
        if let Some(first) = tag.tags.first() {
            return first.clone();
        }
    }
    format!("Entity {}", entity.0)
}

fn spawn_new_entity(
    scene: &Arc<Scene>,
    selection: &mut SelectionState,
    undo: &mut UndoStack,
) -> EntityId {
    spawn_new_entity_with_parent(scene, selection, undo, None)
}

fn spawn_new_entity_with_parent(
    scene: &Arc<Scene>,
    selection: &mut SelectionState,
    undo: &mut UndoStack,
    parent: Option<EntityId>,
) -> EntityId {
    use rython_ecs::component::TransformComponent;

    let new_id = EntityId::next();
    let transform_json = serde_json::to_value(TransformComponent::default())
        .expect("TransformComponent must be serializable");
    let tag_json = serde_json::to_value(rython_ecs::component::TagComponent {
        tags: vec!["New Entity".to_string()],
    })
    .expect("TagComponent must be serializable");

    let cmd = SpawnEntity::new(
        new_id,
        vec![
            ("TransformComponent".to_string(), transform_json),
            ("TagComponent".to_string(), tag_json),
        ],
        parent,
    );
    undo.push(Box::new(cmd), scene);
    selection.select_entity(new_id);
    new_id
}

fn duplicate_entity(
    entity: EntityId,
    scene: &Arc<Scene>,
    selection: &mut SelectionState,
    undo: &mut UndoStack,
) {
    let new_id = EntityId::next();
    let comps = scene.components.snapshot_entity(entity);
    let parent = scene.hierarchy.get_parent(entity);
    let cmd = SpawnEntity::new(
        new_id,
        comps.into_iter().map(|(n, v)| (n.to_string(), v)).collect(),
        parent,
    );
    undo.push(Box::new(cmd), scene);
    selection.select_entity(new_id);
}

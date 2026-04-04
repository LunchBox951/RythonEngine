use rython_ecs::component::{
    BillboardComponent, ColliderComponent, MeshComponent, RigidBodyComponent, TagComponent,
    TransformComponent,
};
use rython_ecs::{EntityId, Scene};
use serde_json::Value;

// ── Command trait ─────────────────────────────────────────────────────────────

pub trait EditorCommand: Send + Sync {
    fn execute(&self, scene: &Scene);
    fn undo(&self, scene: &Scene);
    fn description(&self) -> &str;
}

// ── UndoStack ─────────────────────────────────────────────────────────────────

pub struct UndoStack {
    history: Vec<Box<dyn EditorCommand>>,
    /// Index past the last executed command.
    position: usize,
    max_history: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self { history: Vec::new(), position: 0, max_history: 200 }
    }
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute `cmd`, append to history, truncate any redo tail.
    pub fn push(&mut self, cmd: Box<dyn EditorCommand>, scene: &Scene) {
        cmd.execute(scene);
        // Truncate redo tail
        self.history.truncate(self.position);
        self.history.push(cmd);
        self.position += 1;
        // Trim oldest entries if over cap
        if self.history.len() > self.max_history {
            let drop_count = self.history.len() - self.max_history;
            self.history.drain(..drop_count);
            self.position = self.position.saturating_sub(drop_count);
        }
    }

    pub fn undo(&mut self, scene: &Scene) {
        if self.can_undo() {
            self.position -= 1;
            self.history[self.position].undo(scene);
        }
    }

    pub fn redo(&mut self, scene: &Scene) {
        if self.can_redo() {
            self.history[self.position].execute(scene);
            self.position += 1;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.position > 0
    }

    pub fn can_redo(&self) -> bool {
        self.position < self.history.len()
    }

    pub fn clear(&mut self) {
        self.history.clear();
        self.position = 0;
    }

    /// Append `cmd` to history WITHOUT executing it.
    ///
    /// Used when the effect has already been applied live (e.g. gizmo drag) and
    /// we only need the undo record.
    pub fn push_no_execute(&mut self, cmd: Box<dyn EditorCommand>) {
        self.history.truncate(self.position);
        self.history.push(cmd);
        self.position += 1;
        if self.history.len() > self.max_history {
            let drop_count = self.history.len() - self.max_history;
            self.history.drain(..drop_count);
            self.position = self.position.saturating_sub(drop_count);
        }
    }
}

// ── Helper: deserialize and re-insert a component by type name ────────────────

fn apply_component_json(scene: &Scene, entity: EntityId, type_name: &str, data: &Value) {
    scene.load_component(entity, type_name, data);
}

fn default_component_json(type_name: &str) -> Value {
    match type_name {
        "TransformComponent" => {
            serde_json::to_value(TransformComponent::default())
                .expect("TransformComponent must be serializable")
        }
        "MeshComponent" => {
            serde_json::to_value(MeshComponent::default())
                .expect("MeshComponent must be serializable")
        }
        "TagComponent" => {
            serde_json::to_value(TagComponent::default())
                .expect("TagComponent must be serializable")
        }
        "RigidBodyComponent" => {
            serde_json::to_value(RigidBodyComponent::default())
                .expect("RigidBodyComponent must be serializable")
        }
        "ColliderComponent" => {
            serde_json::to_value(ColliderComponent::default())
                .expect("ColliderComponent must be serializable")
        }
        "BillboardComponent" => {
            serde_json::to_value(BillboardComponent::default())
                .expect("BillboardComponent must be serializable")
        }
        _ => Value::Null,
    }
}

fn remove_component_by_type_name(scene: &Scene, entity: EntityId, type_name: &str) {
    match type_name {
        "TransformComponent" => { scene.components.remove::<TransformComponent>(entity); }
        "MeshComponent" => { scene.components.remove::<MeshComponent>(entity); }
        "TagComponent" => { scene.components.remove::<TagComponent>(entity); }
        "RigidBodyComponent" => { scene.components.remove::<RigidBodyComponent>(entity); }
        "ColliderComponent" => { scene.components.remove::<ColliderComponent>(entity); }
        "BillboardComponent" => { scene.components.remove::<BillboardComponent>(entity); }
        _ => {}
    }
}

// ── Component snapshot for despawn/redo ──────────────────────────────────────

#[derive(Clone)]
pub struct EntitySnapshot {
    pub entity: EntityId,
    pub parent: Option<EntityId>,
    pub components: Vec<(String, Value)>,
    pub children: Vec<EntitySnapshot>,
}

impl EntitySnapshot {
    pub fn capture(entity: EntityId, scene: &Scene) -> Self {
        let comps = scene.components.snapshot_entity(entity);
        let parent = scene.hierarchy.get_parent(entity);
        let children_ids = scene.hierarchy.get_children(entity);
        let children = children_ids
            .into_iter()
            .map(|c| EntitySnapshot::capture(c, scene))
            .collect();
        EntitySnapshot {
            entity,
            parent,
            components: comps.into_iter().map(|(n, v)| (n.to_string(), v)).collect(),
            children,
        }
    }

    /// Re-create this entity in the scene, preserving ID and hierarchy.
    pub fn restore(&self, scene: &Scene) {
        // Insert entity with its original ID (no new components yet)
        scene.spawn_with_id(self.entity, vec![]);
        for (type_name, data) in &self.components {
            apply_component_json(scene, self.entity, type_name, data);
        }
        if let Some(parent) = self.parent {
            scene.hierarchy.set_parent(self.entity, parent);
        }
        for child in &self.children {
            child.restore(scene);
        }
    }
}

// ── SpawnEntity ───────────────────────────────────────────────────────────────

pub struct SpawnEntity {
    pub entity: EntityId,
    pub components: Vec<(String, Value)>,
    pub parent: Option<EntityId>,
}

impl SpawnEntity {
    pub fn new(entity: EntityId, components: Vec<(String, Value)>, parent: Option<EntityId>) -> Self {
        Self { entity, components, parent }
    }
}

impl EditorCommand for SpawnEntity {
    fn execute(&self, scene: &Scene) {
        scene.spawn_with_id(self.entity, vec![]);
        for (type_name, data) in &self.components {
            apply_component_json(scene, self.entity, type_name, data);
        }
        if let Some(parent) = self.parent {
            scene.hierarchy.set_parent(self.entity, parent);
        }
    }

    fn undo(&self, scene: &Scene) {
        scene.despawn_immediate(self.entity);
    }

    fn description(&self) -> &str {
        "Spawn Entity"
    }
}

// ── DespawnEntity ─────────────────────────────────────────────────────────────

pub struct DespawnEntity {
    pub snapshot: EntitySnapshot,
}

impl DespawnEntity {
    pub fn capture(entity: EntityId, scene: &Scene) -> Self {
        Self { snapshot: EntitySnapshot::capture(entity, scene) }
    }
}

impl EditorCommand for DespawnEntity {
    fn execute(&self, scene: &Scene) {
        despawn_recursive(self.snapshot.entity, scene);
    }

    fn undo(&self, scene: &Scene) {
        self.snapshot.restore(scene);
    }

    fn description(&self) -> &str {
        "Delete Entity"
    }
}

// ── BatchCommand ──────────────────────────────────────────────────────────────

/// Wraps multiple `EditorCommand`s into a single undo step.
///
/// `execute()` runs all commands in order; `undo()` runs them in reverse.
pub struct BatchCommand {
    commands: Vec<Box<dyn EditorCommand>>,
    description: String,
}

impl BatchCommand {
    pub fn new(commands: Vec<Box<dyn EditorCommand>>, description: impl Into<String>) -> Self {
        Self { commands, description: description.into() }
    }
}

impl EditorCommand for BatchCommand {
    fn execute(&self, scene: &Scene) {
        for cmd in &self.commands {
            cmd.execute(scene);
        }
    }

    fn undo(&self, scene: &Scene) {
        for cmd in self.commands.iter().rev() {
            cmd.undo(scene);
        }
    }

    fn description(&self) -> &str {
        &self.description
    }
}

fn despawn_recursive(entity: EntityId, scene: &Scene) {
    let children = scene.hierarchy.get_children(entity);
    for child in children {
        despawn_recursive(child, scene);
    }
    scene.despawn_immediate(entity);
}

// ── ModifyComponent ───────────────────────────────────────────────────────────

pub struct ModifyComponent {
    pub entity: EntityId,
    pub type_name: String,
    pub old_value: Value,
    pub new_value: Value,
}

impl EditorCommand for ModifyComponent {
    fn execute(&self, scene: &Scene) {
        apply_component_json(scene, self.entity, &self.type_name, &self.new_value);
    }

    fn undo(&self, scene: &Scene) {
        apply_component_json(scene, self.entity, &self.type_name, &self.old_value);
    }

    fn description(&self) -> &str {
        "Modify Component"
    }
}

// ── ReparentEntity ────────────────────────────────────────────────────────────

pub struct ReparentEntity {
    pub entity: EntityId,
    pub old_parent: Option<EntityId>,
    pub new_parent: Option<EntityId>,
}

impl EditorCommand for ReparentEntity {
    fn execute(&self, scene: &Scene) {
        match self.new_parent {
            Some(p) => scene.hierarchy.set_parent(self.entity, p),
            None => scene.hierarchy.clear_parent(self.entity),
        }
    }

    fn undo(&self, scene: &Scene) {
        match self.old_parent {
            Some(p) => scene.hierarchy.set_parent(self.entity, p),
            None => scene.hierarchy.clear_parent(self.entity),
        }
    }

    fn description(&self) -> &str {
        "Reparent Entity"
    }
}

// ── AttachComponent ───────────────────────────────────────────────────────────

pub struct AttachComponent {
    pub entity: EntityId,
    pub type_name: String,
    pub default_json: Value,
}

impl AttachComponent {
    pub fn new(entity: EntityId, type_name: &str) -> Self {
        let default_json = default_component_json(type_name);
        Self { entity, type_name: type_name.to_string(), default_json }
    }
}

impl EditorCommand for AttachComponent {
    fn execute(&self, scene: &Scene) {
        apply_component_json(scene, self.entity, &self.type_name, &self.default_json);
    }

    fn undo(&self, scene: &Scene) {
        remove_component_by_type_name(scene, self.entity, &self.type_name);
    }

    fn description(&self) -> &str {
        "Attach Component"
    }
}

// ── DetachComponent ───────────────────────────────────────────────────────────

pub struct DetachComponent {
    pub entity: EntityId,
    pub type_name: String,
    pub component_json: Value,
}

impl DetachComponent {
    pub fn capture(entity: EntityId, type_name: &str, scene: &Scene) -> Self {
        // Serialize only the targeted component instead of snapshot_entity
        // which serializes ALL components for the entity.
        let component_json = snapshot_single_component(entity, type_name, scene);
        Self { entity, type_name: type_name.to_string(), component_json }
    }
}

/// Serialize a single component by type name, avoiding the cost of
/// `snapshot_entity` which serializes every component on the entity.
fn snapshot_single_component(entity: EntityId, type_name: &str, scene: &Scene) -> Value {
    match type_name {
        "TransformComponent" => scene.components.get::<TransformComponent>(entity)
            .map(|c| serde_json::to_value(&c).expect("TransformComponent must be serializable"))
            .unwrap_or(Value::Null),
        "MeshComponent" => scene.components.get::<MeshComponent>(entity)
            .map(|c| serde_json::to_value(&c).expect("MeshComponent must be serializable"))
            .unwrap_or(Value::Null),
        "TagComponent" => scene.components.get::<TagComponent>(entity)
            .map(|c| serde_json::to_value(&c).expect("TagComponent must be serializable"))
            .unwrap_or(Value::Null),
        "RigidBodyComponent" => scene.components.get::<RigidBodyComponent>(entity)
            .map(|c| serde_json::to_value(&c).expect("RigidBodyComponent must be serializable"))
            .unwrap_or(Value::Null),
        "ColliderComponent" => scene.components.get::<ColliderComponent>(entity)
            .map(|c| serde_json::to_value(&c).expect("ColliderComponent must be serializable"))
            .unwrap_or(Value::Null),
        "BillboardComponent" => scene.components.get::<BillboardComponent>(entity)
            .map(|c| serde_json::to_value(&c).expect("BillboardComponent must be serializable"))
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

impl EditorCommand for DetachComponent {
    fn execute(&self, scene: &Scene) {
        remove_component_by_type_name(scene, self.entity, &self.type_name);
    }

    fn undo(&self, scene: &Scene) {
        apply_component_json(scene, self.entity, &self.type_name, &self.component_json);
    }

    fn description(&self) -> &str {
        "Remove Component"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ── Mock command ──────────────────────────────────────────────────────────

    #[derive(Default)]
    struct Counts {
        execute: u32,
        undo: u32,
    }

    struct MockCmd {
        counts: Arc<Mutex<Counts>>,
        label: &'static str,
    }

    impl MockCmd {
        fn new(label: &'static str) -> (Self, Arc<Mutex<Counts>>) {
            let counts = Arc::new(Mutex::new(Counts::default()));
            (Self { counts: counts.clone(), label }, counts)
        }
    }

    impl EditorCommand for MockCmd {
        fn execute(&self, _scene: &Scene) {
            self.counts.lock().unwrap().execute += 1;
        }

        fn undo(&self, _scene: &Scene) {
            self.counts.lock().unwrap().undo += 1;
        }

        fn description(&self) -> &str {
            self.label
        }
    }

    // ── UndoStack ─────────────────────────────────────────────────────────────

    #[test]
    fn empty_stack_cannot_undo_or_redo() {
        let stack = UndoStack::new();
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn push_executes_command_immediately() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, counts) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        assert_eq!(counts.lock().unwrap().execute, 1);
    }

    #[test]
    fn push_enables_undo_not_redo() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, _) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn undo_calls_command_undo() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, counts) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        stack.undo(&scene);
        assert_eq!(counts.lock().unwrap().undo, 1);
    }

    #[test]
    fn undo_disables_undo_and_enables_redo() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, _) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        stack.undo(&scene);
        assert!(!stack.can_undo());
        assert!(stack.can_redo());
    }

    #[test]
    fn redo_calls_execute_again() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, counts) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        stack.undo(&scene);
        stack.redo(&scene);
        assert_eq!(counts.lock().unwrap().execute, 2);
    }

    #[test]
    fn redo_enables_undo_and_disables_redo() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, _) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        stack.undo(&scene);
        stack.redo(&scene);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn push_after_undo_truncates_redo_tail() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd_a, _) = MockCmd::new("a");
        let (cmd_b, _) = MockCmd::new("b");
        let (cmd_c, _) = MockCmd::new("c");
        stack.push(Box::new(cmd_a), &scene);
        stack.push(Box::new(cmd_b), &scene);
        stack.undo(&scene);
        assert!(stack.can_redo());
        stack.push(Box::new(cmd_c), &scene);
        assert!(!stack.can_redo(), "new push must clear redo tail");
    }

    #[test]
    fn clear_empties_entire_stack() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, _) = MockCmd::new("a");
        stack.push(Box::new(cmd), &scene);
        stack.clear();
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn push_no_execute_does_not_call_execute() {
        let mut stack = UndoStack::new();
        let (cmd, counts) = MockCmd::new("a");
        stack.push_no_execute(Box::new(cmd));
        assert_eq!(counts.lock().unwrap().execute, 0, "push_no_execute must not call execute");
    }

    #[test]
    fn push_no_execute_enables_undo() {
        let mut stack = UndoStack::new();
        stack.push_no_execute(Box::new(MockCmd::new("a").0));
        assert!(stack.can_undo());
    }

    #[test]
    fn push_no_execute_undo_calls_undo() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd, counts) = MockCmd::new("a");
        stack.push_no_execute(Box::new(cmd));
        stack.undo(&scene);
        assert_eq!(counts.lock().unwrap().undo, 1);
    }

    #[test]
    fn push_no_execute_truncates_redo_tail() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        let (cmd_a, _) = MockCmd::new("a");
        let (cmd_b, _) = MockCmd::new("b");
        stack.push(Box::new(cmd_a), &scene);
        stack.undo(&scene);
        assert!(stack.can_redo());
        stack.push_no_execute(Box::new(cmd_b));
        assert!(!stack.can_redo(), "push_no_execute must clear redo tail");
    }

    #[test]
    fn max_history_caps_at_200() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        for _ in 0..250 {
            let (cmd, _) = MockCmd::new("x");
            stack.push(Box::new(cmd), &scene);
        }
        let mut undo_count = 0u32;
        while stack.can_undo() {
            stack.undo(&scene);
            undo_count += 1;
            // Safety guard against runaway loop
            assert!(undo_count <= 200, "undo count exceeded max_history");
        }
        assert_eq!(undo_count, 200);
    }

    #[test]
    fn undo_noop_when_stack_is_empty() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        // Must not panic
        stack.undo(&scene);
        stack.undo(&scene);
        assert!(!stack.can_undo());
    }

    #[test]
    fn redo_noop_when_nothing_to_redo() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        // Must not panic
        stack.redo(&scene);
        assert!(!stack.can_redo());
    }

    #[test]
    fn multiple_push_undo_redo_cycle() {
        let mut stack = UndoStack::new();
        let scene = Scene::new();
        for label in ["a", "b", "c"] {
            let (cmd, _) = MockCmd::new(label);
            stack.push(Box::new(cmd), &scene);
        }
        // Undo all three
        stack.undo(&scene);
        stack.undo(&scene);
        stack.undo(&scene);
        assert!(!stack.can_undo());
        assert!(stack.can_redo());
        // Redo all three
        stack.redo(&scene);
        stack.redo(&scene);
        stack.redo(&scene);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }
}

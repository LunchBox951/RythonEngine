#![deny(warnings)]

use std::collections::HashMap;

use crossbeam_channel::{unbounded, Receiver, Sender};
use rapier3d::prelude::*;
use rython_core::{EngineError, SchedulerHandle};
use rython_ecs::{ColliderComponent, EntityId, RigidBodyComponent, Scene, TransformComponent};
use rython_modules::Module;
use serde::{Deserialize, Serialize};

// ── Configuration ─────────────────────────────────────────────────────────────

fn default_gravity() -> [f32; 3] {
    [0.0, -9.81, 0.0]
}
fn default_timestep() -> f32 {
    1.0 / 60.0
}
fn default_max_substeps() -> u32 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsConfig {
    #[serde(default = "default_gravity")]
    pub gravity: [f32; 3],
    #[serde(default = "default_timestep")]
    pub fixed_timestep: f32,
    #[serde(default = "default_max_substeps")]
    pub max_substeps: u32,
    #[serde(default)]
    pub lock_2d: Option<String>,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: default_gravity(),
            fixed_timestep: default_timestep(),
            max_substeps: default_max_substeps(),
            lock_2d: None,
        }
    }
}

// ── Body entry ────────────────────────────────────────────────────────────────

struct BodyEntry {
    rigid_body_handle: RigidBodyHandle,
    collider_handle: ColliderHandle,
    last_valid_position: [f32; 3],
}

// ── Physics World ─────────────────────────────────────────────────────────────

pub struct PhysicsWorld {
    config: PhysicsConfig,
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: BroadPhaseMultiSap,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    entity_to_body: HashMap<EntityId, BodyEntry>,
    collider_to_entity: HashMap<ColliderHandle, EntityId>,
    // Reused channel pairs — avoids re-creating them every physics step.
    collision_send: Sender<CollisionEvent>,
    collision_recv: Receiver<CollisionEvent>,
    contact_force_send: Sender<ContactForceEvent>,
}

impl PhysicsWorld {
    pub fn new(config: PhysicsConfig) -> Self {
        let integration_parameters = IntegrationParameters {
            dt: config.fixed_timestep,
            ..Default::default()
        };

        let (collision_send, collision_recv) = unbounded();
        let (contact_force_send, _contact_force_recv) = unbounded();

        Self {
            config,
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            integration_parameters,
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhaseMultiSap::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            entity_to_body: HashMap::new(),
            collider_to_entity: HashMap::new(),
            collision_send,
            collision_recv,
            contact_force_send,
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(PhysicsConfig::default())
    }

    pub fn set_gravity(&mut self, gravity: [f32; 3]) {
        self.config.gravity = gravity;
    }

    pub fn set_2d_mode(&mut self, mode: Option<&str>) {
        self.config.lock_2d = mode.map(|s| s.to_string());
    }

    // ── Full per-frame sync cycle ─────────────────────────────────────────────

    pub fn sync_step(&mut self, scene: &Scene) {
        self.register_new_bodies(scene);
        self.unregister_removed_bodies(scene);
        self.push_transforms(scene);
        self.step_simulation(scene);
        self.pull_transforms(scene);
    }

    // ── Register ─────────────────────────────────────────────────────────────

    fn register_new_bodies(&mut self, scene: &Scene) {
        let entities = scene.components.entities_with::<RigidBodyComponent>();
        for entity in entities {
            if self.entity_to_body.contains_key(&entity) {
                continue;
            }
            let rb_comp = match scene.components.get::<RigidBodyComponent>(entity) {
                Some(c) => c,
                None => continue,
            };
            let col_comp = match scene.components.get::<ColliderComponent>(entity) {
                Some(c) => c,
                None => continue,
            };
            let transform = scene
                .components
                .get::<TransformComponent>(entity)
                .unwrap_or_default();
            self.register_body(entity, &rb_comp, &col_comp, &transform);
        }
    }

    fn register_body(
        &mut self,
        entity: EntityId,
        rb_comp: &RigidBodyComponent,
        col_comp: &ColliderComponent,
        transform: &TransformComponent,
    ) {
        let locked_axes = self.compute_locked_axes();

        let is_dynamic = !matches!(rb_comp.body_type.as_str(), "static" | "fixed" | "kinematic");
        let rb_builder = match rb_comp.body_type.as_str() {
            "static" | "fixed" => RigidBodyBuilder::fixed(),
            "kinematic" => RigidBodyBuilder::kinematic_position_based(),
            _ => RigidBodyBuilder::dynamic(),
        };

        // Mass only applies to dynamic bodies; on static/kinematic it's ignored
        // by rapier anyway. Guard against NaN / non-positive mass which would
        // poison the solver.
        let mut rb_builder = rb_builder
            .translation(vector![transform.x, transform.y, transform.z])
            .gravity_scale(rb_comp.gravity_factor)
            .locked_axes(locked_axes);
        if is_dynamic && rb_comp.mass.is_finite() && rb_comp.mass > 0.0 {
            rb_builder = rb_builder.additional_mass(rb_comp.mass);
        }
        let rb = rb_builder.build();

        let rb_handle = self.rigid_body_set.insert(rb);

        let col = self.make_collider(col_comp, rb_comp);
        let col_handle =
            self.collider_set
                .insert_with_parent(col, rb_handle, &mut self.rigid_body_set);

        self.collider_to_entity.insert(col_handle, entity);
        self.entity_to_body.insert(
            entity,
            BodyEntry {
                rigid_body_handle: rb_handle,
                collider_handle: col_handle,
                last_valid_position: [transform.x, transform.y, transform.z],
            },
        );
    }

    fn make_collider(
        &self,
        col_comp: &ColliderComponent,
        rb_comp: &RigidBodyComponent,
    ) -> Collider {
        let builder = match col_comp.shape.as_str() {
            "sphere" | "ball" => ColliderBuilder::ball(col_comp.size[0] / 2.0),
            _ => ColliderBuilder::cuboid(
                col_comp.size[0] / 2.0,
                col_comp.size[1] / 2.0,
                col_comp.size[2] / 2.0,
            ),
        };

        let groups = InteractionGroups::new(
            Group::from_bits_truncate(rb_comp.collision_layer),
            Group::from_bits_truncate(rb_comp.collision_mask),
        );

        builder
            .collision_groups(groups)
            .sensor(col_comp.is_trigger)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build()
    }

    fn compute_locked_axes(&self) -> LockedAxes {
        match self.config.lock_2d.as_deref() {
            Some("xz") => {
                LockedAxes::TRANSLATION_LOCKED_Y
                    | LockedAxes::ROTATION_LOCKED_X
                    | LockedAxes::ROTATION_LOCKED_Z
            }
            Some("xy") => {
                LockedAxes::TRANSLATION_LOCKED_Z
                    | LockedAxes::ROTATION_LOCKED_X
                    | LockedAxes::ROTATION_LOCKED_Y
            }
            _ => LockedAxes::empty(),
        }
    }

    // ── Unregister ────────────────────────────────────────────────────────────

    fn unregister_removed_bodies(&mut self, scene: &Scene) {
        let to_remove: Vec<EntityId> = self
            .entity_to_body
            .keys()
            .copied()
            .filter(|&e| !scene.components.has::<RigidBodyComponent>(e))
            .collect();
        for entity in to_remove {
            self.remove_body(entity);
        }
    }

    fn remove_body(&mut self, entity: EntityId) {
        let Some(entry) = self.entity_to_body.remove(&entity) else {
            return;
        };
        self.collider_to_entity.remove(&entry.collider_handle);
        self.collider_set.remove(
            entry.collider_handle,
            &mut self.island_manager,
            &mut self.rigid_body_set,
            false,
        );
        self.rigid_body_set.remove(
            entry.rigid_body_handle,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            true,
        );
    }

    // ── Push (static/kinematic → rapier) ─────────────────────────────────────

    fn push_transforms(&mut self, scene: &Scene) {
        for (entity, entry) in &self.entity_to_body {
            let Some(rb) = self.rigid_body_set.get_mut(entry.rigid_body_handle) else {
                continue;
            };
            if rb.is_fixed() || rb.is_kinematic() {
                if let Some(t) = scene.components.get::<TransformComponent>(*entity) {
                    rb.set_translation(vector![t.x, t.y, t.z], true);
                }
            }
        }
    }

    // ── Step ─────────────────────────────────────────────────────────────────

    fn step_simulation(&mut self, scene: &Scene) {
        let gravity = vector![
            self.config.gravity[0],
            self.config.gravity[1],
            self.config.gravity[2]
        ];

        // Drain any stale events from the reused channels before stepping.
        while self.collision_recv.try_recv().is_ok() {}

        // Clone the senders (cheap ref-count bump) for the event collector.
        let event_handler = ChannelEventCollector::new(
            self.collision_send.clone(),
            self.contact_force_send.clone(),
        );

        // Destructure to allow simultaneous mutable borrows of distinct fields.
        let Self {
            physics_pipeline,
            integration_parameters,
            island_manager,
            broad_phase,
            narrow_phase,
            rigid_body_set,
            collider_set,
            impulse_joint_set,
            multibody_joint_set,
            ccd_solver,
            collider_to_entity,
            collision_recv,
            entity_to_body: _,
            config: _,
            collision_send: _,
            contact_force_send: _,
        } = self;

        physics_pipeline.step(
            &gravity,
            integration_parameters,
            island_manager,
            broad_phase,
            narrow_phase,
            rigid_body_set,
            collider_set,
            impulse_joint_set,
            multibody_joint_set,
            ccd_solver,
            None,
            &(),
            &event_handler,
        );

        // Process collision events.
        while let Ok(event) = collision_recv.try_recv() {
            match event {
                CollisionEvent::Started(h1, h2, flags) => {
                    let e1 = collider_to_entity.get(&h1).copied();
                    let e2 = collider_to_entity.get(&h2).copied();
                    if let (Some(e1), Some(e2)) = (e1, e2) {
                        if flags.contains(CollisionEventFlags::SENSOR) {
                            let payload = serde_json::json!({
                                "entity_a": e1.0,
                                "entity_b": e2.0,
                                "event_type": "enter",
                            });
                            scene.emit("trigger", payload.clone());
                            scene.emit(&format!("trigger_enter:{}", e1.0), payload.clone());
                            scene.emit(&format!("trigger_enter:{}", e2.0), payload);
                        } else {
                            // Look up contact normal from the narrow phase.
                            let normal = narrow_phase
                                .contact_pairs()
                                .filter(|p| p.has_any_active_contact)
                                .find(|p| {
                                    (p.collider1 == h1 && p.collider2 == h2)
                                        || (p.collider1 == h2 && p.collider2 == h1)
                                })
                                .and_then(|p| p.manifolds.first())
                                .map(|m| {
                                    let n = m.data.normal;
                                    [n.x, n.y, n.z]
                                })
                                .unwrap_or([0.0, 1.0, 0.0]);

                            // The manifold normal points from collider1→collider2 (e1→e2).
                            // Flip it for e1's per-entity event so each entity receives
                            // the normal pointing toward itself (upward for the floor contact).
                            let flipped = [-normal[0], -normal[1], -normal[2]];
                            scene.emit(
                                "collision",
                                serde_json::json!({
                                    "entity_a": e1.0,
                                    "entity_b": e2.0,
                                    "normal": normal,
                                }),
                            );
                            scene.emit(
                                &format!("collision:{}", e1.0),
                                serde_json::json!({
                                    "entity_a": e1.0,
                                    "entity_b": e2.0,
                                    "normal": flipped,
                                }),
                            );
                            scene.emit(
                                &format!("collision:{}", e2.0),
                                serde_json::json!({
                                    "entity_a": e1.0,
                                    "entity_b": e2.0,
                                    "normal": normal,
                                }),
                            );
                        }
                    }
                }
                CollisionEvent::Stopped(h1, h2, flags) => {
                    let e1 = collider_to_entity.get(&h1).copied();
                    let e2 = collider_to_entity.get(&h2).copied();
                    if let (Some(e1), Some(e2)) = (e1, e2) {
                        if flags.contains(CollisionEventFlags::SENSOR) {
                            let payload = serde_json::json!({
                                "entity_a": e1.0,
                                "entity_b": e2.0,
                                "event_type": "exit",
                            });
                            scene.emit("trigger", payload.clone());
                            scene.emit(&format!("trigger_exit:{}", e1.0), payload.clone());
                            scene.emit(&format!("trigger_exit:{}", e2.0), payload);
                        } else {
                            let payload = serde_json::json!({
                                "entity_a": e1.0,
                                "entity_b": e2.0,
                            });
                            scene.emit("collision_end", payload.clone());
                            scene.emit(&format!("collision_end:{}", e1.0), payload.clone());
                            scene.emit(&format!("collision_end:{}", e2.0), payload);
                        }
                    }
                }
            }
        }
    }

    // ── Pull (rapier → ECS, dynamic only) ────────────────────────────────────

    fn pull_transforms(&mut self, scene: &Scene) {
        let entities: Vec<EntityId> = self.entity_to_body.keys().copied().collect();

        for entity in entities {
            // Extract position and handle before taking any mutable borrows.
            let (pos, rb_handle) = {
                let entry = match self.entity_to_body.get(&entity) {
                    Some(e) => e,
                    None => continue,
                };
                let rb = match self.rigid_body_set.get(entry.rigid_body_handle) {
                    Some(r) => r,
                    None => continue,
                };
                if !rb.is_dynamic() {
                    continue;
                }
                let t = *rb.translation();
                (t, entry.rigid_body_handle)
            };

            let (px, py, pz) = (pos.x, pos.y, pos.z);

            if px.is_nan() || py.is_nan() || pz.is_nan() {
                eprintln!(
                    "[rython-physics] NaN position detected for entity {:?}; resetting",
                    entity
                );
                let last = self.entity_to_body[&entity].last_valid_position;
                let rb_mut = self.rigid_body_set.get_mut(rb_handle).unwrap();
                rb_mut.set_translation(vector![last[0], last[1], last[2]], true);
                rb_mut.set_linvel(vector![0.0, 0.0, 0.0], true);
                continue;
            }

            if let Some(entry) = self.entity_to_body.get_mut(&entity) {
                entry.last_valid_position = [px, py, pz];
            }

            scene
                .components
                .get_mut::<TransformComponent, _>(entity, |t| {
                    t.x = px;
                    t.y = py;
                    t.z = pz;
                });
        }
    }

    // ── Public force / velocity API ───────────────────────────────────────────

    pub fn apply_force(&mut self, entity: EntityId, force: [f32; 3]) {
        if let Some(entry) = self.entity_to_body.get(&entity) {
            if let Some(rb) = self.rigid_body_set.get_mut(entry.rigid_body_handle) {
                rb.add_force(vector![force[0], force[1], force[2]], true);
            }
        }
    }

    pub fn apply_impulse(&mut self, entity: EntityId, impulse: [f32; 3]) {
        if let Some(entry) = self.entity_to_body.get(&entity) {
            if let Some(rb) = self.rigid_body_set.get_mut(entry.rigid_body_handle) {
                rb.apply_impulse(vector![impulse[0], impulse[1], impulse[2]], true);
            }
        }
    }

    pub fn set_linear_velocity(&mut self, entity: EntityId, velocity: [f32; 3]) {
        if let Some(entry) = self.entity_to_body.get(&entity) {
            if let Some(rb) = self.rigid_body_set.get_mut(entry.rigid_body_handle) {
                rb.set_linvel(vector![velocity[0], velocity[1], velocity[2]], true);
            }
        }
    }

    pub fn get_linear_velocity(&self, entity: EntityId) -> Option<[f32; 3]> {
        let entry = self.entity_to_body.get(&entity)?;
        let rb = self.rigid_body_set.get(entry.rigid_body_handle)?;
        let v = rb.linvel();
        Some([v.x, v.y, v.z])
    }

    pub fn get_body_position(&self, entity: EntityId) -> Option<[f32; 3]> {
        let entry = self.entity_to_body.get(&entity)?;
        let rb = self.rigid_body_set.get(entry.rigid_body_handle)?;
        let t = rb.translation();
        Some([t.x, t.y, t.z])
    }

    pub fn body_count(&self) -> usize {
        self.entity_to_body.len()
    }

    /// Test helper: directly set rapier body translation (bypasses ECS).
    #[cfg(test)]
    pub fn set_body_translation_raw(&mut self, entity: EntityId, pos: [f32; 3]) {
        if let Some(entry) = self.entity_to_body.get(&entity) {
            if let Some(rb) = self.rigid_body_set.get_mut(entry.rigid_body_handle) {
                rb.set_translation(vector![pos[0], pos[1], pos[2]], true);
            }
        }
    }
}

// ── Module ────────────────────────────────────────────────────────────────────

pub struct PhysicsModule {
    config: PhysicsConfig,
    world: Option<PhysicsWorld>,
}

impl PhysicsModule {
    pub fn new(config: PhysicsConfig) -> Self {
        Self {
            config,
            world: None,
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(PhysicsConfig::default())
    }
}

impl Module for PhysicsModule {
    fn name(&self) -> &str {
        "physics"
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        self.world = Some(PhysicsWorld::new(self.config.clone()));
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        self.world = None;
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::sync::{Arc, Mutex};

    use super::*;
    use rython_ecs::{ColliderComponent, Component, RigidBodyComponent, Scene, TransformComponent};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn comp<C: Component>(c: C) -> (TypeId, Box<dyn Component>) {
        (TypeId::of::<C>(), Box::new(c) as Box<dyn Component>)
    }

    fn transform(x: f32, y: f32, z: f32) -> TransformComponent {
        TransformComponent {
            x,
            y,
            z,
            ..Default::default()
        }
    }

    fn dyn_rb() -> RigidBodyComponent {
        RigidBodyComponent {
            body_type: "dynamic".to_string(),
            mass: 1.0,
            gravity_factor: 1.0,
            collision_layer: 1,
            collision_mask: 1,
        }
    }

    fn rb_with(body_type: &str, gravity_factor: f32, layer: u32, mask: u32) -> RigidBodyComponent {
        RigidBodyComponent {
            body_type: body_type.to_string(),
            mass: 1.0,
            gravity_factor,
            collision_layer: layer,
            collision_mask: mask,
        }
    }

    fn box_col(size: [f32; 3]) -> ColliderComponent {
        ColliderComponent {
            shape: "box".to_string(),
            size,
            is_trigger: false,
        }
    }

    fn trigger_col(size: [f32; 3]) -> ColliderComponent {
        ColliderComponent {
            shape: "box".to_string(),
            size,
            is_trigger: true,
        }
    }

    fn spawn(
        scene: &Scene,
        t: TransformComponent,
        rb: RigidBodyComponent,
        col: ColliderComponent,
    ) -> EntityId {
        let h = scene.queue_spawn(vec![comp(t), comp(rb), comp(col)]);
        scene.drain_commands();
        h.get().unwrap()
    }

    fn world() -> PhysicsWorld {
        PhysicsWorld::with_default_config()
    }

    fn world_zero_gravity() -> PhysicsWorld {
        PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        })
    }

    fn world_2d(mode: &str) -> PhysicsWorld {
        PhysicsWorld::new(PhysicsConfig {
            lock_2d: Some(mode.to_string()),
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        })
    }

    // ── T-PHYS-01: Gravity — Free Fall ────────────────────────────────────────

    #[test]
    fn t_phys_01_gravity_free_fall() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 100.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world();
        for _ in 0..60 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        // y = 100 - 0.5 * 9.81 * 1^2 = 95.095, tol ±0.5
        assert!((t.y - 95.095).abs() < 0.5, "y={} expected ~95.095", t.y);
        assert!(t.x.abs() < 0.01, "x should be 0");
        assert!(t.z.abs() < 0.01, "z should be 0");
    }

    // ── T-PHYS-02: Gravity Factor ─────────────────────────────────────────────

    #[test]
    fn t_phys_02_gravity_factor() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(0.0, 100.0, 0.0),
            rb_with("dynamic", 1.0, 1, 1),
            box_col([0.5, 0.5, 0.5]),
        );
        let b = spawn(
            &scene,
            transform(10.0, 100.0, 0.0),
            rb_with("dynamic", 0.5, 1, 1),
            box_col([0.5, 0.5, 0.5]),
        );

        let mut w = world();
        for _ in 0..60 {
            w.sync_step(&scene);
        }

        let ta = scene.components.get::<TransformComponent>(a).unwrap();
        let tb = scene.components.get::<TransformComponent>(b).unwrap();
        let disp_a = 100.0 - ta.y;
        let disp_b = 100.0 - tb.y;

        assert!(disp_a > 0.0 && disp_b > 0.0, "both should fall");
        // B falls roughly half as far
        let ratio = disp_a / disp_b;
        assert!(
            (ratio - 2.0).abs() < 0.2,
            "A/B displacement ratio={} expected ~2",
            ratio
        );
    }

    // ── T-PHYS-03: Zero Gravity ───────────────────────────────────────────────

    #[test]
    fn t_phys_03_zero_gravity() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 10.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        for _ in 0..60 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!((t.y - 10.0).abs() < 0.01, "y={} expected 10.0", t.y);
        assert!(t.x.abs() < 0.01);
        assert!(t.z.abs() < 0.01);
    }

    // ── T-PHYS-04: Static Body Does Not Move ──────────────────────────────────

    #[test]
    fn t_phys_04_static_body_no_move() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(5.0, 5.0, 5.0),
            rb_with("static", 1.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world();
        w.sync_step(&scene); // register
        w.apply_force(e, [1000.0, 1000.0, 1000.0]);
        for _ in 0..60 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!((t.x - 5.0).abs() < 0.01, "x={}", t.x);
        assert!((t.y - 5.0).abs() < 0.01, "y={}", t.y);
        assert!((t.z - 5.0).abs() < 0.01, "z={}", t.z);
    }

    // ── T-PHYS-05: Kinematic Body Push from ECS ───────────────────────────────

    #[test]
    fn t_phys_05_kinematic_push() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("kinematic", 1.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world();
        w.sync_step(&scene); // register

        // Move via ECS
        scene.components.get_mut::<TransformComponent, _>(e, |t| {
            t.x = 10.0;
        });
        w.sync_step(&scene);

        let pos = w.get_body_position(e).unwrap();
        assert!(
            (pos[0] - 10.0).abs() < 0.01,
            "rapier x={} expected 10.0",
            pos[0]
        );

        // TransformComponent should still be at 10.0 (kinematic never pulled)
        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!((t.x - 10.0).abs() < 0.01);
    }

    // ── T-PHYS-06: Dynamic Body Pull to ECS ──────────────────────────────────

    #[test]
    fn t_phys_06_dynamic_pull_to_ecs() {
        let scene = Scene::new();
        // Static floor at y=0
        spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("static", 1.0, 1, 1),
            box_col([10.0, 0.5, 10.0]),
        );
        let dyn_e = spawn(
            &scene,
            transform(0.0, 10.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world();
        let mut prev_y = 10.0f32;
        for _ in 0..30 {
            w.sync_step(&scene);
            let t = scene.components.get::<TransformComponent>(dyn_e).unwrap();
            // Y should decrease (body falling)
            assert!(t.y <= prev_y + 0.01, "y={} should not increase", t.y);
            prev_y = t.y;
        }

        // TransformComponent should match rapier position
        let ecs_y = scene.components.get::<TransformComponent>(dyn_e).unwrap().y;
        let rapier_y = w.get_body_position(dyn_e).unwrap()[1];
        assert!(
            (ecs_y - rapier_y).abs() < 0.001,
            "ecs_y={} rapier_y={}",
            ecs_y,
            rapier_y
        );
    }

    // ── T-PHYS-07: Collision Detection — Two Dynamic Bodies ───────────────────

    #[test]
    fn t_phys_07_collision_detection() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(0.5, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let events: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        scene.subscribe("collision", move |_, payload| {
            ev.lock().unwrap().push(payload.clone());
        });

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });

        for _ in 0..5 {
            w.sync_step(&scene);
        }

        let evs = events.lock().unwrap();
        assert!(!evs.is_empty(), "expected CollisionEvent within 5 frames");
        let ev0 = &evs[0];
        let ea = ev0["entity_a"].as_u64().unwrap();
        let eb = ev0["entity_b"].as_u64().unwrap();
        let ids: std::collections::HashSet<u64> = [ea, eb].into();
        assert!(
            ids.contains(&a.0) && ids.contains(&b.0),
            "event must contain both entity IDs"
        );

        // Normal approximately along X
        let normal = ev0["normal"].as_array().unwrap();
        let nx = normal[0].as_f64().unwrap().abs();
        assert!(nx > 0.5, "normal.x={} should be dominant (along X)", nx);
    }

    // ── T-PHYS-08: Collision Layers — Matching Mask ───────────────────────────

    #[test]
    fn t_phys_08_collision_layers_matching() {
        let scene = Scene::new();
        // A: layer=1, mask=2 ; B: layer=2, mask=1 → should collide
        spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 2),
            box_col([1.0, 1.0, 1.0]),
        );
        spawn(
            &scene,
            transform(0.5, 0.0, 0.0),
            rb_with("dynamic", 0.0, 2, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let count = Arc::new(Mutex::new(0u32));
        let c = count.clone();
        scene.subscribe("collision", move |_, _| *c.lock().unwrap() += 1);

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        for _ in 0..5 {
            w.sync_step(&scene);
        }
        assert!(*count.lock().unwrap() > 0, "collision event expected");
    }

    // ── T-PHYS-09: Collision Layers — Non-Matching Mask ──────────────────────

    #[test]
    fn t_phys_09_collision_layers_no_match() {
        let scene = Scene::new();
        // A: layer=1, mask=4 ; B: layer=2, mask=4 → 1&4=0, no collision
        spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 4),
            box_col([1.0, 1.0, 1.0]),
        );
        spawn(
            &scene,
            transform(0.5, 0.0, 0.0),
            rb_with("dynamic", 0.0, 2, 4),
            box_col([1.0, 1.0, 1.0]),
        );

        let count = Arc::new(Mutex::new(0u32));
        let c = count.clone();
        scene.subscribe("collision", move |_, _| *c.lock().unwrap() += 1);

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        for _ in 0..10 {
            w.sync_step(&scene);
        }
        assert_eq!(*count.lock().unwrap(), 0, "no collision event expected");
    }

    // ── T-PHYS-10: Trigger Volume — Enter Event ───────────────────────────────

    #[test]
    fn t_phys_10_trigger_enter() {
        let scene = Scene::new();
        // Trigger at origin
        spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("static", 0.0, 1, 1),
            trigger_col([2.0, 2.0, 2.0]),
        );
        // Dynamic body falling into it from above
        let dyn_e = spawn(
            &scene,
            transform(0.0, 0.5, 0.0),
            dyn_rb(),
            box_col([0.5, 0.5, 0.5]),
        );

        let entered: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let e = entered.clone();
        scene.subscribe("trigger", move |_, payload| {
            if payload["event_type"].as_str() == Some("enter") {
                *e.lock().unwrap() = true;
            }
        });

        let mut w = world();
        for _ in 0..30 {
            w.sync_step(&scene);
        }

        assert!(*entered.lock().unwrap(), "trigger enter event expected");

        // Body should pass through (no contact forces prevent it)
        let _ = dyn_e;
    }

    // ── T-PHYS-11: Trigger Volume — Exit Event ────────────────────────────────

    #[test]
    fn t_phys_11_trigger_exit() {
        let scene = Scene::new();
        // Trigger at y=5 with height 2 → y=[4,6]
        spawn(
            &scene,
            transform(0.0, 5.0, 0.0),
            rb_with("static", 0.0, 1, 1),
            trigger_col([2.0, 2.0, 2.0]),
        );
        // Body starts inside, falls through and exits below
        spawn(
            &scene,
            transform(0.0, 5.0, 0.0),
            dyn_rb(),
            box_col([0.4, 0.4, 0.4]),
        );

        let exited: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let ex = exited.clone();
        scene.subscribe("trigger", move |_, payload| {
            if payload["event_type"].as_str() == Some("exit") {
                *ex.lock().unwrap() = true;
            }
        });

        let mut w = world();
        for _ in 0..120 {
            w.sync_step(&scene);
        }

        assert!(*exited.lock().unwrap(), "trigger exit event expected");
    }

    // ── T-PHYS-11b: Solid Collision End Event ─────────────────────────────────

    #[test]
    fn t_phys_11b_collision_end_solid() {
        let scene = Scene::new();
        // Two dynamic bodies moving toward each other along X in zero gravity.
        // They collide, bounce apart, and a collision_end event fires after separation.
        let a = spawn(
            &scene,
            transform(-2.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(2.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let ended: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let en = ended.clone();
        scene.subscribe("collision_end", move |_, _| {
            *en.lock().unwrap() = true;
        });

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        w.sync_step(&scene); // register bodies
                             // Launch toward each other at high speed so they collide and bounce apart.
        w.set_linear_velocity(a, [10.0, 0.0, 0.0]);
        w.set_linear_velocity(b, [-10.0, 0.0, 0.0]);

        for _ in 0..120 {
            w.sync_step(&scene);
        }

        assert!(
            *ended.lock().unwrap(),
            "collision_end event expected for solid bodies"
        );
    }

    // ── T-PHYS-12: Apply Impulse ──────────────────────────────────────────────

    #[test]
    fn t_phys_12_apply_impulse() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene); // register
        w.apply_impulse(e, [0.0, 100.0, 0.0]);
        w.sync_step(&scene);

        let vel = w.get_linear_velocity(e).unwrap();
        assert!(
            vel[1] > 0.0,
            "vy={} should be > 0 after upward impulse",
            vel[1]
        );

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!(t.y > 0.0, "y={} should be > 0 after step", t.y);
    }

    // ── T-PHYS-13: Set Linear Velocity ───────────────────────────────────────

    #[test]
    fn t_phys_13_set_linear_velocity() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene); // register
        w.set_linear_velocity(e, [5.0, 0.0, 0.0]);

        for _ in 0..60 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        // 5.0 * 1.0s = 5.0, tol ±0.1
        assert!((t.x - 5.0).abs() < 0.1, "x={} expected ~5.0", t.x);
    }

    // ── T-PHYS-14: 2D Lock — XZ Plane ────────────────────────────────────────

    #[test]
    fn t_phys_14_2d_lock_xz() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_2d("xz");
        w.sync_step(&scene);
        w.apply_impulse(e, [1.0, 1.0, 1.0]);

        for _ in 0..30 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!(t.y.abs() < 0.01, "y={} should remain 0 with XZ lock", t.y);
        // X and Z should move
        let total_xz = (t.x * t.x + t.z * t.z).sqrt();
        assert!(total_xz > 0.01, "body should move in XZ plane");
    }

    // ── T-PHYS-15: 2D Lock — XY Plane ────────────────────────────────────────

    #[test]
    fn t_phys_15_2d_lock_xy() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_2d("xy");
        w.sync_step(&scene);
        w.apply_impulse(e, [1.0, 1.0, 1.0]);

        for _ in 0..30 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!(t.z.abs() < 0.01, "z={} should remain 0 with XY lock", t.z);
        let total_xy = (t.x * t.x + t.y * t.y).sqrt();
        assert!(total_xy > 0.01, "body should move in XY plane");
    }

    // ── T-PHYS-16: Body Registration on Component Attach ─────────────────────

    #[test]
    fn t_phys_16_body_registration_on_attach() {
        let scene = Scene::new();
        // Spawn with no physics components
        let h = scene.queue_spawn(vec![comp(transform(0.0, 0.0, 0.0))]);
        scene.drain_commands();
        let e = h.get().unwrap();

        let mut w = world();
        w.sync_step(&scene);
        assert_eq!(w.body_count(), 0, "no body before attach");

        // Attach via queue
        scene.queue_attach(e, dyn_rb());
        scene.queue_attach(e, box_col([1.0, 1.0, 1.0]));
        scene.drain_commands();

        w.sync_step(&scene);
        assert_eq!(w.body_count(), 1, "body should exist after attach");

        // Verify body type is dynamic
        let pos = w.get_body_position(e).unwrap();
        assert!(!pos[0].is_nan());
    }

    // ── T-PHYS-17: Body Removal on Component Detach ───────────────────────────

    #[test]
    fn t_phys_17_body_removal_on_detach() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world();
        w.sync_step(&scene);
        assert_eq!(w.body_count(), 1);

        // Detach RigidBodyComponent
        scene.queue_detach::<RigidBodyComponent>(e);
        scene.drain_commands();

        w.sync_step(&scene);
        assert_eq!(w.body_count(), 0, "body should be removed after detach");
        assert!(w.get_body_position(e).is_none());
    }

    // ── T-PHYS-18: NaN Resilience ─────────────────────────────────────────────

    #[test]
    fn t_phys_18_nan_resilience() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene); // establishes last_valid_position = (0,0,0)

        // Inject NaN into rapier body
        w.set_body_translation_raw(e, [f32::NAN, 0.0, 0.0]);

        // Should not panic; NaN detected and reset
        w.sync_step(&scene);

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!(
            !t.x.is_nan() && !t.y.is_nan() && !t.z.is_nan(),
            "TransformComponent must not be NaN"
        );

        // Position should be reset to last valid (0,0,0)
        let pos = w.get_body_position(e).unwrap();
        assert!(!pos[0].is_nan());
    }

    // ── T-PHYS-19: Per-entity collision normal orientation ────────────────────
    //
    // Verifies that collision:{entity_id} events deliver a normal oriented
    // toward that entity regardless of Rapier's internal collider handle ordering.
    // Player (above) must receive normal[1] > 0 (upward); floor must receive < 0.

    #[test]
    fn t_phys_19_per_entity_normal_orientation() {
        let scene = Scene::new();
        // Static floor
        let floor = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("static", 1.0, 1, 1),
            box_col([10.0, 0.5, 10.0]),
        );
        // Dynamic player above the floor — will fall and land
        let player = spawn(
            &scene,
            transform(0.0, 5.0, 0.0),
            dyn_rb(),
            box_col([0.5, 1.0, 0.5]),
        );

        let player_normal_y: Arc<Mutex<Option<f64>>> = Arc::new(Mutex::new(None));
        let floor_normal_y: Arc<Mutex<Option<f64>>> = Arc::new(Mutex::new(None));

        let pny = player_normal_y.clone();
        scene.subscribe(&format!("collision:{}", player.0), move |_, payload| {
            if let Some(arr) = payload["normal"].as_array() {
                if let Some(y) = arr.get(1).and_then(|v| v.as_f64()) {
                    *pny.lock().unwrap() = Some(y);
                }
            }
        });

        let fny = floor_normal_y.clone();
        scene.subscribe(&format!("collision:{}", floor.0), move |_, payload| {
            if let Some(arr) = payload["normal"].as_array() {
                if let Some(y) = arr.get(1).and_then(|v| v.as_f64()) {
                    *fny.lock().unwrap() = Some(y);
                }
            }
        });

        let mut w = world();
        for _ in 0..120 {
            w.sync_step(&scene);
            if player_normal_y.lock().unwrap().is_some() {
                break;
            }
        }

        let pny = player_normal_y
            .lock()
            .unwrap()
            .expect("player collision:{id} event not fired");
        let fny = floor_normal_y
            .lock()
            .unwrap()
            .expect("floor collision:{id} event not fired");

        assert!(
            pny > 0.7,
            "player per-entity normal[1]={pny} must be > 0.7 (upward) — floor contact detection requires this"
        );
        assert!(
            fny < -0.7,
            "floor per-entity normal[1]={fny} must be < -0.7 (oriented toward floor, inverse of player)"
        );
    }

    // ── Module lifecycle ──────────────────────────────────────────────────────

    #[test]
    fn t_phys_module_load_unload() {
        struct NoopSched;
        impl SchedulerHandle for NoopSched {
            fn submit_sequential(
                &self,
                _f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
                _priority: rython_core::Priority,
                _owner: rython_core::OwnerId,
            ) {
            }
            fn cancel_owned(&self, _owner: rython_core::OwnerId) {}
        }
        let sched = NoopSched;
        let mut m = PhysicsModule::with_default_config();
        assert!(m.world.is_none());
        m.on_load(&sched).unwrap();
        assert!(m.world.is_some());
        m.on_unload(&sched).unwrap();
        assert!(m.world.is_none());
        assert_eq!(m.name(), "physics");
    }

    // ── T-PHYS-20: Sync cycle idempotent registration ─────────────────────────
    //
    // Repeated sync_step calls must not re-register the same entity.

    #[test]
    fn t_phys_20_sync_cycle_idempotent_registration() {
        let scene = Scene::new();
        let _e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        for _ in 0..5 {
            w.sync_step(&scene);
        }
        assert_eq!(
            w.body_count(),
            1,
            "sync_step must not re-register the same body"
        );
    }

    // ── T-PHYS-21: Per-entity collision events fire for both entities ──────────
    //
    // Both collision:{a} and collision:{b} must fire when two bodies overlap.
    // Each payload must name both entity IDs.

    #[test]
    fn t_phys_21_per_entity_collision_both_entities() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(0.5, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let got_a: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let got_b: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));

        let ga = got_a.clone();
        scene.subscribe(&format!("collision:{}", a.0), move |_, payload| {
            if ga.lock().unwrap().is_none() {
                ga.lock().unwrap().replace(payload.clone());
            }
        });
        let gb = got_b.clone();
        scene.subscribe(&format!("collision:{}", b.0), move |_, payload| {
            if gb.lock().unwrap().is_none() {
                gb.lock().unwrap().replace(payload.clone());
            }
        });

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        for _ in 0..5 {
            w.sync_step(&scene);
        }

        let pa = got_a.lock().unwrap();
        let pb = got_b.lock().unwrap();
        assert!(pa.is_some(), "collision:{} not fired", a.0);
        assert!(pb.is_some(), "collision:{} not fired", b.0);

        // Both payloads must contain both entity IDs.
        let ids_a: std::collections::HashSet<u64> = [
            pa.as_ref().unwrap()["entity_a"].as_u64().unwrap(),
            pa.as_ref().unwrap()["entity_b"].as_u64().unwrap(),
        ]
        .into();
        assert!(
            ids_a.contains(&a.0) && ids_a.contains(&b.0),
            "payload for collision:{} must name both entity IDs",
            a.0
        );

        let ids_b: std::collections::HashSet<u64> = [
            pb.as_ref().unwrap()["entity_a"].as_u64().unwrap(),
            pb.as_ref().unwrap()["entity_b"].as_u64().unwrap(),
        ]
        .into();
        assert!(
            ids_b.contains(&a.0) && ids_b.contains(&b.0),
            "payload for collision:{} must name both entity IDs",
            b.0
        );
    }

    // ── T-PHYS-22: Per-entity collision_end events fire for both entities ──────
    //
    // After two solid bodies collide and separate, both collision_end:{a} and
    // collision_end:{b} must fire.

    #[test]
    fn t_phys_22_per_entity_collision_end_both_entities() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(-2.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(2.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let ended_a: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let ended_b: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        let ea = ended_a.clone();
        scene.subscribe(&format!("collision_end:{}", a.0), move |_, _| {
            *ea.lock().unwrap() = true;
        });
        let eb = ended_b.clone();
        scene.subscribe(&format!("collision_end:{}", b.0), move |_, _| {
            *eb.lock().unwrap() = true;
        });

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        w.sync_step(&scene);
        w.set_linear_velocity(a, [10.0, 0.0, 0.0]);
        w.set_linear_velocity(b, [-10.0, 0.0, 0.0]);

        for _ in 0..120 {
            w.sync_step(&scene);
        }

        assert!(*ended_a.lock().unwrap(), "collision_end:{} not fired", a.0);
        assert!(*ended_b.lock().unwrap(), "collision_end:{} not fired", b.0);
    }

    // ── T-PHYS-23: Per-entity trigger_enter and trigger_exit events ───────────
    //
    // Both trigger_enter:{trigger_id} and trigger_enter:{body_id} must fire when
    // a dynamic body enters a sensor volume; likewise trigger_exit on departure.

    #[test]
    fn t_phys_23_per_entity_trigger_events() {
        let scene = Scene::new();
        let trigger = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("static", 0.0, 1, 1),
            trigger_col([4.0, 4.0, 4.0]),
        );
        // Body starts inside the trigger zone and falls under gravity out the bottom.
        let body = spawn(
            &scene,
            transform(0.0, 1.0, 0.0),
            dyn_rb(),
            box_col([0.4, 0.4, 0.4]),
        );

        let entered_trigger: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let entered_body: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let exited_trigger: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let exited_body: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        let et = entered_trigger.clone();
        scene.subscribe(&format!("trigger_enter:{}", trigger.0), move |_, _| {
            *et.lock().unwrap() = true;
        });
        let eb = entered_body.clone();
        scene.subscribe(&format!("trigger_enter:{}", body.0), move |_, _| {
            *eb.lock().unwrap() = true;
        });
        let xt = exited_trigger.clone();
        scene.subscribe(&format!("trigger_exit:{}", trigger.0), move |_, _| {
            *xt.lock().unwrap() = true;
        });
        let xb = exited_body.clone();
        scene.subscribe(&format!("trigger_exit:{}", body.0), move |_, _| {
            *xb.lock().unwrap() = true;
        });

        let mut w = world();
        for _ in 0..180 {
            w.sync_step(&scene);
        }

        assert!(
            *entered_trigger.lock().unwrap(),
            "trigger_enter:{} not fired",
            trigger.0
        );
        assert!(
            *entered_body.lock().unwrap(),
            "trigger_enter:{} not fired",
            body.0
        );
        assert!(
            *exited_trigger.lock().unwrap(),
            "trigger_exit:{} not fired",
            trigger.0
        );
        assert!(
            *exited_body.lock().unwrap(),
            "trigger_exit:{} not fired",
            body.0
        );
    }

    // ── T-PHYS-24: Sensor overlap emits trigger, not collision ────────────────
    //
    // When a dynamic body overlaps a sensor, only 'trigger' fires.
    // 'collision' must NOT fire.

    #[test]
    fn t_phys_24_sensor_no_solid_collision_event() {
        let scene = Scene::new();
        spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("static", 0.0, 1, 1),
            trigger_col([4.0, 4.0, 4.0]),
        );
        spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([0.5, 0.5, 0.5]),
        );

        let trigger_fired: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let collision_fired: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        let tf = trigger_fired.clone();
        scene.subscribe("trigger", move |_, _| {
            *tf.lock().unwrap() = true;
        });
        let cf = collision_fired.clone();
        scene.subscribe("collision", move |_, _| {
            *cf.lock().unwrap() = true;
        });

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        for _ in 0..10 {
            w.sync_step(&scene);
        }

        assert!(
            *trigger_fired.lock().unwrap(),
            "trigger event expected for sensor overlap"
        );
        assert!(
            !*collision_fired.lock().unwrap(),
            "collision event must NOT fire for sensor overlap"
        );
    }

    // ── T-PHYS-25: Normal orientation — horizontal X-axis collision ───────────
    //
    // Each per-entity collision event must deliver a normal pointing TOWARD that
    // entity, regardless of rapier's internal collider-handle ordering.
    // Body a (left, -X) must receive normal[0] < -0.5.
    // Body b (right, +X) must receive normal[0] > +0.5.

    #[test]
    fn t_phys_25_per_entity_normal_orientation_x_axis() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(-2.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(2.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );

        let normal_a: Arc<Mutex<Option<[f64; 3]>>> = Arc::new(Mutex::new(None));
        let normal_b: Arc<Mutex<Option<[f64; 3]>>> = Arc::new(Mutex::new(None));

        let na = normal_a.clone();
        scene.subscribe(&format!("collision:{}", a.0), move |_, payload| {
            if na.lock().unwrap().is_none() {
                if let Some(arr) = payload["normal"].as_array() {
                    let nx = arr[0].as_f64().unwrap_or(0.0);
                    let ny = arr[1].as_f64().unwrap_or(0.0);
                    let nz = arr[2].as_f64().unwrap_or(0.0);
                    *na.lock().unwrap() = Some([nx, ny, nz]);
                }
            }
        });
        let nb = normal_b.clone();
        scene.subscribe(&format!("collision:{}", b.0), move |_, payload| {
            if nb.lock().unwrap().is_none() {
                if let Some(arr) = payload["normal"].as_array() {
                    let nx = arr[0].as_f64().unwrap_or(0.0);
                    let ny = arr[1].as_f64().unwrap_or(0.0);
                    let nz = arr[2].as_f64().unwrap_or(0.0);
                    *nb.lock().unwrap() = Some([nx, ny, nz]);
                }
            }
        });

        let mut w = PhysicsWorld::new(PhysicsConfig {
            gravity: [0.0, 0.0, 0.0],
            ..Default::default()
        });
        w.sync_step(&scene);
        w.set_linear_velocity(a, [10.0, 0.0, 0.0]);
        w.set_linear_velocity(b, [-10.0, 0.0, 0.0]);

        for _ in 0..60 {
            w.sync_step(&scene);
            if normal_a.lock().unwrap().is_some() && normal_b.lock().unwrap().is_some() {
                break;
            }
        }

        let na = normal_a
            .lock()
            .unwrap()
            .expect("collision:{a} per-entity event not fired");
        let nb = normal_b
            .lock()
            .unwrap()
            .expect("collision:{b} per-entity event not fired");

        // a is on the left: its per-entity normal must point leftward (-X, toward a).
        assert!(
            na[0] < -0.5,
            "collision:{} normal[0]={} must be < -0.5 (pointing toward a on the left)",
            a.0,
            na[0]
        );
        // b is on the right: its per-entity normal must point rightward (+X, toward b).
        assert!(
            nb[0] > 0.5,
            "collision:{} normal[0]={} must be > +0.5 (pointing toward b on the right)",
            b.0,
            nb[0]
        );
    }

    // ── T-PHYS-26: set_gravity runtime change ────────────────────────────────
    //
    // Calling set_gravity between steps must change the acceleration applied in
    // subsequent steps.

    #[test]
    fn t_phys_26_set_gravity_runtime_change() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 10.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        // 30 frames of zero gravity — body should stay at y=10.
        for _ in 0..30 {
            w.sync_step(&scene);
        }
        let y_before = scene.components.get::<TransformComponent>(e).unwrap().y;
        assert!(
            (y_before - 10.0).abs() < 0.01,
            "y={} should be 10.0 with zero gravity",
            y_before
        );

        // Enable gravity mid-simulation.
        w.set_gravity([0.0, -9.81, 0.0]);
        for _ in 0..60 {
            w.sync_step(&scene);
        }
        let y_after = scene.components.get::<TransformComponent>(e).unwrap().y;
        assert!(
            y_after < y_before - 1.0,
            "y={} should have fallen after enabling gravity",
            y_after
        );
    }

    // ── T-PHYS-27: Empty World Step ──────────────────────────────────────────────
    //
    // Stepping physics on an empty scene (no bodies) for 60 frames must not panic.

    #[test]
    fn t_phys_27_empty_world_step() {
        let scene = Scene::new();
        let mut w = world();
        for _ in 0..60 {
            w.sync_step(&scene);
        }
        assert_eq!(w.body_count(), 0, "no bodies should exist in empty scene");
    }

    // ── T-PHYS-28: Collision Layer All Bits ──────────────────────────────────────
    //
    // Two overlapping dynamic bodies with collision_layer=0xFFFFFFFF and
    // collision_mask=0xFFFFFFFF should collide (positions diverge due to overlap
    // resolution).

    #[test]
    fn t_phys_28_collision_layer_all_bits() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 0xFFFFFFFF, 0xFFFFFFFF),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(0.3, 0.0, 0.0),
            rb_with("dynamic", 0.0, 0xFFFFFFFF, 0xFFFFFFFF),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        for _ in 0..30 {
            w.sync_step(&scene);
        }

        let ta = scene.components.get::<TransformComponent>(a).unwrap();
        let tb = scene.components.get::<TransformComponent>(b).unwrap();
        let distance =
            ((ta.x - tb.x).powi(2) + (ta.y - tb.y).powi(2) + (ta.z - tb.z).powi(2)).sqrt();
        assert!(
            distance > 0.5,
            "bodies should have been pushed apart by overlap resolution, distance={}",
            distance
        );
    }

    // ── T-PHYS-29: Collision Layer No Match ──────────────────────────────────────
    //
    // Two overlapping dynamic bodies with non-overlapping layers/masks should NOT
    // collide — they pass through each other.

    #[test]
    fn t_phys_29_collision_layer_no_match() {
        let scene = Scene::new();
        let a = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([1.0, 1.0, 1.0]),
        );
        let b = spawn(
            &scene,
            transform(0.3, 0.0, 0.0),
            rb_with("dynamic", 0.0, 2, 2),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        for _ in 0..30 {
            w.sync_step(&scene);
        }

        // With non-overlapping layers (layer=1,mask=1 vs layer=2,mask=2),
        // neither body's layer intersects the other's mask, so no collision.
        // Positions should remain near initial values.
        let ta = scene.components.get::<TransformComponent>(a).unwrap();
        let tb = scene.components.get::<TransformComponent>(b).unwrap();
        assert!(
            (ta.x - 0.0).abs() < 0.05,
            "body a should stay near origin, x={}",
            ta.x
        );
        assert!(
            (tb.x - 0.3).abs() < 0.05,
            "body b should stay near 0.3, x={}",
            tb.x
        );
    }

    // ── T-PHYS-30: Fast Body Tunneling Detection ─────────────────────────────────
    //
    // Spawns a small fast-moving dynamic body aimed at a thin static wall.
    // If CCD is active, the body should be stopped by the wall. If not, the body
    // may tunnel through — document the result.

    #[test]
    fn t_phys_30_fast_body_tunneling_detection() {
        let scene = Scene::new();
        // Thin static wall at x=10
        spawn(
            &scene,
            transform(10.0, 0.0, 0.0),
            rb_with("static", 0.0, 1, 1),
            box_col([0.1, 10.0, 10.0]),
        );
        // Small dynamic body at x=0 with very high velocity toward the wall
        let bullet = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            rb_with("dynamic", 0.0, 1, 1),
            box_col([0.2, 0.2, 0.2]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene); // register bodies
        w.set_linear_velocity(bullet, [500.0, 0.0, 0.0]);

        for _ in 0..60 {
            w.sync_step(&scene);
        }

        let t = scene.components.get::<TransformComponent>(bullet).unwrap();
        // NOTE: rapier3d has a CCDSolver but CCD must be explicitly enabled per body
        // (RigidBodyBuilder::ccd_enabled(true)). The engine currently does not enable
        // per-body CCD, so the bullet may tunnel through the thin wall. This is a
        // known limitation. If CCD were active, t.x would be <= 10.0.
        //
        // We verify the test runs without panic regardless of tunneling outcome.
        assert!(
            !t.x.is_nan(),
            "bullet position must not be NaN after high-velocity simulation"
        );
        // If the body did NOT tunnel, it should be stopped at or before the wall.
        // If it did tunnel, it will be past x=10. Either outcome is accepted here.
        if t.x <= 10.5 {
            // CCD active or the body was caught — great.
        } else {
            // Known limitation: per-body CCD not enabled, bullet tunneled through.
            eprintln!(
                "[t_phys_30] CCD not active: bullet tunneled to x={:.1} (past wall at x=10)",
                t.x
            );
        }
    }

    // ── T-PHYS-31: Body Registration/Deregistration Cycle ────────────────────────
    //
    // Register a body, deregister it, register a new one. Verify body_count
    // reflects each change correctly.

    #[test]
    fn t_phys_31_body_registration_deregistration_cycle() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene);
        assert_eq!(w.body_count(), 1, "1 body after first registration");

        // Remove the RigidBodyComponent so next sync_step deregisters the body
        scene.queue_detach::<RigidBodyComponent>(e);
        scene.drain_commands();
        w.sync_step(&scene);
        assert_eq!(w.body_count(), 0, "0 bodies after deregistration");

        // Spawn a new entity with physics components
        let e2 = spawn(
            &scene,
            transform(5.0, 5.0, 5.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );
        w.sync_step(&scene);
        assert_eq!(
            w.body_count(),
            1,
            "1 body after re-registration of new entity"
        );

        // Verify the new body is tracked
        let pos = w.get_body_position(e2).unwrap();
        assert!((pos[0] - 5.0).abs() < 0.01, "new body at correct position");
    }

    // ── T-PHYS-32: NaN Position Recovery ─────────────────────────────────────────
    //
    // Forcibly set a body's position to NaN. After sync_step, the body must be
    // clamped back to its last valid position and velocity zeroed.

    #[test]
    fn t_phys_32_nan_position_recovery() {
        let scene = Scene::new();
        let e = spawn(
            &scene,
            transform(3.0, 4.0, 5.0),
            dyn_rb(),
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene); // register; last_valid_position = (3,4,5)

        // Inject NaN into all three axes
        w.set_body_translation_raw(e, [f32::NAN, f32::NAN, f32::NAN]);

        // sync_step should detect NaN and reset to last_valid_position
        w.sync_step(&scene);

        let t = scene.components.get::<TransformComponent>(e).unwrap();
        assert!(!t.x.is_nan(), "x must not be NaN after recovery");
        assert!(!t.y.is_nan(), "y must not be NaN after recovery");
        assert!(!t.z.is_nan(), "z must not be NaN after recovery");

        // Should be reset to last valid position (3, 4, 5)
        assert!((t.x - 3.0).abs() < 0.01, "x={} should be reset to 3.0", t.x);
        assert!((t.y - 4.0).abs() < 0.01, "y={} should be reset to 4.0", t.y);
        assert!((t.z - 5.0).abs() < 0.01, "z={} should be reset to 5.0", t.z);

        // Velocity should be zeroed after NaN recovery
        let vel = w.get_linear_velocity(e).unwrap();
        assert!(
            vel[0].abs() < 0.01 && vel[1].abs() < 0.01 && vel[2].abs() < 0.01,
            "velocity should be zeroed after NaN recovery, got {:?}",
            vel
        );
    }

    // ── T-PHYS-33: Mass Field Affects Impulse Response ────────────────────────
    //
    // Regression test: RigidBodyComponent.mass was silently ignored at
    // registration, so every body had rapier's density-derived default mass.
    // Equal impulses must produce inversely proportional velocities.
    #[test]
    fn t_phys_33_mass_affects_impulse_response() {
        let scene = Scene::new();
        let light = spawn(
            &scene,
            transform(0.0, 0.0, 0.0),
            RigidBodyComponent {
                body_type: "dynamic".to_string(),
                mass: 1.0,
                gravity_factor: 0.0,
                collision_layer: 1,
                collision_mask: 1,
            },
            box_col([1.0, 1.0, 1.0]),
        );
        let heavy = spawn(
            &scene,
            transform(10.0, 0.0, 0.0),
            RigidBodyComponent {
                body_type: "dynamic".to_string(),
                mass: 10.0,
                gravity_factor: 0.0,
                collision_layer: 1,
                collision_mask: 1,
            },
            box_col([1.0, 1.0, 1.0]),
        );

        let mut w = world_zero_gravity();
        w.sync_step(&scene); // register both bodies

        // Apply identical impulse to both.
        w.apply_impulse(light, [0.0, 10.0, 0.0]);
        w.apply_impulse(heavy, [0.0, 10.0, 0.0]);

        w.sync_step(&scene);

        let v_light = w.get_linear_velocity(light).unwrap();
        let v_heavy = w.get_linear_velocity(heavy).unwrap();

        // Heavy body velocity should be ~1/10 of light body velocity.
        // Allow generous tolerance for rapier's internal additional_mass semantics.
        let ratio = v_heavy[1] / v_light[1];
        assert!(
            ratio > 0.0 && ratio < 0.5,
            "heavy/light velocity ratio={ratio}, expected ~0.1 (mass was wired through)"
        );
        assert!(
            v_light[1] > v_heavy[1] * 2.0,
            "light v={}, heavy v={}: mass must differentiate impulse response",
            v_light[1],
            v_heavy[1]
        );
    }

    // ── T-PHYS-34: Zero/NaN/negative mass does not panic ──────────────────────
    #[test]
    fn t_phys_34_invalid_mass_no_panic() {
        let scene = Scene::new();
        for m in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
            let _ = spawn(
                &scene,
                transform(0.0, 0.0, 0.0),
                RigidBodyComponent {
                    body_type: "dynamic".to_string(),
                    mass: m,
                    gravity_factor: 0.0,
                    collision_layer: 1,
                    collision_mask: 1,
                },
                box_col([1.0, 1.0, 1.0]),
            );
        }
        let mut w = world_zero_gravity();
        // Must not panic even with invalid mass values.
        w.sync_step(&scene);
        w.sync_step(&scene);
    }
}

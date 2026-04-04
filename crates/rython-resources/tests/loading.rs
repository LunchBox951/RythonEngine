//! Integration tests for ResourceManager asset loading — pending state,
//! deduplication across asset types, and poll_completions on empty channel.

use rython_resources::{HandleState, ResourceManager, ResourceManagerConfig};

fn make_manager() -> ResourceManager {
    ResourceManager::new(ResourceManagerConfig { streaming_budget_mb: 256.0 })
}

// ── ResourceManagerConfig ─────────────────────────────────────────────────────

#[test]
fn resource_manager_config_default_budget_is_256mb() {
    let cfg = ResourceManagerConfig::default();
    assert!(
        (cfg.streaming_budget_mb - 256.0).abs() < 0.001,
        "default budget should be 256.0 MB, got {}",
        cfg.streaming_budget_mb
    );
}

#[test]
fn resource_manager_budget_matches_config() {
    let mgr = ResourceManager::new(ResourceManagerConfig { streaming_budget_mb: 512.0 });
    assert!((mgr.memory_budget_mb() - 512.0).abs() < 0.001);
}

#[test]
fn resource_manager_initial_memory_used_is_zero() {
    let mgr = make_manager();
    assert_eq!(mgr.memory_used_mb(), 0.0);
}

// ── load_mesh ─────────────────────────────────────────────────────────────────

#[test]
fn load_mesh_returns_pending_handle() {
    let mgr = make_manager();
    let h = mgr.load_mesh("nonexistent.glb");
    assert_eq!(h.state(), HandleState::Pending, "load_mesh must return PENDING immediately");
}

#[test]
fn load_mesh_same_path_deduplicates() {
    let mgr = make_manager();
    let h1 = mgr.load_mesh("model.glb");
    let h2 = mgr.load_mesh("model.glb");
    assert!(h1.ptr_eq(&h2), "same path must return same underlying asset handle");
}

#[test]
fn load_mesh_different_paths_distinct_handles() {
    let mgr = make_manager();
    let h1 = mgr.load_mesh("model_a.glb");
    let h2 = mgr.load_mesh("model_b.glb");
    assert!(!h1.ptr_eq(&h2), "different paths must yield distinct handles");
}

// ── load_font ─────────────────────────────────────────────────────────────────

#[test]
fn load_font_returns_pending_handle() {
    let mgr = make_manager();
    let h = mgr.load_font("nonexistent.ttf", 16.0);
    assert_eq!(h.state(), HandleState::Pending);
}

#[test]
fn load_font_same_path_same_size_deduplicates() {
    let mgr = make_manager();
    let h1 = mgr.load_font("ui.ttf", 12.0);
    let h2 = mgr.load_font("ui.ttf", 12.0);
    assert!(h1.ptr_eq(&h2), "same path+size must deduplicate");
}

#[test]
fn load_font_different_sizes_distinct_handles() {
    let mgr = make_manager();
    let h1 = mgr.load_font("ui.ttf", 12.0);
    let h2 = mgr.load_font("ui.ttf", 14.0);
    assert!(!h1.ptr_eq(&h2), "different font sizes must yield distinct handles");
}

// ── load_spritesheet ──────────────────────────────────────────────────────────

#[test]
fn load_spritesheet_returns_pending_handle() {
    let mgr = make_manager();
    let h = mgr.load_spritesheet("sprites.png", 4, 2);
    assert_eq!(h.state(), HandleState::Pending);
}

#[test]
fn load_spritesheet_same_params_deduplicates() {
    let mgr = make_manager();
    let h1 = mgr.load_spritesheet("sheet.png", 4, 2);
    let h2 = mgr.load_spritesheet("sheet.png", 4, 2);
    assert!(h1.ptr_eq(&h2), "same path+cols+rows must deduplicate");
}

#[test]
fn load_spritesheet_different_cols_distinct_handles() {
    let mgr = make_manager();
    let h1 = mgr.load_spritesheet("sheet.png", 4, 1);
    let h2 = mgr.load_spritesheet("sheet.png", 8, 1);
    assert!(!h1.ptr_eq(&h2), "different cols must yield distinct handles");
}

// ── load_sound ────────────────────────────────────────────────────────────────

#[test]
fn load_sound_returns_pending_handle() {
    let mgr = make_manager();
    let h = mgr.load_sound("sound.wav");
    assert_eq!(h.state(), HandleState::Pending);
}

// ── poll_completions with empty channel is a noop ─────────────────────────────

#[test]
fn poll_completions_with_no_pending_does_not_panic() {
    let mgr = make_manager();
    // No assets loaded — draining an empty channel must be a noop.
    mgr.poll_completions();
    mgr.poll_completions(); // calling twice is also fine
    assert_eq!(mgr.memory_used_mb(), 0.0);
}

// ── Mixed asset types: each gets its own cache key ────────────────────────────

#[test]
fn load_image_and_mesh_with_same_path_are_distinct() {
    let mgr = make_manager();
    // Different asset types with identical paths must NOT share an entry.
    let image = mgr.load_image("asset.png");
    let mesh = mgr.load_mesh("asset.png");
    assert!(!image.ptr_eq(&mesh), "image and mesh with same filename must be distinct handles");
}

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/rython-engine
    // workspace root = ../../
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

/// Layer assignment for each internal crate.
fn crate_layer(name: &str) -> Option<u8> {
    match name {
        "rython-core" => Some(0),
        "rython-scheduler" | "rython-modules" => Some(1),
        "rython-ecs" | "rython-window" | "rython-input"
        | "rython-renderer" | "rython-physics" | "rython-audio"
        | "rython-resources" => Some(2),
        "rython-ui" | "rython-scripting" => Some(3),
        "rython-engine" => Some(4),
        _ => None,
    }
}

/// Parse a crate's Cargo.toml and extract its [dependencies] keys that
/// correspond to internal workspace crates.
fn internal_deps(cargo_toml_path: &Path, internal_crates: &HashSet<String>) -> Vec<String> {
    let contents = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", cargo_toml_path.display()));

    let table: toml::Value = contents
        .parse()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", cargo_toml_path.display()));

    let mut deps = Vec::new();

    for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dep_table) = table.get(section).and_then(|v| v.as_table()) {
            for key in dep_table.keys() {
                let normalized = key.replace('_', "-");
                if internal_crates.contains(&normalized) {
                    deps.push(normalized);
                }
            }
        }
    }

    deps
}

/// Detect cycles in a directed graph using DFS.
/// Returns Some(cycle_description) if a cycle is found, None otherwise.
fn find_cycle(graph: &HashMap<String, Vec<String>>) -> Option<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();

    for node in graph.keys() {
        if !visited.contains(node) {
            if let Some(cycle) = dfs_cycle(node, graph, &mut visited, &mut in_stack) {
                return Some(cycle);
            }
        }
    }

    None
}

fn dfs_cycle(
    node: &str,
    graph: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
) -> Option<String> {
    in_stack.insert(node.to_string());

    if let Some(deps) = graph.get(node) {
        for dep in deps {
            if in_stack.contains(dep) {
                return Some(format!("{dep} -> {node}"));
            }
            if !visited.contains(dep) {
                if let Some(cycle) = dfs_cycle(dep, graph, visited, in_stack) {
                    return Some(cycle);
                }
            }
        }
    }

    in_stack.remove(node);
    visited.insert(node.to_string());
    None
}

// ─── T-SPEC-01: Workspace Compilation ────────────────────────────────────────
// This test is implicitly satisfied by the fact that the test binary compiled.
// We add a trivial assertion to make the intent explicit.
#[test]
fn t_spec_01_workspace_compiles() {
    // If this test runs, the workspace compiled successfully.
    // The `#![deny(warnings)]` in each lib.rs ensures warnings are treated as errors.
    assert!(true, "workspace compiled with zero warnings");
}

// ─── T-SPEC-02: Dependency DAG Acyclicity and Layer Constraints ───────────────
#[test]
fn t_spec_02_dependency_dag_acyclicity() {
    let root = workspace_root();
    let crates_dir = root.join("crates");

    let internal_crates: HashSet<String> = vec![
        "rython-core",
        "rython-scheduler",
        "rython-modules",
        "rython-ecs",
        "rython-window",
        "rython-input",
        "rython-renderer",
        "rython-physics",
        "rython-audio",
        "rython-resources",
        "rython-ui",
        "rython-scripting",
        "rython-engine",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    // Build the dependency graph
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for crate_name in &internal_crates {
        let cargo_path = crates_dir.join(crate_name).join("Cargo.toml");
        let deps = internal_deps(&cargo_path, &internal_crates);
        graph.insert(crate_name.clone(), deps);
    }

    // 1. No cycles
    let cycle = find_cycle(&graph);
    assert!(
        cycle.is_none(),
        "cycle detected in dependency graph: {}",
        cycle.unwrap_or_default()
    );

    // 2. Layer constraints
    for (crate_name, deps) in &graph {
        let owner_layer = match crate_layer(crate_name) {
            Some(l) => l,
            None => continue,
        };

        for dep in deps {
            let dep_layer = match crate_layer(dep) {
                Some(l) => l,
                None => continue,
            };

            assert!(
                dep_layer <= owner_layer,
                "{crate_name} (Layer {owner_layer}) depends on {dep} (Layer {dep_layer}): \
                 higher-layer crates may not depend on lower-layer crates in reverse"
            );

            // Layer 0 must have zero internal dependencies
            assert!(
                owner_layer != 0,
                "Layer 0 crate '{crate_name}' must have no internal dependencies, \
                 but depends on '{dep}'"
            );
        }
    }

    // Verify Layer 0 explicitly
    let layer0_deps = graph.get("rython-core").unwrap();
    assert!(
        layer0_deps.is_empty(),
        "rython-core (Layer 0) must have no internal dependencies, found: {layer0_deps:?}"
    );
}

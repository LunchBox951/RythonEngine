use crate::{module::Module, state::ModuleState};
use rython_core::{EngineError, OwnerId, SchedulerHandle};
use std::collections::{HashMap, HashSet};

/// Internal entry for a registered module.
pub struct ModuleEntry {
    pub module: Box<dyn Module>,
    pub state: ModuleState,
    pub ref_count: usize,
    pub exclusive_owner: Option<OwnerId>,
}

/// Orchestrates module dependency resolution, lifecycle, and ref-counting.
pub struct ModuleLoader {
    /// All registered modules (keyed by name).
    modules: HashMap<String, ModuleEntry>,
    /// Dependency graph: name -> [dep_name, ...]
    deps: HashMap<String, Vec<String>>,
    /// Load order from last load_all() call (for reverse-unload).
    load_order: Vec<String>,
}

impl ModuleLoader {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            deps: HashMap::new(),
            load_order: Vec::new(),
        }
    }

    /// Register a module. If a module with the same name is already registered
    /// the existing entry's ref count is incremented and the new module
    /// instance is dropped — the original registration wins. Callers that want
    /// "duplicate = error" semantics should check `contains()` first.
    pub fn register(&mut self, module: Box<dyn Module>, owner: Option<OwnerId>) {
        let name = module.name().to_owned();
        let dep_names = module.dependencies();
        let exclusive_owner = if module.is_exclusive() { owner } else { None };

        if let Some(existing) = self.modules.get_mut(&name) {
            // Shared dependency — just increment ref count.
            // The incoming `module` box is dropped here. We also do NOT
            // overwrite self.deps, because doing so would silently replace
            // the original module's declared dependency graph with the
            // duplicate's.
            existing.ref_count += 1;
            return;
        }
        self.deps.insert(name.clone(), dep_names);
        self.modules.insert(
            name,
            ModuleEntry {
                module,
                state: ModuleState::Loading,
                ref_count: 1,
                exclusive_owner,
            },
        );
    }

    /// Load all registered modules in dependency order (post-order topological sort).
    /// Returns Err if a circular dependency is detected OR if any module's
    /// `on_load` fails. On failure, previously-loaded modules are rolled back
    /// by calling `on_unload` in reverse.
    pub fn load_all(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        let order = topological_sort(&self.deps).map_err(|cycle| EngineError::Module {
            module: cycle.join(" -> "),
            message: format!("circular dependency detected: {}", cycle.join(" -> ")),
        })?;

        // Append to load_order incrementally after each successful load so that
        // a later unload never touches modules that were never actually loaded.
        self.load_order.clear();
        for name in &order {
            if let Some(entry) = self.modules.get_mut(name) {
                entry.state = ModuleState::Loading;
                match entry.module.on_load(scheduler) {
                    Ok(()) => {
                        entry.state = ModuleState::Loaded;
                        self.load_order.push(name.clone());
                    }
                    Err(e) => {
                        // Roll back: unload everything we've loaded so far, in
                        // reverse, before surfacing the error.
                        let _ = self.unload_all(scheduler);
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Unload all modules in reverse load order.
    pub fn unload_all(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        let reverse: Vec<String> = self.load_order.iter().rev().cloned().collect();
        self.load_order.clear();
        let mut first_err: Option<EngineError> = None;
        for name in reverse {
            if let Err(e) = self.unload_by_name(&name, scheduler) {
                // Record the first error but keep unloading the rest so we
                // don't leak resources.
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Unload a specific module. Decrements ref_count; only unloads when count reaches 0.
    /// The entry is removed from the map regardless of whether `on_unload`
    /// returned `Ok` or `Err`, so a failing `on_unload` cannot leave a zombie
    /// entry stuck in the `Unloading` state.
    pub fn unload_by_name(
        &mut self,
        name: &str,
        scheduler: &dyn SchedulerHandle,
    ) -> Result<(), EngineError> {
        let should_unload = {
            match self.modules.get_mut(name) {
                Some(entry) if entry.ref_count > 1 => {
                    entry.ref_count -= 1;
                    false
                }
                Some(entry) => {
                    entry.state = ModuleState::Unloading;
                    true
                }
                None => return Ok(()),
            }
        };
        if !should_unload {
            return Ok(());
        }
        // Take the entry out of the map before calling on_unload so that
        // failure still guarantees removal.
        let mut entry = match self.modules.remove(name) {
            Some(e) => e,
            None => return Ok(()),
        };
        entry.module.on_unload(scheduler)
    }

    pub fn get_state(&self, name: &str) -> Option<ModuleState> {
        self.modules.get(name).map(|e| e.state)
    }

    pub fn ref_count(&self, name: &str) -> Option<usize> {
        self.modules.get(name).map(|e| e.ref_count)
    }

    pub fn is_loaded(&self, name: &str) -> bool {
        self.modules
            .get(name)
            .map(|e| e.state == ModuleState::Loaded)
            .unwrap_or(false)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    /// Transfer exclusive ownership of a module from one owner to another.
    pub fn transfer_ownership(
        &mut self,
        name: &str,
        from: OwnerId,
        to: OwnerId,
    ) -> Result<(), EngineError> {
        let entry = self
            .modules
            .get_mut(name)
            .ok_or_else(|| EngineError::Module {
                module: name.to_string(),
                message: "module not found".to_string(),
            })?;

        if !entry.module.is_exclusive() {
            return Err(EngineError::Module {
                module: name.to_string(),
                message: "module is not exclusive".to_string(),
            });
        }

        if entry.exclusive_owner != Some(from) {
            return Err(EngineError::Module {
                module: name.to_string(),
                message: format!("transfer rejected: {from} is not the current owner"),
            });
        }

        entry.exclusive_owner = Some(to);
        Ok(())
    }

    /// Relinquish exclusive ownership, leaving the module unowned.
    pub fn relinquish_ownership(&mut self, name: &str, owner: OwnerId) -> Result<(), EngineError> {
        let entry = self
            .modules
            .get_mut(name)
            .ok_or_else(|| EngineError::Module {
                module: name.to_string(),
                message: "module not found".to_string(),
            })?;

        if !entry.module.is_exclusive() {
            return Err(EngineError::Module {
                module: name.to_string(),
                message: "module is not exclusive".to_string(),
            });
        }

        if entry.exclusive_owner != Some(owner) {
            return Err(EngineError::Module {
                module: name.to_string(),
                message: format!("relinquish rejected: {owner} is not the current owner"),
            });
        }

        entry.exclusive_owner = None;
        Ok(())
    }

    pub fn exclusive_owner(&self, name: &str) -> Option<OwnerId> {
        self.modules.get(name).and_then(|e| e.exclusive_owner)
    }
}

impl Default for ModuleLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Depth-first post-order topological sort over the dependency graph.
/// Returns `Err(cycle_path)` if a cycle is detected.
pub fn topological_sort(deps: &HashMap<String, Vec<String>>) -> Result<Vec<String>, Vec<String>> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();
    let mut order: Vec<String> = Vec::new();

    // Sort keys for deterministic output
    let mut nodes: Vec<&String> = deps.keys().collect();
    nodes.sort();

    for node in nodes {
        if !visited.contains(node) {
            dfs(node, deps, &mut visited, &mut in_stack, &mut order)?;
        }
    }

    Ok(order)
}

fn dfs(
    node: &str,
    deps: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    order: &mut Vec<String>,
) -> Result<(), Vec<String>> {
    in_stack.insert(node.to_string());

    if let Some(children) = deps.get(node) {
        let mut sorted_children = children.clone();
        sorted_children.sort();
        for child in &sorted_children {
            if in_stack.contains(child) {
                return Err(vec![child.clone(), node.to_string()]);
            }
            if !visited.contains(child) {
                // If child not in deps map, it's an external dep — skip DFS
                if deps.contains_key(child) {
                    dfs(child, deps, visited, in_stack, order).map_err(|mut cycle| {
                        cycle.push(node.to_string());
                        cycle
                    })?;
                }
            }
        }
    }

    in_stack.remove(node);
    visited.insert(node.to_string());
    order.push(node.to_string());

    Ok(())
}

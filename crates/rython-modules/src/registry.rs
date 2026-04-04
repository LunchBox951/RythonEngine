use crate::{module::Module, state::ModuleState};
use parking_lot::RwLock;
use rython_core::{EngineError, OwnerId};
use std::collections::HashMap;

pub(crate) struct RegistryEntry {
    pub module: Box<dyn Module>,
    pub state: ModuleState,
    pub ref_count: usize,
    /// Current exclusive owner (only set for is_exclusive() modules).
    pub exclusive_owner: Option<OwnerId>,
}

/// Thread-safe registry of loaded modules, keyed by name.
pub struct ModuleRegistry {
    inner: RwLock<HashMap<String, RegistryEntry>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert(&self, module: Box<dyn Module>, owner: Option<OwnerId>) {
        let name = module.name().to_owned();
        let exclusive_owner = if module.is_exclusive() { owner } else { None };
        let mut map = self.inner.write();
        if let Some(entry) = map.get_mut(&name) {
            entry.ref_count += 1;
        } else {
            map.insert(
                name,
                RegistryEntry {
                    module,
                    state: ModuleState::Loading,
                    ref_count: 1,
                    exclusive_owner,
                },
            );
        }
    }

    pub fn set_state(&self, name: &str, state: ModuleState) {
        if let Some(entry) = self.inner.write().get_mut(name) {
            entry.state = state;
        }
    }

    pub fn get_state(&self, name: &str) -> Option<ModuleState> {
        self.inner.read().get(name).map(|e| e.state)
    }

    pub fn ref_count(&self, name: &str) -> Option<usize> {
        self.inner.read().get(name).map(|e| e.ref_count)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.inner.read().contains_key(name)
    }

    /// Decrement ref-count. Returns true if the module should now unload (count hit 0).
    pub fn decrement_ref(&self, name: &str) -> bool {
        let mut map = self.inner.write();
        if let Some(entry) = map.get_mut(name) {
            if entry.ref_count > 0 {
                entry.ref_count -= 1;
            }
            if entry.ref_count == 0 {
                return true;
            }
        }
        false
    }

    pub fn remove(&self, name: &str) {
        self.inner.write().remove(name);
    }

    pub fn names(&self) -> Vec<String> {
        self.inner.read().keys().cloned().collect()
    }

    /// Attempt to transfer exclusive ownership of a module.
    pub fn transfer_ownership(
        &self,
        name: &str,
        from: OwnerId,
        to: OwnerId,
    ) -> Result<(), EngineError> {
        let mut map = self.inner.write();
        let entry = map.get_mut(name).ok_or_else(|| EngineError::Module {
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
                message: format!("transfer rejected: caller {from} is not the current owner"),
            });
        }

        entry.exclusive_owner = Some(to);
        Ok(())
    }

    /// Relinquish exclusive ownership (returns module to unowned state).
    pub fn relinquish_ownership(&self, name: &str, owner: OwnerId) -> Result<(), EngineError> {
        let mut map = self.inner.write();
        let entry = map.get_mut(name).ok_or_else(|| EngineError::Module {
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
                message: format!("relinquish rejected: caller {owner} is not the current owner"),
            });
        }

        entry.exclusive_owner = None;
        Ok(())
    }

    /// Check if caller is the exclusive owner.
    pub fn is_owner(&self, name: &str, caller: OwnerId) -> bool {
        self.inner
            .read()
            .get(name)
            .and_then(|e| e.exclusive_owner)
            .map(|o| o == caller)
            .unwrap_or(false)
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

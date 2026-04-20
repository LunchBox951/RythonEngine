//! Python bindings for the customizable InputMap system.
//!
//! Three layers of state live here:
//!
//!  1. Python user state — the `InputMap` subclass instance + its `InputAction`
//!     children with binding specs + callback lists. Owned entirely by the
//!     Python runtime.
//!  2. Bridge singletons — the active `PlayerController` Arc and the registry
//!     of Python maps currently pushed. Bridging between Python state and
//!     Rust `InputMappingContext`s happens on `push_map`.
//!  3. Rust engine state — `InputMappingContext` instances held by the
//!     `PlayerController`. Built by lowering from a Python `InputMap`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;
use rython_input::{
    ActionValue, EventPhase, HardwareKey, InputAction as InputActionDecl, InputActionEvent,
    InputBinding, InputMappingContext, Modifier, PlayerController, SwizzleOrder, Trigger,
    ValueKind,
};
use rython_window::{
    GamepadAxisType, GamepadButton as GpButton, KeyCode as RKeyCode, MouseAxisType,
    MouseButton as RMouseButton,
};

// ─── Active PlayerController ────────────────────────────────────────────────

static ACTIVE_PC: OnceLock<Arc<StdMutex<PlayerController>>> = OnceLock::new();

/// Publish the engine's `PlayerController` to the bridge. Called once from
/// `main.rs` during engine boot so Python module functions (`push_map` etc.)
/// can reach it.
pub fn set_active_player_controller(pc: Arc<StdMutex<PlayerController>>) {
    let _ = ACTIVE_PC.set(pc);
}

pub(crate) fn active_pc() -> Option<&'static Arc<StdMutex<PlayerController>>> {
    ACTIVE_PC.get()
}

// ─── Registry of pushed Python maps ────────────────────────────────────────
//
// When a Python `InputMap` is pushed, a copy is kept here (by id) so that
// callback dispatch (step 6) can look up the right callback lists for each
// `InputActionEvent` the controller emits.

static PUSHED_MAPS: OnceLock<Arc<Mutex<Vec<Py<InputMap>>>>> = OnceLock::new();

fn pushed_maps() -> &'static Arc<Mutex<Vec<Py<InputMap>>>> {
    PUSHED_MAPS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

// ─── Hardware enum wrappers ────────────────────────────────────────────────

macro_rules! rython_enum {
    ($name:ident, $py_name:literal { $($variant:ident),+ $(,)? }) => {
        #[pyclass(eq, eq_int, from_py_object, name = $py_name)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name {
            $($variant,)+
        }
    };
}

rython_enum!(KeyCodePy, "KeyCode" {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    Space, Enter, Escape, Tab, Backspace,
    LeftShift, RightShift, LeftControl, RightControl, LeftAlt, RightAlt,
    Up, Down, Left, Right,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
});

impl From<KeyCodePy> for RKeyCode {
    fn from(k: KeyCodePy) -> Self {
        use KeyCodePy as K;
        match k {
            K::A => RKeyCode::A, K::B => RKeyCode::B, K::C => RKeyCode::C, K::D => RKeyCode::D,
            K::E => RKeyCode::E, K::F => RKeyCode::F, K::G => RKeyCode::G, K::H => RKeyCode::H,
            K::I => RKeyCode::I, K::J => RKeyCode::J, K::K => RKeyCode::K, K::L => RKeyCode::L,
            K::M => RKeyCode::M, K::N => RKeyCode::N, K::O => RKeyCode::O, K::P => RKeyCode::P,
            K::Q => RKeyCode::Q, K::R => RKeyCode::R, K::S => RKeyCode::S, K::T => RKeyCode::T,
            K::U => RKeyCode::U, K::V => RKeyCode::V, K::W => RKeyCode::W, K::X => RKeyCode::X,
            K::Y => RKeyCode::Y, K::Z => RKeyCode::Z,
            K::Key0 => RKeyCode::Key0, K::Key1 => RKeyCode::Key1, K::Key2 => RKeyCode::Key2,
            K::Key3 => RKeyCode::Key3, K::Key4 => RKeyCode::Key4, K::Key5 => RKeyCode::Key5,
            K::Key6 => RKeyCode::Key6, K::Key7 => RKeyCode::Key7, K::Key8 => RKeyCode::Key8,
            K::Key9 => RKeyCode::Key9,
            K::Space => RKeyCode::Space, K::Enter => RKeyCode::Enter, K::Escape => RKeyCode::Escape,
            K::Tab => RKeyCode::Tab, K::Backspace => RKeyCode::Backspace,
            K::LeftShift => RKeyCode::LeftShift, K::RightShift => RKeyCode::RightShift,
            K::LeftControl => RKeyCode::LeftControl, K::RightControl => RKeyCode::RightControl,
            K::LeftAlt => RKeyCode::LeftAlt, K::RightAlt => RKeyCode::RightAlt,
            K::Up => RKeyCode::Up, K::Down => RKeyCode::Down,
            K::Left => RKeyCode::Left, K::Right => RKeyCode::Right,
            K::F1 => RKeyCode::F1, K::F2 => RKeyCode::F2, K::F3 => RKeyCode::F3,
            K::F4 => RKeyCode::F4, K::F5 => RKeyCode::F5, K::F6 => RKeyCode::F6,
            K::F7 => RKeyCode::F7, K::F8 => RKeyCode::F8, K::F9 => RKeyCode::F9,
            K::F10 => RKeyCode::F10, K::F11 => RKeyCode::F11, K::F12 => RKeyCode::F12,
        }
    }
}

rython_enum!(MouseButtonPy, "MouseButton" { Left, Right, Middle });

impl From<MouseButtonPy> for RMouseButton {
    fn from(m: MouseButtonPy) -> Self {
        match m {
            MouseButtonPy::Left => RMouseButton::Left,
            MouseButtonPy::Right => RMouseButton::Right,
            MouseButtonPy::Middle => RMouseButton::Middle,
        }
    }
}

rython_enum!(MouseAxisPy, "MouseAxis" { X, Y });

impl From<MouseAxisPy> for MouseAxisType {
    fn from(m: MouseAxisPy) -> Self {
        match m {
            MouseAxisPy::X => MouseAxisType::X,
            MouseAxisPy::Y => MouseAxisType::Y,
        }
    }
}

rython_enum!(GamepadButtonPy, "GamepadButton" {
    South, East, West, North,
    LeftBumper, RightBumper,
    LeftTriggerButton, RightTriggerButton,
    LeftStickPress, RightStickPress,
    DPadUp, DPadDown, DPadLeft, DPadRight,
    Start, Select,
});

impl From<GamepadButtonPy> for GpButton {
    fn from(b: GamepadButtonPy) -> Self {
        use GamepadButtonPy as B;
        match b {
            B::South => GpButton::South, B::East => GpButton::East,
            B::West => GpButton::West, B::North => GpButton::North,
            B::LeftBumper => GpButton::LeftBumper, B::RightBumper => GpButton::RightBumper,
            B::LeftTriggerButton => GpButton::LeftTriggerButton,
            B::RightTriggerButton => GpButton::RightTriggerButton,
            B::LeftStickPress => GpButton::LeftStickPress,
            B::RightStickPress => GpButton::RightStickPress,
            B::DPadUp => GpButton::DPadUp, B::DPadDown => GpButton::DPadDown,
            B::DPadLeft => GpButton::DPadLeft, B::DPadRight => GpButton::DPadRight,
            B::Start => GpButton::Start, B::Select => GpButton::Select,
        }
    }
}

rython_enum!(GamepadAxisPy, "GamepadAxis" {
    LeftStickX, LeftStickY, RightStickX, RightStickY, LeftTrigger, RightTrigger,
});

impl From<GamepadAxisPy> for GamepadAxisType {
    fn from(a: GamepadAxisPy) -> Self {
        use GamepadAxisPy as A;
        match a {
            A::LeftStickX => GamepadAxisType::LeftStickX,
            A::LeftStickY => GamepadAxisType::LeftStickY,
            A::RightStickX => GamepadAxisType::RightStickX,
            A::RightStickY => GamepadAxisType::RightStickY,
            A::LeftTrigger => GamepadAxisType::LeftTrigger,
            A::RightTrigger => GamepadAxisType::RightTrigger,
        }
    }
}

rython_enum!(GamepadStickPy, "GamepadStick" { LeftStick, RightStick });

impl GamepadStickPy {
    fn axes(self) -> (GamepadAxisType, GamepadAxisType) {
        match self {
            Self::LeftStick => (GamepadAxisType::LeftStickX, GamepadAxisType::LeftStickY),
            Self::RightStick => (GamepadAxisType::RightStickX, GamepadAxisType::RightStickY),
        }
    }
}

fn extract_hardware_key(obj: &Bound<'_, PyAny>) -> PyResult<HardwareKey> {
    if let Ok(k) = obj.extract::<KeyCodePy>() {
        return Ok(HardwareKey::Key(k.into()));
    }
    if let Ok(m) = obj.extract::<MouseButtonPy>() {
        return Ok(HardwareKey::Mouse(m.into()));
    }
    if let Ok(a) = obj.extract::<MouseAxisPy>() {
        return Ok(HardwareKey::MouseAxis(a.into()));
    }
    if let Ok(b) = obj.extract::<GamepadButtonPy>() {
        return Ok(HardwareKey::Gamepad(b.into()));
    }
    if let Ok(a) = obj.extract::<GamepadAxisPy>() {
        return Ok(HardwareKey::GamepadAxis(a.into()));
    }
    if let Ok(s) = obj.extract::<GamepadStickPy>() {
        let (x, y) = s.axes();
        return Ok(HardwareKey::GamepadStick {
            x_axis: x,
            y_axis: y,
        });
    }
    Err(PyErr::new::<PyTypeError, _>(format!(
        "expected KeyCode/MouseButton/MouseAxis/GamepadButton/GamepadAxis/GamepadStick, got {}",
        obj.get_type().name()?
    )))
}

// ─── Value conversions ────────────────────────────────────────────────────

#[pyclass(name = "ActionValue", frozen)]
pub struct ActionValuePy {
    inner: ActionValue,
}

#[pymethods]
impl ActionValuePy {
    fn as_bool(&self) -> bool {
        matches!(self.inner, ActionValue::Button(true))
            || matches!(self.inner, ActionValue::Axis1D(v) if v.abs() > 0.0)
    }

    fn as_float(&self) -> f32 {
        match self.inner {
            ActionValue::Button(b) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
            ActionValue::Axis1D(v) => v,
            ActionValue::Axis2D([x, y]) => (x * x + y * y).sqrt(),
            ActionValue::Axis3D([x, y, z]) => (x * x + y * y + z * z).sqrt(),
        }
    }

    fn as_vec2(&self) -> (f32, f32) {
        match self.inner {
            ActionValue::Axis2D([x, y]) => (x, y),
            ActionValue::Axis1D(v) => (v, 0.0),
            ActionValue::Button(b) => (if b { 1.0 } else { 0.0 }, 0.0),
            ActionValue::Axis3D([x, y, _]) => (x, y),
        }
    }

    fn as_vec3(&self) -> (f32, f32, f32) {
        match self.inner {
            ActionValue::Axis3D([x, y, z]) => (x, y, z),
            ActionValue::Axis2D([x, y]) => (x, y, 0.0),
            ActionValue::Axis1D(v) => (v, 0.0, 0.0),
            ActionValue::Button(b) => (if b { 1.0 } else { 0.0 }, 0.0, 0.0),
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            ActionValue::Button(_) => "button",
            ActionValue::Axis1D(_) => "axis1d",
            ActionValue::Axis2D(_) => "axis2d",
            ActionValue::Axis3D(_) => "axis3d",
        }
    }

    fn __repr__(&self) -> String {
        format!("ActionValue({:?})", self.inner)
    }
}

impl ActionValuePy {
    pub fn new(inner: ActionValue) -> Self {
        Self { inner }
    }
}

fn parse_value_kind(kind: &str) -> PyResult<ValueKind> {
    match kind {
        "button" => Ok(ValueKind::Button),
        "axis1d" => Ok(ValueKind::Axis1D),
        "axis2d" => Ok(ValueKind::Axis2D),
        "axis3d" => Ok(ValueKind::Axis3D),
        _ => Err(PyErr::new::<PyValueError, _>(format!(
            "unknown value kind {kind:?} (expected button/axis1d/axis2d/axis3d)"
        ))),
    }
}

// ─── Modifier + Trigger spec wrappers ─────────────────────────────────────

#[pyclass(frozen, name = "ModifierSpec", from_py_object)]
#[derive(Clone)]
pub struct ModifierPy {
    pub inner: Modifier,
}

#[pyclass(frozen, name = "TriggerSpec", from_py_object)]
#[derive(Clone)]
pub struct TriggerPy {
    pub inner: Trigger,
}

#[pyclass(frozen, name = "Modifiers")]
pub struct ModifiersFactory;

#[pymethods]
impl ModifiersFactory {
    #[staticmethod]
    #[pyo3(signature = (x = false, y = false, z = false))]
    #[allow(non_snake_case)]
    fn Negate(x: bool, y: bool, z: bool) -> ModifierPy {
        ModifierPy {
            inner: Modifier::Negate { x, y, z },
        }
    }

    #[staticmethod]
    #[pyo3(signature = (x = 1.0, y = 1.0, z = 1.0))]
    #[allow(non_snake_case)]
    fn Scale(x: f32, y: f32, z: f32) -> ModifierPy {
        ModifierPy {
            inner: Modifier::Scale([x, y, z]),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (lower, upper = 1.0, radial = false))]
    #[allow(non_snake_case)]
    fn DeadZone(lower: f32, upper: f32, radial: bool) -> ModifierPy {
        ModifierPy {
            inner: Modifier::DeadZone {
                lower,
                upper,
                radial,
            },
        }
    }

    #[staticmethod]
    #[allow(non_snake_case)]
    fn Swizzle(order: &str) -> PyResult<ModifierPy> {
        let s = SwizzleOrder::from_str(order)
            .ok_or_else(|| PyErr::new::<PyValueError, _>(format!("unknown swizzle {order:?}")))?;
        Ok(ModifierPy {
            inner: Modifier::Swizzle(s),
        })
    }
}

#[pyclass(frozen, name = "Triggers")]
pub struct TriggersFactory;

#[pymethods]
impl TriggersFactory {
    #[staticmethod]
    #[allow(non_snake_case)]
    fn Down() -> TriggerPy {
        TriggerPy {
            inner: Trigger::down(),
        }
    }

    #[staticmethod]
    #[allow(non_snake_case)]
    fn Pressed() -> TriggerPy {
        TriggerPy {
            inner: Trigger::pressed(),
        }
    }

    #[staticmethod]
    #[allow(non_snake_case)]
    fn Released() -> TriggerPy {
        TriggerPy {
            inner: Trigger::released(),
        }
    }

    #[staticmethod]
    #[allow(non_snake_case)]
    fn Hold(threshold_seconds: f32) -> TriggerPy {
        TriggerPy {
            inner: Trigger::hold(threshold_seconds),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (max_seconds = 0.25))]
    #[allow(non_snake_case)]
    fn Tap(max_seconds: f32) -> TriggerPy {
        TriggerPy {
            inner: Trigger::tap(max_seconds),
        }
    }

    #[staticmethod]
    #[allow(non_snake_case)]
    fn Pulse(interval_seconds: f32) -> TriggerPy {
        TriggerPy {
            inner: Trigger::pulse(interval_seconds),
        }
    }

    #[staticmethod]
    #[allow(non_snake_case)]
    fn Chorded(partner: &str) -> TriggerPy {
        TriggerPy {
            inner: Trigger::chorded(partner),
        }
    }
}

// ─── InputAction + InputMap ────────────────────────────────────────────────

#[derive(Clone)]
struct BindingSpec {
    key: HardwareKey,
    modifiers: Vec<Modifier>,
    triggers: Vec<Trigger>,
}

type CallbackList = Arc<Mutex<Vec<Py<PyAny>>>>;

#[pyclass(name = "InputAction")]
pub struct InputAction {
    id: String,
    kind: ValueKind,
    bindings: Mutex<Vec<BindingSpec>>,
    on_started: CallbackList,
    on_ongoing: CallbackList,
    on_triggered: CallbackList,
    on_completed: CallbackList,
    on_canceled: CallbackList,
}

impl InputAction {
    fn new_inner(id: String, kind: ValueKind) -> Self {
        Self {
            id,
            kind,
            bindings: Mutex::new(Vec::new()),
            on_started: Arc::new(Mutex::new(Vec::new())),
            on_ongoing: Arc::new(Mutex::new(Vec::new())),
            on_triggered: Arc::new(Mutex::new(Vec::new())),
            on_completed: Arc::new(Mutex::new(Vec::new())),
            on_canceled: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn callback_list(&self, phase: EventPhase) -> CallbackList {
        let list = match phase {
            EventPhase::Started => &self.on_started,
            EventPhase::Ongoing => &self.on_ongoing,
            EventPhase::Triggered => &self.on_triggered,
            EventPhase::Completed => &self.on_completed,
            EventPhase::Canceled => &self.on_canceled,
        };
        Arc::clone(list)
    }
}

fn parse_modifier_list(list: Option<Bound<'_, PyList>>) -> PyResult<Vec<Modifier>> {
    let mut out = Vec::new();
    if let Some(l) = list {
        for item in l.iter() {
            let spec: ModifierPy = item.extract()?;
            out.push(spec.inner);
        }
    }
    Ok(out)
}

fn parse_trigger_list(list: Option<Bound<'_, PyList>>) -> PyResult<Vec<Trigger>> {
    let mut out = Vec::new();
    if let Some(l) = list {
        for item in l.iter() {
            let spec: TriggerPy = item.extract()?;
            out.push(spec.inner.clone());
        }
    }
    Ok(out)
}

#[pymethods]
impl InputAction {
    #[getter]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[getter]
    fn kind(&self) -> &'static str {
        match self.kind {
            ValueKind::Button => "button",
            ValueKind::Axis1D => "axis1d",
            ValueKind::Axis2D => "axis2d",
            ValueKind::Axis3D => "axis3d",
        }
    }

    /// Bind a single hardware key/button/axis to this action.
    #[pyo3(signature = (key, *, modifiers = None, triggers = None))]
    fn bind(
        &self,
        key: &Bound<'_, PyAny>,
        modifiers: Option<Bound<'_, PyList>>,
        triggers: Option<Bound<'_, PyList>>,
    ) -> PyResult<()> {
        let hw = extract_hardware_key(key)?;
        let mods = parse_modifier_list(modifiers)?;
        let trigs = parse_trigger_list(triggers)?;
        self.bindings.lock().push(BindingSpec {
            key: hw,
            modifiers: mods,
            triggers: trigs,
        });
        Ok(())
    }

    #[pyo3(signature = (*, up, down, left, right, modifiers = None, triggers = None))]
    fn bind_composite_2d(
        &self,
        up: KeyCodePy,
        down: KeyCodePy,
        left: KeyCodePy,
        right: KeyCodePy,
        modifiers: Option<Bound<'_, PyList>>,
        triggers: Option<Bound<'_, PyList>>,
    ) -> PyResult<()> {
        let mods = parse_modifier_list(modifiers)?;
        let trigs = parse_trigger_list(triggers)?;
        self.bindings.lock().push(BindingSpec {
            key: HardwareKey::Composite2D {
                up: up.into(),
                down: down.into(),
                left: left.into(),
                right: right.into(),
            },
            modifiers: mods,
            triggers: trigs,
        });
        Ok(())
    }

    #[pyo3(signature = (*, up, down, left, right, forward, back, modifiers = None, triggers = None))]
    #[allow(clippy::too_many_arguments)]
    fn bind_composite_3d(
        &self,
        up: KeyCodePy,
        down: KeyCodePy,
        left: KeyCodePy,
        right: KeyCodePy,
        forward: KeyCodePy,
        back: KeyCodePy,
        modifiers: Option<Bound<'_, PyList>>,
        triggers: Option<Bound<'_, PyList>>,
    ) -> PyResult<()> {
        let mods = parse_modifier_list(modifiers)?;
        let trigs = parse_trigger_list(triggers)?;
        self.bindings.lock().push(BindingSpec {
            key: HardwareKey::Composite3D {
                up: up.into(),
                down: down.into(),
                left: left.into(),
                right: right.into(),
                forward: forward.into(),
                back: back.into(),
            },
            modifiers: mods,
            triggers: trigs,
        });
        Ok(())
    }

    fn on_started(&self, callback: Py<PyAny>) {
        self.on_started.lock().push(callback);
    }

    fn on_ongoing(&self, callback: Py<PyAny>) {
        self.on_ongoing.lock().push(callback);
    }

    fn on_triggered(&self, callback: Py<PyAny>) {
        self.on_triggered.lock().push(callback);
    }

    fn on_completed(&self, callback: Py<PyAny>) {
        self.on_completed.lock().push(callback);
    }

    fn on_canceled(&self, callback: Py<PyAny>) {
        self.on_canceled.lock().push(callback);
    }

    fn __repr__(&self) -> String {
        format!("InputAction(id={:?}, kind={})", self.id, self.kind())
    }
}

#[pyclass(subclass, name = "InputMap")]
pub struct InputMap {
    name: String,
    priority: i32,
    /// Action id → action object.
    actions: Mutex<HashMap<String, Py<InputAction>>>,
    /// Declaration order so `Chorded` partners can reference earlier actions.
    action_order: Mutex<Vec<String>>,
}

#[pymethods]
impl InputMap {
    #[new]
    #[pyo3(signature = (name = "default", priority = 0))]
    fn new(name: &str, priority: i32) -> Self {
        Self {
            name: name.to_owned(),
            priority,
            actions: Mutex::new(HashMap::new()),
            action_order: Mutex::new(Vec::new()),
        }
    }

    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[getter]
    fn priority(&self) -> i32 {
        self.priority
    }

    /// Declare a new action and return its `InputAction` handle.
    fn action(&self, py: Python<'_>, id: &str, kind: &str) -> PyResult<Py<InputAction>> {
        let value_kind = parse_value_kind(kind)?;
        let action_obj = Py::new(py, InputAction::new_inner(id.to_owned(), value_kind))?;
        let mut map = self.actions.lock();
        let mut order = self.action_order.lock();
        if !map.contains_key(id) {
            order.push(id.to_owned());
        }
        map.insert(id.to_owned(), action_obj.clone_ref(py));
        Ok(action_obj)
    }

    fn __repr__(&self) -> String {
        format!(
            "InputMap(name={:?}, priority={}, actions={})",
            self.name,
            self.priority,
            self.actions.lock().len()
        )
    }
}

impl InputMap {
    /// Build a Rust `InputMappingContext` from this map's Python-declared
    /// actions + bindings. Called at `push_map` time.
    pub(crate) fn build_context(&self, py: Python<'_>) -> InputMappingContext {
        let mut ctx = InputMappingContext::new(self.name.clone(), self.priority);
        let order = self.action_order.lock().clone();
        let actions = self.actions.lock();
        for id in &order {
            if let Some(action) = actions.get(id) {
                let action_ref = action.bind(py).borrow();
                ctx.add_action(InputActionDecl::new(
                    action_ref.id.clone(),
                    action_ref.kind,
                ));
                let bindings = action_ref.bindings.lock();
                for spec in bindings.iter() {
                    let binding = InputBinding {
                        key: spec.key.clone(),
                        modifiers: spec.modifiers.clone(),
                        triggers: spec.triggers.clone(),
                    };
                    ctx.add_binding(&action_ref.id, binding);
                }
            }
        }
        ctx
    }

}

// ─── Module-level functions ───────────────────────────────────────────────

fn pc_or_err() -> PyResult<&'static Arc<StdMutex<PlayerController>>> {
    active_pc().ok_or_else(|| {
        PyErr::new::<PyRuntimeError, _>(
            "PlayerController is not initialised — call rython.input.push_map() after engine boot",
        )
    })
}

/// Push an `InputMap` instance onto the controller's context stack.
pub(crate) fn push_map(py: Python<'_>, map: Py<InputMap>) -> PyResult<()> {
    let pc_arc = pc_or_err()?;
    let ctx = {
        let map_ref = map.bind(py).borrow();
        map_ref.build_context(py)
    };
    {
        let mut pc = pc_arc.lock().unwrap_or_else(|p| p.into_inner());
        pc.push_context(ctx);
    }
    pushed_maps().lock().push(map.clone_ref(py));
    Ok(())
}

/// Pop a previously-pushed `InputMap` by id (name).
pub(crate) fn pop_map(py: Python<'_>, id: &str) -> PyResult<()> {
    let pc_arc = pc_or_err()?;
    {
        let mut pc = pc_arc.lock().unwrap_or_else(|p| p.into_inner());
        pc.pop_context(id);
    }
    let mut pushed = pushed_maps().lock();
    pushed.retain(|m| {
        let b = m.bind(py).borrow();
        b.name != id
    });
    Ok(())
}

/// Remove every pushed map.
pub(crate) fn clear_maps(py: Python<'_>) -> PyResult<()> {
    let pc_arc = pc_or_err()?;
    {
        let mut pc = pc_arc.lock().unwrap_or_else(|p| p.into_inner());
        pc.clear_contexts();
    }
    pushed_maps().lock().clear();
    let _ = py;
    Ok(())
}

/// Ids of pushed maps in priority-descending order.
pub(crate) fn active_maps() -> PyResult<Vec<String>> {
    let pc_arc = pc_or_err()?;
    let pc = pc_arc.lock().unwrap_or_else(|p| p.into_inner());
    Ok(pc.active_contexts())
}

/// Replace the hardware key at `(map_id, action_id, binding_index)` with `new_key`.
pub(crate) fn rebind(
    map_id: &str,
    action_id: &str,
    binding_index: usize,
    new_key: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let pc_arc = pc_or_err()?;
    let hw = extract_hardware_key(new_key)?;
    let mut pc = pc_arc.lock().unwrap_or_else(|p| p.into_inner());
    let ctx = pc
        .context_mut(map_id)
        .ok_or_else(|| PyErr::new::<PyValueError, _>(format!("no map {map_id:?}")))?;
    let binding = ctx
        .binding_mut(action_id, binding_index)
        .ok_or_else(|| {
            PyErr::new::<PyValueError, _>(format!(
                "no binding at ({action_id:?}, {binding_index})"
            ))
        })?;
    binding.key = hw;
    Ok(())
}

// ─── Callback dispatch ────────────────────────────────────────────────────

/// Dispatch an already-drained event vector to the Python callback lists
/// attached to pushed maps. Callers (main.rs step 6) typically drain once
/// and then fan the same vector into both scene-bus emission and this
/// callback dispatcher.
pub fn dispatch_input_events(py: Python<'_>, events: Vec<InputActionEvent>) {
    if events.is_empty() {
        return;
    }
    dispatch_events_with_gil(py, events);
}

fn dispatch_events_with_gil(py: Python<'_>, events: Vec<InputActionEvent>) {
    let maps: Vec<Py<InputMap>> = pushed_maps()
        .lock()
        .iter()
        .map(|m| m.clone_ref(py))
        .collect();
    for ev in events {
        for map_ref in &maps {
            let map = map_ref.bind(py).borrow();
            let action: Option<Py<InputAction>> = {
                let actions = map.actions.lock();
                actions.get(&ev.action).map(|a| a.clone_ref(py))
            };
            drop(map);
            if let Some(action_obj) = action {
                dispatch_to_action(py, &action_obj, &ev);
            }
        }
    }
}

fn dispatch_to_action(py: Python<'_>, action: &Py<InputAction>, ev: &InputActionEvent) {
    let action_ref = action.bind(py).borrow();
    let list = action_ref.callback_list(ev.phase);
    // Release the action's borrow before calling — callbacks may introspect it.
    drop(action_ref);
    let callbacks: Vec<Py<PyAny>> = {
        let guard = list.lock();
        guard.iter().map(|c| c.clone_ref(py)).collect()
    };
    if callbacks.is_empty() {
        return;
    }
    let value_py = Py::new(py, ActionValuePy::new(ev.value)).ok();
    for cb in &callbacks {
        let result = match &value_py {
            Some(v) => cb.bind(py).call1((v,)),
            None => cb.bind(py).call0(),
        };
        if let Err(e) = result {
            e.print_and_set_sys_last_vars(py);
        }
    }
}

// ─── Registration ─────────────────────────────────────────────────────────

pub fn register(py: Python<'_>, rython: &Bound<'_, PyModule>) -> PyResult<()> {
    rython.add_class::<KeyCodePy>()?;
    rython.add_class::<MouseButtonPy>()?;
    rython.add_class::<MouseAxisPy>()?;
    rython.add_class::<GamepadButtonPy>()?;
    rython.add_class::<GamepadAxisPy>()?;
    rython.add_class::<GamepadStickPy>()?;
    rython.add_class::<ModifierPy>()?;
    rython.add_class::<TriggerPy>()?;
    rython.add_class::<ActionValuePy>()?;
    rython.add_class::<InputAction>()?;
    rython.add_class::<InputMap>()?;

    let mods = Py::new(py, ModifiersFactory)?;
    rython.add("Modifiers", mods)?;
    let trigs = Py::new(py, TriggersFactory)?;
    rython.add("Triggers", trigs)?;
    Ok(())
}

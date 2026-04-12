use crate::widget::WidgetId;

/// Easing function applied to a tween's normalized time [0, 1].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingFn {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bounce,
    Elastic,
}

/// Apply an easing function to normalized time t in [0.0, 1.0].
pub fn apply_easing(easing: EasingFn, t: f32) -> f32 {
    match easing {
        EasingFn::Linear => t,
        EasingFn::EaseIn => t * t,
        EasingFn::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        EasingFn::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
        EasingFn::Bounce => {
            const N1: f32 = 7.5625;
            const D1: f32 = 2.75;
            let mut t = t;
            if t < 1.0 / D1 {
                N1 * t * t
            } else if t < 2.0 / D1 {
                t -= 1.5 / D1;
                N1 * t * t + 0.75
            } else if t < 2.5 / D1 {
                t -= 2.25 / D1;
                N1 * t * t + 0.9375
            } else {
                t -= 2.625 / D1;
                N1 * t * t + 0.984375
            }
        }
        EasingFn::Elastic => {
            if t == 0.0 || t == 1.0 {
                t
            } else {
                let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
    }
}

/// A single property tween definition (used for sequential chains).
#[derive(Debug, Clone)]
pub struct TweenDef {
    pub property: String,
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub easing: EasingFn,
}

/// A standalone tween that can be sampled directly (used in unit tests).
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub easing: EasingFn,
    done: bool,
}

impl Tween {
    pub fn new(from: f32, to: f32, duration: f32, easing: EasingFn) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: 0.0,
            easing,
            done: false,
        }
    }

    /// Sample at normalized time t in [0.0, 1.0].
    pub fn sample(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let eased = apply_easing(self.easing, t);
        self.from + (self.to - self.from) * eased
    }

    /// Advance by dt seconds and return the current interpolated value.
    pub fn advance(&mut self, dt: f32) -> f32 {
        if self.done {
            return self.to;
        }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.done = true;
        }
        let t = if self.duration > 0.0 {
            self.elapsed / self.duration
        } else {
            1.0
        };
        self.sample(t)
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

// ─── Internal animation items ─────────────────────────────────────────────────

struct ActiveTween {
    widget_id: WidgetId,
    property: String,
    from: f32,
    to: f32,
    duration: f32,
    elapsed: f32,
    easing: EasingFn,
    done: bool,
}

impl ActiveTween {
    fn new(
        widget_id: WidgetId,
        property: String,
        from: f32,
        to: f32,
        duration: f32,
        easing: EasingFn,
    ) -> Self {
        Self {
            widget_id,
            property,
            from,
            to,
            duration,
            elapsed: 0.0,
            easing,
            done: false,
        }
    }

    fn advance(&mut self, dt: f32) -> f32 {
        if self.done {
            return self.to;
        }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.done = true;
        }
        let t = if self.duration > 0.0 {
            self.elapsed / self.duration
        } else {
            1.0
        };
        let eased = apply_easing(self.easing, t);
        self.from + (self.to - self.from) * eased
    }
}

struct SequentialAnim {
    widget_id: WidgetId,
    steps: Vec<TweenDef>,
    current: usize,
    step_elapsed: f32,
}

impl SequentialAnim {
    fn tick(&mut self, mut dt: f32) -> Vec<(WidgetId, String, f32)> {
        let mut updates = Vec::new();
        loop {
            if self.current >= self.steps.len() {
                break;
            }
            let step = &self.steps[self.current];
            // Zero- or negative-duration steps complete instantly with no
            // division-by-zero risk in the `else` branch below.
            if step.duration <= 0.0 {
                updates.push((self.widget_id, step.property.clone(), step.to));
                self.step_elapsed = 0.0;
                self.current += 1;
                if dt <= 0.0 {
                    break;
                }
                continue;
            }
            self.step_elapsed += dt;
            if self.step_elapsed >= step.duration {
                // Emit final value, advance to next step, carry over remaining dt
                updates.push((self.widget_id, step.property.clone(), step.to));
                dt = self.step_elapsed - step.duration;
                self.step_elapsed = 0.0;
                self.current += 1;
                if dt <= 0.0 {
                    break;
                }
            } else {
                let t = self.step_elapsed / step.duration;
                let eased = apply_easing(step.easing, t);
                let v = step.from + (step.to - step.from) * eased;
                updates.push((self.widget_id, step.property.clone(), v));
                break;
            }
        }
        updates
    }

    fn is_done(&self) -> bool {
        self.current >= self.steps.len()
    }
}

enum AnimItem {
    Single(ActiveTween),
    Sequential(SequentialAnim),
}

// ─── UIAnimator ───────────────────────────────────────────────────────────────

/// Manages all active animations for the UI system.
#[derive(Default)]
pub struct UIAnimator {
    items: Vec<AnimItem>,
}

impl UIAnimator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a single-property tween.
    pub fn start_tween(
        &mut self,
        widget_id: WidgetId,
        property: &str,
        from: f32,
        to: f32,
        duration: f32,
        easing: EasingFn,
    ) {
        self.items.push(AnimItem::Single(ActiveTween::new(
            widget_id,
            property.to_string(),
            from,
            to,
            duration,
            easing,
        )));
    }

    /// Start a sequential animation chain: steps run one after another.
    pub fn start_sequence(&mut self, widget_id: WidgetId, steps: Vec<TweenDef>) {
        if steps.is_empty() {
            return;
        }
        self.items.push(AnimItem::Sequential(SequentialAnim {
            widget_id,
            steps,
            current: 0,
            step_elapsed: 0.0,
        }));
    }

    /// Tick all animations by dt seconds. Returns (widget_id, property, value) updates
    /// to be applied by UIManager.
    pub fn tick(&mut self, dt: f32) -> Vec<(WidgetId, String, f32)> {
        let mut updates = Vec::new();
        let mut completed = Vec::new();

        for (i, item) in self.items.iter_mut().enumerate() {
            match item {
                AnimItem::Single(t) => {
                    let v = t.advance(dt);
                    updates.push((t.widget_id, t.property.clone(), v));
                    if t.done {
                        completed.push(i);
                    }
                }
                AnimItem::Sequential(seq) => {
                    let seq_updates = seq.tick(dt);
                    updates.extend(seq_updates);
                    if seq.is_done() {
                        completed.push(i);
                    }
                }
            }
        }

        for i in completed.into_iter().rev() {
            self.items.swap_remove(i);
        }

        updates
    }

    /// True if there are any active animations targeting the given widget.
    pub fn has_active_for(&self, widget_id: WidgetId) -> bool {
        self.items.iter().any(|item| match item {
            AnimItem::Single(t) => t.widget_id == widget_id,
            AnimItem::Sequential(s) => s.widget_id == widget_id,
        })
    }
}

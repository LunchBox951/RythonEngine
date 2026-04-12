#![deny(warnings)]

pub mod animator;
pub mod commands;
pub mod manager;
pub mod theme;
pub mod widget;

pub use animator::{EasingFn, Tween, TweenDef, UIAnimator};
pub use commands::UICmd;
pub use manager::UIManager;
pub use theme::Theme;
pub use widget::{LayoutDir, Widget, WidgetId, WidgetKind, WidgetState};

// ─── Acceptance Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rython_renderer::Color;
    use std::sync::{Arc, Mutex};

    fn ui() -> UIManager {
        UIManager::new(Theme::default())
    }

    // ── T-UI-01: Widget Creation ──────────────────────────────────────────────

    #[test]
    fn t_ui_01_widget_creation() {
        let mut ui = ui();
        let id = ui.create_label("Hello", 0.5, 0.3, 0.2, 0.05);
        let w = ui.get_widget(id);
        assert!(id > 0, "widget should have a non-zero ID");
        assert_eq!(w.text, "Hello");
        assert!((w.x - 0.5).abs() < f32::EPSILON);
        assert!((w.y - 0.3).abs() < f32::EPSILON);
        assert!(w.visible, "widgets are visible by default");

        // Second widget gets a different ID
        let id2 = ui.create_label("World", 0.1, 0.1, 0.2, 0.05);
        assert_ne!(id, id2, "widget IDs must be unique");
    }

    // ── T-UI-02: Widget Tree — Parent-Child ───────────────────────────────────

    #[test]
    fn t_ui_02_widget_tree_parent_child() {
        let mut ui = ui();
        let panel = ui.create_panel(0.3, 0.2, 0.4, 0.6);
        let btn = ui.create_button("OK", 0.05, 0.05, 0.2, 0.05);
        ui.add_child(panel, btn);
        ui.compute_layout();

        assert_eq!(ui.get_widget(btn).parent, Some(panel));
        assert!(ui.get_widget(panel).children.contains(&btn));

        // Button's absolute position = panel.abs + button.relative
        let w_btn = ui.get_widget(btn);
        let w_panel = ui.get_widget(panel);
        assert!((w_btn.abs_x - (w_panel.abs_x + 0.05)).abs() < 1e-4);
        assert!((w_btn.abs_y - (w_panel.abs_y + 0.05)).abs() < 1e-4);
    }

    // ── T-UI-03: Widget Tree — Visibility Cascade ─────────────────────────────

    #[test]
    fn t_ui_03_visibility_cascade() {
        let mut ui = ui();
        let panel = ui.create_panel(0.3, 0.2, 0.4, 0.6);
        let btn1 = ui.create_button_child("A", panel, 0.0, 0.0, 0.2, 0.05);
        let btn2 = ui.create_button_child("B", panel, 0.0, 0.0, 0.2, 0.05);

        // All visible initially
        assert!(ui.is_visible(panel));
        assert!(ui.is_visible(btn1));
        assert!(ui.is_visible(btn2));

        // Hiding parent cascades
        ui.hide(panel);
        assert!(!ui.is_visible(panel));
        assert!(!ui.is_visible(btn1), "btn1 hidden via parent");
        assert!(!ui.is_visible(btn2), "btn2 hidden via parent");

        // Showing parent restores visibility
        ui.show(panel);
        assert!(ui.is_visible(panel));
        assert!(ui.is_visible(btn1));
        assert!(ui.is_visible(btn2));
    }

    // ── T-UI-04: Vertical Stack Layout ────────────────────────────────────────

    #[test]
    fn t_ui_04_vertical_stack_layout() {
        let mut ui = ui();
        let panel = ui.create_panel(0.1, 0.2, 0.4, 0.6);
        ui.set_layout(panel, LayoutDir::Vertical, 0.02, 0.01);

        let btn1 = ui.create_button_child("A", panel, 0.0, 0.0, 0.4, 0.05);
        let btn2 = ui.create_button_child("B", panel, 0.0, 0.0, 0.4, 0.05);
        let btn3 = ui.create_button_child("C", panel, 0.0, 0.0, 0.4, 0.05);
        ui.compute_layout();

        let panel_y = ui.get_widget(panel).abs_y;
        let padding = 0.01_f32;
        let spacing = 0.02_f32;
        let btn_h = 0.05_f32;

        let y1 = ui.get_widget(btn1).abs_y;
        let y2 = ui.get_widget(btn2).abs_y;
        let y3 = ui.get_widget(btn3).abs_y;

        let expected_y1 = panel_y + padding;
        let expected_y2 = expected_y1 + btn_h + spacing;
        let expected_y3 = expected_y2 + btn_h + spacing;

        assert!(
            (y1 - expected_y1).abs() < 1e-4,
            "btn1 y={y1} expected={expected_y1}"
        );
        assert!(
            (y2 - expected_y2).abs() < 1e-4,
            "btn2 y={y2} expected={expected_y2}"
        );
        assert!(
            (y3 - expected_y3).abs() < 1e-4,
            "btn3 y={y3} expected={expected_y3}"
        );
    }

    // ── T-UI-05: Horizontal Stack Layout ─────────────────────────────────────

    #[test]
    fn t_ui_05_horizontal_stack_layout() {
        let mut ui = ui();
        let panel = ui.create_panel(0.1, 0.2, 0.6, 0.1);
        ui.set_layout(panel, LayoutDir::Horizontal, 0.02, 0.0);

        let btn1 = ui.create_button_child("A", panel, 0.0, 0.0, 0.1, 0.05);
        let btn2 = ui.create_button_child("B", panel, 0.0, 0.0, 0.1, 0.05);
        let btn3 = ui.create_button_child("C", panel, 0.0, 0.0, 0.1, 0.05);
        ui.compute_layout();

        let panel_x = ui.get_widget(panel).abs_x;
        let spacing = 0.02_f32;
        let btn_w = 0.1_f32;

        let x1 = ui.get_widget(btn1).abs_x;
        let x2 = ui.get_widget(btn2).abs_x;
        let x3 = ui.get_widget(btn3).abs_x;

        let expected_x1 = panel_x;
        let expected_x2 = expected_x1 + btn_w + spacing;
        let expected_x3 = expected_x2 + btn_w + spacing;

        assert!(
            (x1 - expected_x1).abs() < 1e-4,
            "btn1 x={x1} expected={expected_x1}"
        );
        assert!(
            (x2 - expected_x2).abs() < 1e-4,
            "btn2 x={x2} expected={expected_x2}"
        );
        assert!(
            (x3 - expected_x3).abs() < 1e-4,
            "btn3 x={x3} expected={expected_x3}"
        );

        // Gaps between buttons are exactly spacing
        assert!((x2 - (x1 + btn_w) - spacing).abs() < 1e-4);
        assert!((x3 - (x2 + btn_w) - spacing).abs() < 1e-4);
    }

    // ── T-UI-06: Theme Application ────────────────────────────────────────────

    #[test]
    fn t_ui_06_theme_application() {
        let theme = Theme {
            button_color: Color::rgb(50, 50, 70),
            ..Theme::default()
        };
        let mut ui = UIManager::new(theme);

        // Button without explicit color inherits theme
        let btn_themed = ui.create_button("Play", 0.5, 0.5, 0.2, 0.06);
        assert_eq!(ui.effective_color(btn_themed), Color::rgb(50, 50, 70));

        // Button with explicit color overrides theme
        let btn_explicit =
            ui.create_button_colored("Quit", 0.5, 0.6, 0.2, 0.06, Color::rgb(100, 0, 0));
        assert_eq!(ui.effective_color(btn_explicit), Color::rgb(100, 0, 0));
    }

    // ── T-UI-07: Theme Switch at Runtime ─────────────────────────────────────

    #[test]
    fn t_ui_07_theme_switch_at_runtime() {
        let theme_a = Theme {
            text_color: Color::rgb(255, 255, 255),
            ..Theme::default()
        };
        let mut ui = UIManager::new(theme_a);

        // Label without explicit color
        let label = ui.create_label("Hi", 0.5, 0.3, 0.2, 0.05);
        assert_eq!(ui.effective_text_color(label), Color::rgb(255, 255, 255));

        // Label with explicit color
        let label_explicit =
            ui.create_label_colored("Bye", 0.5, 0.4, 0.2, 0.05, Color::rgb(128, 64, 64));

        // Switch theme
        ui.set_theme(Theme {
            text_color: Color::rgb(0, 0, 0),
            ..Theme::default()
        });

        // Themed label follows new theme
        assert_eq!(ui.effective_text_color(label), Color::rgb(0, 0, 0));
        // Explicit label is unaffected
        assert_eq!(
            ui.effective_text_color(label_explicit),
            Color::rgb(128, 64, 64)
        );
    }

    // ── T-UI-08: Animation — Linear Tween ────────────────────────────────────

    #[test]
    fn t_ui_08_animation_linear_tween() {
        let tween = Tween::new(0.0, 1.0, 1.0, EasingFn::Linear);
        for (t, expected) in [
            (0.0f32, 0.0f32),
            (0.25, 0.25),
            (0.5, 0.5),
            (0.75, 0.75),
            (1.0, 1.0),
        ] {
            let v = tween.sample(t);
            assert!(
                (v - expected).abs() < 0.01,
                "linear t={t}: got {v}, expected {expected}"
            );
        }
    }

    // ── T-UI-09: Animation — Ease In ─────────────────────────────────────────

    #[test]
    fn t_ui_09_animation_ease_in() {
        let tween = Tween::new(0.0, 1.0, 1.0, EasingFn::EaseIn);
        let v = tween.sample(0.5);
        assert!(
            v < 0.5,
            "ease_in starts slow, at t=0.5 value {v} should be < 0.5"
        );
        assert!(
            (v - 0.25).abs() < 0.01,
            "quadratic ease-in: t^2 at t=0.5 = 0.25, got {v}"
        );
    }

    // ── T-UI-10: Animation — Ease Out ────────────────────────────────────────

    #[test]
    fn t_ui_10_animation_ease_out() {
        let tween = Tween::new(0.0, 1.0, 1.0, EasingFn::EaseOut);
        let v = tween.sample(0.5);
        assert!(
            v > 0.5,
            "ease_out starts fast, at t=0.5 value {v} should be > 0.5"
        );
        assert!(
            (v - 0.75).abs() < 0.01,
            "quadratic ease-out at t=0.5 = 0.75, got {v}"
        );
    }

    // ── T-UI-11: Animation Completion ────────────────────────────────────────

    #[test]
    fn t_ui_11_animation_completion() {
        let mut ui = ui();
        let label = ui.create_label("", 0.5, 0.5, 0.0, 0.0);
        ui.get_widget_mut(label).alpha = 0.0;

        ui.start_tween(label, "alpha", 0.0, 1.0, 0.5, EasingFn::Linear);

        // Advance beyond the animation duration
        ui.tick(0.5);
        ui.tick(0.5);

        let alpha = ui.get_widget(label).alpha;
        assert!(
            (alpha - 1.0).abs() < f32::EPSILON,
            "alpha should be clamped to 1.0, got {alpha}"
        );
        assert!(
            !ui.has_active_animation(label),
            "animation should be completed"
        );
    }

    // ── T-UI-12: Sequential Animation Chain ──────────────────────────────────

    #[test]
    fn t_ui_12_sequential_animation_chain() {
        let mut ui = ui();
        let wid = ui.create_label("", 0.5, 0.5, 0.0, 0.0);
        ui.get_widget_mut(wid).alpha = 0.0;

        ui.animate_sequence(
            wid,
            vec![
                TweenDef {
                    property: "alpha".to_string(),
                    from: 0.0,
                    to: 1.0,
                    duration: 0.3,
                    easing: EasingFn::Linear,
                },
                TweenDef {
                    property: "position_y".to_string(),
                    from: 0.5,
                    to: 0.2,
                    duration: 0.3,
                    easing: EasingFn::Linear,
                },
            ],
        );

        // t = 0.15: first step at 50% → alpha ≈ 0.5, position_y unchanged
        ui.tick(0.15);
        let alpha = ui.get_widget(wid).alpha;
        let pos_y = ui.get_widget(wid).y;
        assert!((alpha - 0.5).abs() < 0.05, "alpha at t=0.15: got {alpha}");
        assert!(
            (pos_y - 0.5).abs() < 0.01,
            "position_y should still be 0.5 at t=0.15, got {pos_y}"
        );

        // t = 0.30: first step finishes
        ui.tick(0.15);
        // t = 0.45: second step 0.15/0.3 = 50% → pos_y = 0.5 + (0.2-0.5)*0.5 = 0.35
        ui.tick(0.15);
        let alpha = ui.get_widget(wid).alpha;
        let pos_y = ui.get_widget(wid).y;
        assert!((alpha - 1.0).abs() < 0.01, "alpha at t=0.45: got {alpha}");
        assert!(
            (pos_y - 0.35).abs() < 0.05,
            "position_y at t=0.45: got {pos_y}"
        );

        // t ≥ 0.6: both at final values
        ui.tick(0.3);
        let pos_y_final = ui.get_widget(wid).y;
        assert!(
            (pos_y_final - 0.2).abs() < 0.01,
            "final position_y should be 0.2, got {pos_y_final}"
        );
    }

    // ── T-UI-13: Input Routing — Click on Button ──────────────────────────────

    #[test]
    fn t_ui_13_input_routing_click_on_button() {
        let mut ui = ui();
        let btn = ui.create_button("Play", 0.4, 0.4, 0.2, 0.06);
        ui.compute_layout();

        let clicked = Arc::new(Mutex::new(false));
        let clicked_clone = Arc::clone(&clicked);
        ui.set_on_click(
            btn,
            Arc::new(move || {
                *clicked_clone.lock().unwrap() = true;
            }),
        );

        // Click inside button bounds (x: 0.4..0.6, y: 0.4..0.46)
        let cb = ui.on_mouse_click(0.5, 0.43);
        assert!(cb.is_some(), "click inside button should be consumed");
        if let Some(cb) = cb {
            cb();
        }
        assert!(*clicked.lock().unwrap(), "on_click callback should fire");
    }

    // ── T-UI-14: Input Routing — Click Outside Button ─────────────────────────

    #[test]
    fn t_ui_14_input_routing_click_outside_button() {
        let mut ui = ui();
        let btn = ui.create_button("Play", 0.4, 0.4, 0.2, 0.06);
        ui.compute_layout();

        let clicked = Arc::new(Mutex::new(false));
        let clicked_clone = Arc::clone(&clicked);
        ui.set_on_click(
            btn,
            Arc::new(move || {
                *clicked_clone.lock().unwrap() = true;
            }),
        );

        // Click outside button bounds
        let cb = ui.on_mouse_click(0.1, 0.1);
        assert!(cb.is_none(), "click outside button should not be consumed");
        if let Some(cb) = cb {
            cb();
        }
        assert!(!*clicked.lock().unwrap(), "on_click should not fire");
    }

    // ── T-UI-15: Input Routing — Hover State ─────────────────────────────────

    #[test]
    fn t_ui_15_input_routing_hover_state() {
        let mut ui = ui();
        let btn = ui.create_button("Play", 0.4, 0.4, 0.2, 0.06);
        ui.compute_layout();

        assert_eq!(ui.get_widget(btn).state, WidgetState::Normal);

        // Move mouse over button
        ui.on_mouse_move(0.5, 0.43);
        assert_eq!(
            ui.get_widget(btn).state,
            WidgetState::Hover,
            "button should be hovered"
        );

        // Move mouse away
        ui.on_mouse_move(0.1, 0.1);
        assert_eq!(
            ui.get_widget(btn).state,
            WidgetState::Normal,
            "hover should clear"
        );
    }

    // ── T-UI-16: Focus and Keyboard Events ───────────────────────────────────

    #[test]
    fn t_ui_16_focus_and_keyboard_events() {
        let mut ui = ui();
        let text_input = ui.create_text_input("Enter name...", 0.5, 0.4, 0.3, 0.05);

        ui.focus(text_input);
        assert!(ui.get_widget(text_input).focused);

        // Typing characters should be consumed and appended to text
        let consumed_a = ui.on_key_press('a');
        let consumed_b = ui.on_key_press('b');
        let consumed_c = ui.on_key_press('c');

        assert!(consumed_a, "'a' should be consumed by focused TextInput");
        assert!(consumed_b);
        assert!(consumed_c);
        assert_eq!(ui.get_widget(text_input).text, "abc");
    }

    // ── T-UI-17: Tab Navigation ───────────────────────────────────────────────

    #[test]
    fn t_ui_17_tab_navigation() {
        let mut ui = ui();
        let btn1 = ui.create_button("One", 0.1, 0.1, 0.1, 0.05);
        let btn2 = ui.create_button("Two", 0.1, 0.2, 0.1, 0.05);
        let btn3 = ui.create_button("Three", 0.1, 0.3, 0.1, 0.05);

        ui.set_tab_order(vec![btn1, btn2, btn3]);
        ui.focus(btn1);

        assert!(ui.get_widget(btn1).focused);
        assert!(!ui.get_widget(btn2).focused);
        assert!(!ui.get_widget(btn3).focused);

        // First Tab: btn1 → btn2
        let next = ui.on_tab();
        assert_eq!(next, Some(btn2));
        assert!(!ui.get_widget(btn1).focused, "btn1 should lose focus");
        assert!(ui.get_widget(btn2).focused, "btn2 should gain focus");
        assert!(!ui.get_widget(btn3).focused);

        // Second Tab: btn2 → btn3
        let next = ui.on_tab();
        assert_eq!(next, Some(btn3));
        assert!(!ui.get_widget(btn2).focused);
        assert!(ui.get_widget(btn3).focused, "btn3 should gain focus");
    }

    // ── T-UI-18: Draw Command Emission ───────────────────────────────────────

    #[test]
    fn t_ui_18_draw_command_emission() {
        use rython_renderer::DrawCommand;

        let mut ui = ui();
        let panel = ui.create_panel(0.3, 0.2, 0.4, 0.6);
        let _btn = ui.create_button_child("Play", panel, 0.05, 0.05, 0.3, 0.05);
        ui.compute_layout();

        let cmds = ui.build_draw_commands();

        let has_rect = cmds.iter().any(|c| matches!(c, DrawCommand::Rect(_)));
        let has_text = cmds.iter().any(|c| matches!(c, DrawCommand::Text(_)));
        assert!(has_rect, "expected at least one DrawRect");
        assert!(has_text, "expected at least one DrawText");

        // All UI draw commands must have z >= 100.0 (rendered above typical game draw commands)
        for cmd in &cmds {
            assert!(cmd.z() >= 100.0, "UI z={} must be >= 100.0", cmd.z());
        }
    }

    // ── T-UI-19: Command Queue — Show/Hide ───────────────────────────────────

    #[test]
    fn t_ui_19_command_queue_show_hide() {
        let mut ui = ui();
        let label = ui.create_label("Test", 0.5, 0.5, 0.2, 0.05);

        assert!(ui.is_visible(label));

        // Queue hide and drain
        ui.queue_cmd(UICmd::Hide(label));
        ui.drain_commands();
        assert!(
            !ui.is_visible(label),
            "widget should be hidden after HideUICmd"
        );

        // Queue show and drain
        ui.queue_cmd(UICmd::Show(label));
        ui.drain_commands();
        assert!(
            ui.is_visible(label),
            "widget should be visible after ShowUICmd"
        );
    }

    // ── T-UI-20: save_json — structure ───────────────────────────────────────

    #[test]
    fn t_ui_20_save_json_structure() {
        let mut ui = ui();
        let panel = ui.create_panel(0.0, 0.0, 0.5, 1.0);
        let label = ui.create_label("Score: 0", 0.01, 0.01, 0.2, 0.05);
        ui.add_child(panel, label);

        let val = ui.save_json();

        // Top-level keys
        assert!(val.get("theme").is_some(), "must have theme key");
        let widgets = val["widgets"].as_array().expect("must have widgets array");

        // Panel is a root, label is a child — both appear
        assert_eq!(widgets.len(), 2);

        // Panel appears first (root before child in DFS order)
        assert_eq!(widgets[0]["kind"].as_str().unwrap(), "Panel");
        assert_eq!(widgets[1]["kind"].as_str().unwrap(), "Label");
        assert_eq!(widgets[1]["text"].as_str().unwrap(), "Score: 0");

        // Parent/child linkage
        assert!(widgets[0]["parent"].is_null(), "panel has no parent");
        assert_eq!(
            widgets[0]["children"][0].as_u64().unwrap(),
            widgets[1]["id"].as_u64().unwrap(),
            "panel's children list must reference label id"
        );
        assert_eq!(
            widgets[1]["parent"].as_u64().unwrap(),
            widgets[0]["id"].as_u64().unwrap(),
            "label parent must reference panel id"
        );
    }

    // ── T-UI-21: load_json — restore tree ────────────────────────────────────

    #[test]
    fn t_ui_21_load_json_restore_tree() {
        // Build + save
        let mut src = ui();
        let panel = src.create_panel(0.1, 0.2, 0.4, 0.6);
        let btn = src.create_button_child("OK", panel, 0.05, 0.05, 0.2, 0.05);
        let json = src.save_json();

        // Load into fresh manager
        let mut dst = UIManager::with_default_theme();
        dst.load_json(&json);

        assert_eq!(dst.widget_count(), 2, "both widgets must be restored");

        let w_panel = dst.get_widget(panel);
        assert_eq!(w_panel.kind, WidgetKind::Panel);
        assert!((w_panel.x - 0.1).abs() < f32::EPSILON);
        assert!((w_panel.y - 0.2).abs() < f32::EPSILON);
        assert!(
            w_panel.children.contains(&btn),
            "panel must reference btn as child"
        );

        let w_btn = dst.get_widget(btn);
        assert_eq!(w_btn.kind, WidgetKind::Button);
        assert_eq!(w_btn.text, "OK");
        assert_eq!(w_btn.parent, Some(panel));
    }

    // ── T-UI-22: save/load round-trip ────────────────────────────────────────

    #[test]
    fn t_ui_22_save_load_round_trip() {
        let mut src = UIManager::new(crate::Theme {
            font_size: 24,
            text_color: Color::rgb(10, 20, 30),
            ..crate::Theme::default()
        });

        let panel = src.create_panel(0.0, 0.0, 1.0, 1.0);
        src.set_layout(panel, LayoutDir::Vertical, 0.02, 0.01);
        let label = src.create_button_child("Hello", panel, 0.0, 0.0, 0.3, 0.05);
        src.hide(label);

        let json = src.save_json();

        let mut dst = UIManager::with_default_theme();
        dst.load_json(&json);

        // Theme restored
        assert_eq!(dst.theme.font_size, 24);
        assert_eq!(dst.theme.text_color, Color::rgb(10, 20, 30));

        // Layout settings restored
        let w_panel = dst.get_widget(panel);
        assert_eq!(w_panel.layout, LayoutDir::Vertical);
        assert!((w_panel.spacing - 0.02).abs() < 1e-5);
        assert!((w_panel.padding - 0.01).abs() < 1e-5);

        // Visibility restored
        assert!(
            !dst.get_widget(label).visible,
            "hidden widget stays hidden after round-trip"
        );

        // Root order preserved (only panel is a root widget)
        assert_eq!(dst.root_order(), &[panel]);

        // New widget IDs don't collide with loaded IDs
        let new_id = dst.create_label("New", 0.5, 0.5, 0.1, 0.05);
        assert_ne!(
            new_id, panel,
            "new ID must not collide with loaded panel ID"
        );
        assert_ne!(
            new_id, label,
            "new ID must not collide with loaded button ID"
        );
    }

    // ── T-UI-23: remove_widget removes subtree ───────────────────────────────

    #[test]
    fn t_ui_23_remove_widget_removes_subtree() {
        let mut ui = ui();
        let panel = ui.create_panel(0.0, 0.0, 1.0, 1.0);
        let btn = ui.create_button_child("A", panel, 0.0, 0.0, 0.1, 0.05);
        let _lbl = ui.create_button_child("B", panel, 0.0, 0.1, 0.1, 0.05);

        assert_eq!(ui.widget_count(), 3);

        ui.remove_widget(btn);

        assert_eq!(ui.widget_count(), 2, "btn removed");
        assert!(
            !ui.get_widget(panel).children.contains(&btn),
            "panel must not reference removed btn"
        );
    }

    // ── T-UI-24: remove_widget cascades to grandchildren ─────────────────────

    #[test]
    fn t_ui_24_remove_widget_cascade_grandchildren() {
        let mut ui = ui();
        let root = ui.create_panel(0.0, 0.0, 1.0, 1.0);
        let child = ui.create_button_child("Child", root, 0.0, 0.0, 0.3, 0.05);
        let grandchild = ui.create_button_child("Grand", child, 0.0, 0.0, 0.1, 0.03);

        assert_eq!(ui.widget_count(), 3);

        // Remove the intermediate child — grandchild must also be purged
        ui.remove_widget(child);

        assert_eq!(ui.widget_count(), 1, "child and grandchild both removed");
        assert!(
            !ui.get_widget(root).children.contains(&child),
            "root must not reference child"
        );
        // grandchild no longer accessible (would panic if we tried get_widget)
        assert_eq!(ui.widget_count(), 1);
        _ = grandchild; // suppress unused warning
    }

    // ── T-UI-25: EaseInOut is symmetric at t=0.5 ─────────────────────────────

    #[test]
    fn t_ui_25_easing_ease_in_out_symmetric() {
        use crate::animator::apply_easing;
        let v = apply_easing(EasingFn::EaseInOut, 0.5);
        assert!(
            (v - 0.5).abs() < 0.01,
            "EaseInOut at t=0.5 must be 0.5 (symmetric), got {v}"
        );
        // Starts slow
        let v_early = apply_easing(EasingFn::EaseInOut, 0.1);
        assert!(
            v_early < 0.1,
            "EaseInOut starts slow: at t=0.1 got {v_early}"
        );
        // Ends slow
        let v_late = apply_easing(EasingFn::EaseInOut, 0.9);
        assert!(v_late > 0.9, "EaseInOut ends slow: at t=0.9 got {v_late}");
    }

    // ── T-UI-26: Bounce easing boundary values ────────────────────────────────

    #[test]
    fn t_ui_26_easing_bounce_boundaries() {
        use crate::animator::apply_easing;
        let v0 = apply_easing(EasingFn::Bounce, 0.0);
        let v1 = apply_easing(EasingFn::Bounce, 1.0);
        assert!((v0 - 0.0).abs() < 0.01, "Bounce at t=0 must be 0, got {v0}");
        assert!((v1 - 1.0).abs() < 0.01, "Bounce at t=1 must be 1, got {v1}");
    }

    // ── T-UI-27: Elastic easing boundary values ───────────────────────────────

    #[test]
    fn t_ui_27_easing_elastic_boundaries() {
        use crate::animator::apply_easing;
        let v0 = apply_easing(EasingFn::Elastic, 0.0);
        let v1 = apply_easing(EasingFn::Elastic, 1.0);
        assert!(
            (v0 - 0.0).abs() < 0.001,
            "Elastic at t=0 must be 0, got {v0}"
        );
        assert!(
            (v1 - 1.0).abs() < 0.001,
            "Elastic at t=1 must be 1, got {v1}"
        );
    }

    // ── T-UI-28: TextInput backspace ──────────────────────────────────────────

    #[test]
    fn t_ui_28_text_input_backspace() {
        let mut ui = ui();
        let inp = ui.create_text_input("...", 0.5, 0.5, 0.3, 0.05);
        ui.focus(inp);

        ui.on_key_press('h');
        ui.on_key_press('i');
        assert_eq!(ui.get_widget(inp).text, "hi");

        // Backspace removes last char
        ui.on_key_press('\x08');
        assert_eq!(
            ui.get_widget(inp).text,
            "h",
            "backspace should remove last char"
        );

        // Backspace on single char empties string
        ui.on_key_press('\x08');
        assert_eq!(
            ui.get_widget(inp).text,
            "",
            "backspace on single char empties"
        );

        // Backspace on empty string is a no-op (no panic)
        ui.on_key_press('\x08');
        assert_eq!(ui.get_widget(inp).text, "");
    }

    // ── T-UI-29: on_key_press with no focused widget returns false ────────────

    #[test]
    fn t_ui_29_key_press_no_focus_returns_false() {
        let mut ui = ui();
        let _btn = ui.create_button("X", 0.5, 0.5, 0.1, 0.05);
        // No widget focused
        assert!(
            !ui.on_key_press('a'),
            "key press with no focus must return false"
        );
    }

    // ── T-UI-30: Tab navigation wraps around ─────────────────────────────────

    #[test]
    fn t_ui_30_tab_navigation_wrap_around() {
        let mut ui = ui();
        let btn1 = ui.create_button("One", 0.1, 0.1, 0.1, 0.05);
        let btn2 = ui.create_button("Two", 0.1, 0.2, 0.1, 0.05);
        let btn3 = ui.create_button("Three", 0.1, 0.3, 0.1, 0.05);

        ui.set_tab_order(vec![btn1, btn2, btn3]);
        ui.focus(btn3);

        // Tab from last element wraps to first
        let next = ui.on_tab();
        assert_eq!(next, Some(btn1), "tab from last must wrap to first");
        assert!(
            ui.get_widget(btn1).focused,
            "btn1 should be focused after wrap"
        );
        assert!(!ui.get_widget(btn3).focused, "btn3 should lose focus");
    }

    // ── T-UI-31: UICmd::Focus command ────────────────────────────────────────

    #[test]
    fn t_ui_31_cmd_focus() {
        let mut ui = ui();
        let inp = ui.create_text_input("hint", 0.5, 0.5, 0.3, 0.05);
        assert!(!ui.get_widget(inp).focused);

        ui.queue_cmd(UICmd::Focus(inp));
        ui.drain_commands();

        assert!(
            ui.get_widget(inp).focused,
            "widget must be focused after UICmd::Focus"
        );
    }

    // ── T-UI-32: Hidden widget excluded from draw commands ────────────────────

    #[test]
    fn t_ui_32_hidden_widget_not_drawn() {
        use rython_renderer::DrawCommand;

        let mut ui = ui();
        let label = ui.create_label("Secret", 0.5, 0.5, 0.2, 0.05);
        ui.hide(label);
        ui.compute_layout();

        let cmds = ui.build_draw_commands();
        let has_secret = cmds.iter().any(|c| {
            if let DrawCommand::Text(t) = c {
                t.text == "Secret"
            } else {
                false
            }
        });
        assert!(!has_secret, "hidden label must not appear in draw commands");
    }

    // ── T-UI-33: z-depth — children render above parents ─────────────────────

    #[test]
    fn t_ui_33_child_z_above_parent() {
        let mut ui = ui();
        let panel = ui.create_panel(0.0, 0.0, 1.0, 1.0);
        let btn = ui.create_button_child("A", panel, 0.05, 0.05, 0.2, 0.05);

        assert!(
            ui.get_widget(btn).z > ui.get_widget(panel).z,
            "child z ({}) must exceed parent z ({})",
            ui.get_widget(btn).z,
            ui.get_widget(panel).z
        );
    }

    // ── T-UI-34: Horizontal layout with non-zero padding ─────────────────────

    #[test]
    fn t_ui_34_horizontal_layout_with_padding() {
        let mut ui = ui();
        let panel = ui.create_panel(0.1, 0.2, 0.8, 0.1);
        ui.set_layout(panel, LayoutDir::Horizontal, 0.0, 0.05);

        let btn1 = ui.create_button_child("A", panel, 0.0, 0.0, 0.1, 0.05);
        let btn2 = ui.create_button_child("B", panel, 0.0, 0.0, 0.1, 0.05);
        ui.compute_layout();

        let panel_x = ui.get_widget(panel).abs_x;
        let padding = 0.05_f32;
        let btn_w = 0.1_f32;
        let spacing = 0.0_f32;

        let x1 = ui.get_widget(btn1).abs_x;
        let x2 = ui.get_widget(btn2).abs_x;

        assert!((x1 - (panel_x + padding)).abs() < 1e-4, "btn1 x={x1}");
        assert!(
            (x2 - (panel_x + padding + btn_w + spacing)).abs() < 1e-4,
            "btn2 x={x2}"
        );

        // y offset by padding too
        let panel_y = ui.get_widget(panel).abs_y;
        let y1 = ui.get_widget(btn1).abs_y;
        assert!((y1 - (panel_y + padding)).abs() < 1e-4, "btn1 y={y1}");
    }

    // ── T-UI-35: Multiple root widgets — save/load preserves roots ───────────

    #[test]
    fn t_ui_35_multiple_roots_round_trip() {
        let mut src = ui();
        let r1 = src.create_panel(0.0, 0.0, 0.5, 1.0);
        let r2 = src.create_panel(0.5, 0.0, 0.5, 1.0);
        let json = src.save_json();

        let widgets = json["widgets"].as_array().unwrap();
        assert_eq!(widgets.len(), 2, "both root panels serialized");
        assert!(widgets[0]["parent"].is_null(), "r1 has no parent");
        assert!(widgets[1]["parent"].is_null(), "r2 has no parent");

        let mut dst = UIManager::with_default_theme();
        dst.load_json(&json);

        assert_eq!(dst.widget_count(), 2);
        assert_eq!(dst.root_order(), &[r1, r2], "root order preserved");
    }

    // ── T-UI-36: save/load preserves explicit widget colors ──────────────────

    #[test]
    fn t_ui_36_round_trip_preserves_explicit_colors() {
        let mut src = ui();
        let btn = src.create_button_colored("Quit", 0.5, 0.5, 0.2, 0.06, Color::rgb(200, 10, 10));
        let json = src.save_json();

        let mut dst = UIManager::with_default_theme();
        dst.load_json(&json);

        assert_eq!(
            dst.effective_color(btn),
            Color::rgb(200, 10, 10),
            "explicit button color must survive round-trip"
        );
    }

    // ── T-UI-37: load_layout is additive — does not clear existing tree ───────

    #[test]
    fn t_ui_37_load_layout_additive() {
        let mut ui = ui();
        let existing = ui.create_label("Existing", 0.0, 0.0, 0.2, 0.05);

        // Build a layout JSON with one panel
        let mut src = UIManager::with_default_theme();
        let _panel = src.create_panel(0.1, 0.1, 0.5, 0.5);
        let layout_json = src.save_json();

        // load_layout should ADD, not replace
        let _ = ui.load_layout(&layout_json);

        assert_eq!(
            ui.widget_count(),
            2,
            "existing widget kept + new panel added"
        );
        // Original widget still accessible
        assert_eq!(ui.get_widget(existing).text, "Existing");
    }

    // ── T-UI-38: set_text updates widget text ────────────────────────────────

    #[test]
    fn t_ui_38_set_text() {
        let mut ui = ui();
        let label = ui.create_label("Initial", 0.5, 0.5, 0.2, 0.05);
        ui.set_text(label, "Updated");
        assert_eq!(ui.get_widget(label).text, "Updated");
    }

    // ── T-UI-39: position_x tween ─────────────────────────────────────────────

    #[test]
    fn t_ui_39_position_x_tween() {
        let mut ui = ui();
        let label = ui.create_label("", 0.0, 0.5, 0.1, 0.05);

        ui.start_tween(label, "position_x", 0.0, 1.0, 1.0, EasingFn::Linear);
        ui.tick(0.5);

        let x = ui.get_widget(label).abs_x;
        assert!(
            (x - 0.5).abs() < 0.05,
            "position_x at t=0.5 should be ~0.5, got {x}"
        );

        ui.tick(0.5);
        let x_final = ui.get_widget(label).abs_x;
        assert!(
            (x_final - 1.0).abs() < 0.01,
            "position_x final should be 1.0, got {x_final}"
        );
        assert!(!ui.has_active_animation(label), "tween must be complete");
    }

    // ── T-UI-40: draw commands not emitted for hidden child ───────────────────

    #[test]
    fn t_ui_40_hidden_child_not_drawn() {
        use rython_renderer::DrawCommand;

        let mut ui = ui();
        let panel = ui.create_panel(0.1, 0.1, 0.8, 0.8);
        let btn = ui.create_button_child("Visible", panel, 0.05, 0.05, 0.2, 0.05);
        let hidden_btn = ui.create_button_child("Hidden", panel, 0.05, 0.15, 0.2, 0.05);
        ui.hide(hidden_btn);
        ui.compute_layout();

        let cmds = ui.build_draw_commands();
        let visible_text = cmds.iter().any(|c| {
            if let DrawCommand::Text(t) = c {
                t.text == "Visible"
            } else {
                false
            }
        });
        let hidden_text = cmds.iter().any(|c| {
            if let DrawCommand::Text(t) = c {
                t.text == "Hidden"
            } else {
                false
            }
        });

        assert!(visible_text, "visible button must emit draw commands");
        assert!(!hidden_text, "hidden button must NOT emit draw commands");
        _ = btn;
    }

    // ── T-UI-41: Deep Nesting — Three Levels ─────────────────────────────────

    #[test]
    fn t_ui_41_deep_nesting_three_levels() {
        let mut ui = ui();
        let outer = ui.create_panel(0.1, 0.2, 0.8, 0.8);
        ui.set_layout(outer, LayoutDir::Vertical, 0.02, 0.01);

        let inner = ui.create_panel(0.0, 0.0, 0.6, 0.4);
        ui.add_child(outer, inner);
        ui.set_layout(inner, LayoutDir::Vertical, 0.02, 0.01);

        let btn = ui.create_button("Deep", 0.0, 0.0, 0.2, 0.05);
        ui.add_child(inner, btn);

        ui.compute_layout();

        let w_outer = ui.get_widget(outer);
        let w_inner = ui.get_widget(inner);
        let w_btn = ui.get_widget(btn);

        // inner panel offset = outer.abs + outer.padding
        let expected_inner_y = w_outer.abs_y + 0.01;
        assert!(
            (w_inner.abs_y - expected_inner_y).abs() < 1e-4,
            "inner panel abs_y={} expected={}",
            w_inner.abs_y,
            expected_inner_y
        );

        // button offset = inner.abs + inner.padding
        let expected_btn_y = w_inner.abs_y + 0.01;
        assert!(
            (w_btn.abs_y - expected_btn_y).abs() < 1e-4,
            "button abs_y={} expected={} (accounts for both parent offsets)",
            w_btn.abs_y,
            expected_btn_y
        );
    }

    // ── T-UI-42: Deep Nesting — Mixed Layouts ────────────────────────────────

    #[test]
    fn t_ui_42_deep_nesting_mixed_layouts() {
        let mut ui = ui();
        let outer = ui.create_panel(0.1, 0.1, 0.8, 0.8);
        ui.set_layout(outer, LayoutDir::Vertical, 0.02, 0.01);

        let inner = ui.create_panel(0.0, 0.0, 0.6, 0.1);
        ui.add_child(outer, inner);
        ui.set_layout(inner, LayoutDir::Horizontal, 0.03, 0.0);

        let btn1 = ui.create_button("L", 0.0, 0.0, 0.1, 0.05);
        ui.add_child(inner, btn1);
        let btn2 = ui.create_button("R", 0.0, 0.0, 0.1, 0.05);
        ui.add_child(inner, btn2);

        ui.compute_layout();

        let w_outer = ui.get_widget(outer);
        let w_inner = ui.get_widget(inner);
        let w_btn1 = ui.get_widget(btn1);
        let w_btn2 = ui.get_widget(btn2);

        // Inner panel is offset vertically within the outer panel (padding)
        let expected_inner_y = w_outer.abs_y + 0.01;
        assert!(
            (w_inner.abs_y - expected_inner_y).abs() < 1e-4,
            "inner panel should be offset by outer padding"
        );

        // btn1 and btn2 should be side-by-side (horizontal layout within inner)
        assert!(
            (w_btn1.abs_y - w_btn2.abs_y).abs() < 1e-4,
            "btn1 and btn2 must share the same abs_y (side-by-side), got {} vs {}",
            w_btn1.abs_y,
            w_btn2.abs_y
        );

        // btn2 should be to the right of btn1 by (btn_w + spacing)
        let expected_btn2_x = w_btn1.abs_x + 0.1 + 0.03;
        assert!(
            (w_btn2.abs_x - expected_btn2_x).abs() < 1e-4,
            "btn2 abs_x={} expected={} (btn1.abs_x + btn_w + spacing)",
            w_btn2.abs_x,
            expected_btn2_x
        );
    }

    // ── T-UI-43: Remove Widget Mid-Animation ─────────────────────────────────

    #[test]
    fn t_ui_43_remove_widget_mid_animation() {
        let mut ui = ui();
        let label = ui.create_label("Anim", 0.0, 0.5, 0.1, 0.05);

        ui.start_tween(label, "position_x", 0.0, 1.0, 1.0, EasingFn::Linear);

        // Advance partway through animation
        ui.tick(0.3);

        // Remove the widget while the animation is still active
        ui.remove_widget(label);

        // Tick again — must not panic even though the target widget is gone
        ui.tick(0.5);
        ui.tick(0.5);

        // Widget is gone
        assert_eq!(ui.widget_count(), 0, "widget must be removed");
    }

    // ── T-UI-44: Overlapping Widgets Click Z-Order ───────────────────────────

    #[test]
    fn t_ui_44_overlapping_widgets_click_zorder() {
        let mut ui = ui();
        let btn_bottom = ui.create_button("Bottom", 0.4, 0.4, 0.2, 0.06);
        let btn_top = ui.create_button("Top", 0.4, 0.4, 0.2, 0.06);

        // Manually set z so btn_top is above btn_bottom
        ui.get_widget_mut(btn_bottom).z = 100.0;
        ui.get_widget_mut(btn_top).z = 101.0;
        ui.compute_layout();

        let bottom_clicked = Arc::new(Mutex::new(false));
        let top_clicked = Arc::new(Mutex::new(false));

        let bc = Arc::clone(&bottom_clicked);
        ui.set_on_click(
            btn_bottom,
            Arc::new(move || {
                *bc.lock().unwrap() = true;
            }),
        );

        let tc = Arc::clone(&top_clicked);
        ui.set_on_click(
            btn_top,
            Arc::new(move || {
                *tc.lock().unwrap() = true;
            }),
        );

        // Click at the shared position — only the top widget should receive it
        let cb = ui.on_mouse_click(0.5, 0.43);
        assert!(cb.is_some(), "click should be consumed by the top button");
        if let Some(cb) = cb {
            cb();
        }

        assert!(
            *top_clicked.lock().unwrap(),
            "top button's handler must fire"
        );
        assert!(
            !*bottom_clicked.lock().unwrap(),
            "bottom button's handler must NOT fire"
        );
    }

    // ── T-UI-45: Focus Traversal Across Parents ──────────────────────────────

    #[test]
    fn t_ui_45_focus_traversal_across_parents() {
        let mut ui = ui();
        let panel1 = ui.create_panel(0.0, 0.0, 0.5, 0.5);
        let btn1 = ui.create_button_child("P1-A", panel1, 0.0, 0.0, 0.2, 0.05);
        let btn2 = ui.create_button_child("P1-B", panel1, 0.0, 0.1, 0.2, 0.05);

        let panel2 = ui.create_panel(0.5, 0.0, 0.5, 0.5);
        let btn3 = ui.create_button_child("P2-A", panel2, 0.0, 0.0, 0.2, 0.05);
        let btn4 = ui.create_button_child("P2-B", panel2, 0.0, 0.1, 0.2, 0.05);

        ui.set_tab_order(vec![btn1, btn2, btn3, btn4]);
        ui.focus(btn1);

        assert!(ui.get_widget(btn1).focused);

        // Tab through all 4 buttons across different panels
        let next = ui.on_tab();
        assert_eq!(next, Some(btn2), "first tab: btn1 → btn2");
        assert!(ui.get_widget(btn2).focused);

        let next = ui.on_tab();
        assert_eq!(
            next,
            Some(btn3),
            "second tab: btn2 → btn3 (crosses panel boundary)"
        );
        assert!(ui.get_widget(btn3).focused);
        assert!(!ui.get_widget(btn2).focused, "btn2 should lose focus");

        let next = ui.on_tab();
        assert_eq!(next, Some(btn4), "third tab: btn3 → btn4");
        assert!(ui.get_widget(btn4).focused);

        // Wrap around back to btn1
        let next = ui.on_tab();
        assert_eq!(next, Some(btn1), "fourth tab: btn4 → btn1 (wrap)");
        assert!(ui.get_widget(btn1).focused);
        assert!(
            !ui.get_widget(btn4).focused,
            "btn4 should lose focus after wrap"
        );
    }

    // ── T-UI-46: Widget Removal Cascades to Children ─────────────────────────

    #[test]
    fn t_ui_46_widget_removal_cascade_children() {
        let mut ui = ui();
        let panel = ui.create_panel(0.0, 0.0, 1.0, 1.0);
        let btn1 = ui.create_button_child("A", panel, 0.0, 0.0, 0.1, 0.05);
        let btn2 = ui.create_button_child("B", panel, 0.0, 0.1, 0.1, 0.05);
        let btn3 = ui.create_button_child("C", panel, 0.0, 0.2, 0.1, 0.05);

        assert_eq!(ui.widget_count(), 4, "panel + 3 children");

        // Remove the panel — all children must cascade
        ui.remove_widget(panel);

        assert_eq!(
            ui.widget_count(),
            0,
            "all widgets must be removed when panel is removed"
        );

        // Verify none of the child IDs are accessible
        // (get_widget would panic on a missing ID; widget_count confirms they are gone)
        _ = (btn1, btn2, btn3);
    }
}

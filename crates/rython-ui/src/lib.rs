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

        assert!((y1 - expected_y1).abs() < 1e-4, "btn1 y={y1} expected={expected_y1}");
        assert!((y2 - expected_y2).abs() < 1e-4, "btn2 y={y2} expected={expected_y2}");
        assert!((y3 - expected_y3).abs() < 1e-4, "btn3 y={y3} expected={expected_y3}");
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

        assert!((x1 - expected_x1).abs() < 1e-4, "btn1 x={x1} expected={expected_x1}");
        assert!((x2 - expected_x2).abs() < 1e-4, "btn2 x={x2} expected={expected_x2}");
        assert!((x3 - expected_x3).abs() < 1e-4, "btn3 x={x3} expected={expected_x3}");

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
        assert_eq!(ui.effective_text_color(label_explicit), Color::rgb(128, 64, 64));
    }

    // ── T-UI-08: Animation — Linear Tween ────────────────────────────────────

    #[test]
    fn t_ui_08_animation_linear_tween() {
        let tween = Tween::new(0.0, 1.0, 1.0, EasingFn::Linear);
        for (t, expected) in
            [(0.0f32, 0.0f32), (0.25, 0.25), (0.5, 0.5), (0.75, 0.75), (1.0, 1.0)]
        {
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
        assert!(v < 0.5, "ease_in starts slow, at t=0.5 value {v} should be < 0.5");
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
        assert!(v > 0.5, "ease_out starts fast, at t=0.5 value {v} should be > 0.5");
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
        assert!(!ui.has_active_animation(label), "animation should be completed");
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
        if let Some(cb) = cb { cb(); }
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
        if let Some(cb) = cb { cb(); }
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
        assert!(!ui.is_visible(label), "widget should be hidden after HideUICmd");

        // Queue show and drain
        ui.queue_cmd(UICmd::Show(label));
        ui.drain_commands();
        assert!(ui.is_visible(label), "widget should be visible after ShowUICmd");
    }
}

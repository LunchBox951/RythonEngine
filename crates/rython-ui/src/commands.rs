use crate::widget::WidgetId;

/// Commands that can be enqueued to UIManager for deferred processing.
///
/// Commands are drained each frame during the UIManager's per-frame task,
/// ensuring consistent state within a frame.
pub enum UICmd {
    /// Make a widget (and its subtree) visible.
    Show(WidgetId),
    /// Hide a widget (and its subtree).
    Hide(WidgetId),
    /// Set keyboard focus to a specific widget.
    Focus(WidgetId),
    /// Show or hide the OS cursor.
    SetCursorVisible(bool),
}

//! Chat widget hooks for side-conversation mode.
//!
//! App-level side-thread lifecycle lives in `app::side`; this module owns the
//! chat-surface pieces that side mode toggles, such as the composer placeholder,
//! footer label, and inline `/side` message submission behavior.

use super::*;

impl ChatWidget {
    pub(crate) fn submit_user_message_as_plain_user_turn(
        &mut self,
        user_message: UserMessage,
    ) -> Option<AppCommand> {
        self.submit_user_message_with_shell_escape_policy(user_message, ShellEscapePolicy::Disallow)
    }

    pub(crate) fn set_side_conversation_active(&mut self, active: bool) {
        self.active_side_conversation = active;
        let placeholder = if active {
            self.side_placeholder_text.clone()
        } else {
            self.normal_placeholder_text.clone()
        };
        self.bottom_pane.set_placeholder_text(placeholder);
        self.bottom_pane.set_side_conversation_active(active);
    }

    pub(crate) fn side_conversation_active(&self) -> bool {
        self.active_side_conversation
    }

    pub(crate) fn set_side_conversation_context_label(&mut self, label: Option<String>) {
        self.bottom_pane.set_side_conversation_context_label(label);
    }

    /// Update which realtime audio device is currently selected.
    ///
    /// Stub — realtime voice is not yet implemented in this fork.
    pub(crate) fn set_realtime_audio_device(
        &mut self,
        _kind: crate::app_event::RealtimeAudioDeviceKind,
        _name: Option<String>,
    ) {
    }

    /// Returns `true` when a realtime WebRTC voice conversation is active.
    ///
    /// Stub — always returns `false` in this fork.
    pub(crate) fn realtime_conversation_is_live(&self) -> bool {
        false
    }

    /// Open a prompt asking the user to restart a realtime audio device.
    ///
    /// Stub — no-op in this fork.
    pub(crate) fn open_realtime_audio_restart_prompt(
        &mut self,
        _kind: crate::app_event::RealtimeAudioDeviceKind,
    ) {
    }

    /// Restart the given realtime audio device.
    ///
    /// Stub — no-op in this fork.
    pub(crate) fn restart_realtime_audio_device(
        &mut self,
        _kind: crate::app_event::RealtimeAudioDeviceKind,
    ) {
    }
}

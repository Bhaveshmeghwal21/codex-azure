//! Realtime audio device enumeration for non-Linux platforms.
//!
//! On Linux, a stub is inlined in lib.rs since platform audio APIs are not available.
//! On Windows and macOS, this module provides device listing via the OS audio stack.

use crate::app_event::RealtimeAudioDeviceKind;

/// List available audio device names of the given kind.
///
/// Returns `Ok(vec![])` on platforms where audio enumeration is not yet
/// implemented so callers can show an empty picker rather than an error.
pub(crate) fn list_realtime_audio_device_names(
    kind: RealtimeAudioDeviceKind,
) -> Result<Vec<String>, String> {
    let _ = kind;
    // Audio device enumeration is not yet implemented for this platform in
    // the Azure fork. Return an empty list so the picker opens gracefully.
    Ok(Vec::new())
}

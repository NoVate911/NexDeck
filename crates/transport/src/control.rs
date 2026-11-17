//! Strongly-typed remote control actions and response contracts.
//!
//! Only explicitly allowlisted media and volume actions can be decoded and
//! forwarded across [`crate::Envelope`] frames with `MessageKind::ControlCommand`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Allowlisted remote control actions available on the Windows host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlAction {
    /// Step host master audio volume up.
    VolumeUp,
    /// Step host master audio volume down.
    VolumeDown,
    /// Toggle host master audio mute state.
    VolumeMuteToggle,
    /// Toggle media playback (play / pause).
    MediaPlayPause,
    /// Skip to next media track.
    MediaNextTrack,
    /// Return to previous media track.
    MediaPreviousTrack,
}

impl ControlAction {
    /// Returns a concise human-readable action identifier for logs and ack messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::VolumeUp => "VolumeUp",
            Self::VolumeDown => "VolumeDown",
            Self::VolumeMuteToggle => "VolumeMuteToggle",
            Self::MediaPlayPause => "MediaPlayPause",
            Self::MediaNextTrack => "MediaNextTrack",
            Self::MediaPreviousTrack => "MediaPreviousTrack",
        }
    }
}

/// Request payload carried inside `MessageKind::ControlCommand`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlCommandRequest {
    /// Client-generated correlation identifier (1..=64 ASCII non-control chars).
    pub command_id: String,
    /// Strictly allowlisted action to invoke.
    pub action: ControlAction,
}

impl ControlCommandRequest {
    /// Validates request identifier bounds to prevent malformed payloads.
    ///
    /// # Errors
    ///
    /// Returns [`ControlValidationError`] if `command_id` is empty, exceeds 64 chars,
    /// or contains whitespace/control characters.
    pub fn validate(&self) -> Result<(), ControlValidationError> {
        let trimmed = self.command_id.trim();
        if trimmed.is_empty() || trimmed.len() != self.command_id.len() {
            return Err(ControlValidationError::InvalidCommandId);
        }
        if trimmed.len() > 64 || !trimmed.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(ControlValidationError::InvalidCommandId);
        }
        Ok(())
    }
}

/// Execution acknowledgment published back over `MessageKind::ControlCommand`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlCommandResponse {
    /// Echoed correlation identifier from [`ControlCommandRequest`].
    pub command_id: String,
    /// Action that was requested.
    pub action: ControlAction,
    /// Whether the action completed cleanly on the host.
    pub ok: bool,
    /// Human-readable status or error message.
    pub message: String,
}

/// Validation failures for [`ControlCommandRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlValidationError {
    /// `command_id` violated ASCII graphic / length constraints.
    InvalidCommandId,
}

impl fmt::Display for ControlValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCommandId => {
                write!(
                    f,
                    "command_id must be 1..=64 printable ASCII characters without spaces"
                )
            }
        }
    }
}

impl std::error::Error for ControlValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_control_command_json_and_validates_id() {
        let req = ControlCommandRequest {
            command_id: "cmd-1".to_owned(),
            action: ControlAction::MediaPlayPause,
        };
        assert_eq!(req.validate(), Ok(()));

        let json = serde_json::to_string(&req).expect("serialize");
        let decoded: ControlCommandRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, req);

        let bad_id = ControlCommandRequest {
            command_id: "bad id".to_owned(),
            action: ControlAction::VolumeUp,
        };
        assert_eq!(
            bad_id.validate(),
            Err(ControlValidationError::InvalidCommandId)
        );

        let empty = ControlCommandRequest {
            command_id: String::new(),
            action: ControlAction::VolumeMuteToggle,
        };
        assert_eq!(
            empty.validate(),
            Err(ControlValidationError::InvalidCommandId)
        );
    }
}

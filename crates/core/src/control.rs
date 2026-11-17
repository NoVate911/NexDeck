//! Allowlisted remote media and volume control execution on the host.
//!
//! Only explicitly enumerated [`ControlAction`] variants are accepted; arbitrary
//! shell commands or elevation/UAC bypasses are never permitted.

use std::sync::{Arc, Mutex};

use nexdeck_transport::control::ControlAction;

/// Host-side executor for allowlisted [`ControlAction`] commands.
pub trait ControlExecutor: Send {
    /// Executes `action` and returns a concise confirmation message on success.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic message if the platform does not support the action or the
    /// OS input call failed.
    fn execute(&self, action: ControlAction) -> Result<String, String>;
}

/// Thread-safe test executor that records executed actions in memory.
#[derive(Debug, Default, Clone)]
pub struct RecordingControlExecutor {
    recorded: Arc<Mutex<Vec<ControlAction>>>,
}

impl RecordingControlExecutor {
    /// Creates an empty [`RecordingControlExecutor`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of all recorded actions in execution order.
    #[must_use]
    pub fn actions(&self) -> Vec<ControlAction> {
        self.recorded
            .lock()
            .map_or_else(|_| Vec::new(), |guard| guard.clone())
    }
}

impl ControlExecutor for RecordingControlExecutor {
    fn execute(&self, action: ControlAction) -> Result<String, String> {
        if let Ok(mut guard) = self.recorded.lock() {
            guard.push(action);
        }
        Ok(format!("Executed {}", action.label()))
    }
}

/// Platform executor returned when running on an OS without Win32 media key bindings.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedControlExecutor;

impl ControlExecutor for UnsupportedControlExecutor {
    fn execute(&self, _action: ControlAction) -> Result<String, String> {
        Err("remote media control requires a Windows host".to_owned())
    }
}

/// Returns the default host [`ControlExecutor`] for the current operating system.
#[must_use]
pub fn default_control_executor() -> Box<dyn ControlExecutor + Send> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsMediaExecutor)
    }
    #[cfg(not(windows))]
    {
        Box::new(UnsupportedControlExecutor)
    }
}

#[cfg(any(windows, test))]
const fn virtual_key_for_action(action: ControlAction) -> u16 {
    // Standard Win32 virtual-key codes (`VK_VOLUME_*` and `VK_MEDIA_*`).
    match action {
        ControlAction::VolumeMuteToggle => 0xAD,
        ControlAction::VolumeDown => 0xAE,
        ControlAction::VolumeUp => 0xAF,
        ControlAction::MediaNextTrack => 0xB0,
        ControlAction::MediaPreviousTrack => 0xB1,
        ControlAction::MediaPlayPause => 0xB3,
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_VOLUME_DOWN,
        VK_VOLUME_MUTE, VK_VOLUME_UP,
    };

    use super::{ControlAction, ControlExecutor, virtual_key_for_action};

    // Compile-time check ensuring our portable virtual-key mapping matches Win32 headers.
    const _: () = {
        assert!(virtual_key_for_action(ControlAction::VolumeMuteToggle) == VK_VOLUME_MUTE);
        assert!(virtual_key_for_action(ControlAction::VolumeDown) == VK_VOLUME_DOWN);
        assert!(virtual_key_for_action(ControlAction::VolumeUp) == VK_VOLUME_UP);
        assert!(virtual_key_for_action(ControlAction::MediaNextTrack) == VK_MEDIA_NEXT_TRACK);
        assert!(virtual_key_for_action(ControlAction::MediaPreviousTrack) == VK_MEDIA_PREV_TRACK);
        assert!(virtual_key_for_action(ControlAction::MediaPlayPause) == VK_MEDIA_PLAY_PAUSE);
    };

    /// Native Windows 11 media & volume key executor using `user32::SendInput`.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct WindowsMediaExecutor;

    impl ControlExecutor for WindowsMediaExecutor {
        fn execute(&self, action: ControlAction) -> Result<String, String> {
            let vk = virtual_key_for_action(action);
            send_media_virtual_key(vk)?;
            Ok(format!("Executed {}", action.label()))
        }
    }

    #[allow(unsafe_code)]
    fn send_media_virtual_key(vk: u16) -> Result<(), String> {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        let cb_size = i32::try_from(std::mem::size_of::<INPUT>())
            .map_err(|_| "INPUT size exceeds i32 range".to_owned())?;

        // SAFETY: `inputs` is an initialized, properly aligned 2-element `INPUT` slice
        // matching `cb_size` bytes per element.
        let injected = unsafe { SendInput(2, inputs.as_ptr(), cb_size) };
        if injected != 2 {
            return Err(format!("SendInput injected {injected}/2 input events"));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use windows::WindowsMediaExecutor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_control_actions_to_expected_virtual_keys() {
        assert_eq!(
            virtual_key_for_action(ControlAction::VolumeMuteToggle),
            0xAD
        );
        assert_eq!(virtual_key_for_action(ControlAction::VolumeDown), 0xAE);
        assert_eq!(virtual_key_for_action(ControlAction::VolumeUp), 0xAF);
        assert_eq!(virtual_key_for_action(ControlAction::MediaNextTrack), 0xB0);
        assert_eq!(
            virtual_key_for_action(ControlAction::MediaPreviousTrack),
            0xB1
        );
        assert_eq!(virtual_key_for_action(ControlAction::MediaPlayPause), 0xB3);
    }

    #[test]
    fn recording_executor_tracks_invoked_actions() {
        let exec = RecordingControlExecutor::new();
        assert!(exec.execute(ControlAction::VolumeUp).is_ok());
        assert!(exec.execute(ControlAction::MediaPlayPause).is_ok());
        assert_eq!(
            exec.actions(),
            vec![ControlAction::VolumeUp, ControlAction::MediaPlayPause]
        );
    }
}

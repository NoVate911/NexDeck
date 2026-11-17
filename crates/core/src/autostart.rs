//! Current-user Windows autostart management (`HKCU\...\Run`).
//!
//! Strictly operates within `HKEY_CURRENT_USER` so background registration
//! never elevates privileges or weakens Windows UAC boundaries.

use std::fmt;
use std::net::SocketAddr;
use std::path::Path;

/// Registry value name under the Windows user Run key.
pub const AUTOSTART_VALUE_NAME: &str = "NexDeckAgent";

/// Registry subkey path under `HKEY_CURRENT_USER`.
pub const WINDOWS_RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Errors produced while formatting, installing, querying, or removing autostart entries.
#[derive(Debug)]
pub enum AutostartError {
    /// Executable path or arguments contained invalid or unsafe characters.
    InvalidCommand(&'static str),
    /// Standard I/O failure resolving the current executable path.
    Io(std::io::Error),
    /// Win32 Registry API returned a non-zero error code.
    WindowsRegistry {
        /// Registry operation that failed.
        operation: &'static str,
        /// Win32 error code returned by `advapi32.dll`.
        code: u32,
    },
    /// Autostart management is unsupported on the current OS.
    UnsupportedPlatform,
}

impl fmt::Display for AutostartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCommand(msg) => write!(f, "invalid autostart command: {msg}"),
            Self::Io(err) => write!(f, "autostart I/O error: {err}"),
            Self::WindowsRegistry { operation, code } => {
                write!(f, "Windows registry error during {operation}: code {code}")
            }
            Self::UnsupportedPlatform => {
                write!(f, "autostart registration is only supported on Windows")
            }
        }
    }
}

impl std::error::Error for AutostartError {}

impl From<std::io::Error> for AutostartError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Formats a quoted command line suitable for `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
///
/// # Errors
///
/// Returns [`AutostartError::InvalidCommand`] if `exe_path` is empty, non-UTF-8, or contains
/// embedded quotes, newlines, or nul characters.
pub fn format_autostart_command(
    exe_path: &Path,
    bind_addr: Option<SocketAddr>,
) -> Result<String, AutostartError> {
    let raw = exe_path.to_str().ok_or(AutostartError::InvalidCommand(
        "executable path must be valid UTF-8",
    ))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AutostartError::InvalidCommand(
            "executable path cannot be empty",
        ));
    }
    if trimmed.contains(['"', '\r', '\n', '\0']) {
        return Err(AutostartError::InvalidCommand(
            "executable path contains disallowed control or quote characters",
        ));
    }

    let mut command = format!("\"{trimmed}\" --daemon");
    if let Some(addr) = bind_addr {
        command.push_str(&format!(" --bind {addr}"));
    }
    Ok(command)
}

/// Registers the currently running `nexdeck-agent` binary in user autostart.
///
/// # Errors
///
/// Returns [`AutostartError`] if the binary path cannot be resolved, formatted, or written
/// to the user Registry key.
pub fn install_current_exe_autostart(
    bind_addr: Option<SocketAddr>,
) -> Result<String, AutostartError> {
    let exe_path = std::env::current_exe()?;
    let command = format_autostart_command(&exe_path, bind_addr)?;
    install_autostart(&command)?;
    Ok(command)
}

/// Writes `command` to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\NexDeckAgent`.
///
/// # Errors
///
/// Returns [`AutostartError`] if validation fails, the platform is unsupported, or Registry
/// access fails.
pub fn install_autostart(command: &str) -> Result<(), AutostartError> {
    if command.trim().is_empty() || command.contains(['\r', '\n', '\0']) {
        return Err(AutostartError::InvalidCommand(
            "autostart command must be non-empty single-line text",
        ));
    }
    platform::install_autostart(command)
}

/// Removes `NexDeckAgent` from `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
///
/// Returns `Ok(true)` if an existing entry was removed or `Ok(false)` if no entry existed.
///
/// # Errors
///
/// Returns [`AutostartError`] if the platform is unsupported or Registry deletion fails.
pub fn uninstall_autostart() -> Result<bool, AutostartError> {
    platform::uninstall_autostart()
}

/// Reads the current `NexDeckAgent` autostart command line, if registered.
///
/// # Errors
///
/// Returns [`AutostartError`] if the platform is unsupported or Registry reading fails.
pub fn query_autostart() -> Result<Option<String>, AutostartError> {
    platform::query_autostart()
}

#[cfg(any(windows, test))]
fn to_wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(any(windows, test))]
fn decode_wide_reg_sz(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    let decoded = String::from_utf16_lossy(&units);
    decoded.trim_end_matches('\0').to_owned()
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW,
        RegSetValueExW,
    };

    use super::{
        AUTOSTART_VALUE_NAME, AutostartError, WINDOWS_RUN_KEY_PATH, decode_wide_reg_sz, to_wide_nul,
    };

    struct RegKeyGuard(NonNull<c_void>);

    impl RegKeyGuard {
        fn raw(&self) -> HKEY {
            self.0.as_ptr()
        }
    }

    impl Drop for RegKeyGuard {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            // SAFETY: `self.0` holds a valid open `HKEY` returned by `RegCreateKeyExW` or `RegOpenKeyExW`.
            unsafe {
                let _ = RegCloseKey(self.0.as_ptr());
            }
        }
    }

    #[allow(unsafe_code)]
    fn create_run_key() -> Result<RegKeyGuard, AutostartError> {
        let subkey = to_wide_nul(WINDOWS_RUN_KEY_PATH);
        let mut raw_key: HKEY = std::ptr::null_mut();

        // SAFETY: `subkey` is nul-terminated UTF-16 and `raw_key` points to stack storage.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                std::ptr::addr_of_mut!(raw_key),
                std::ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(AutostartError::WindowsRegistry {
                operation: "RegCreateKeyExW",
                code: status,
            });
        }

        NonNull::new(raw_key)
            .map(RegKeyGuard)
            .ok_or(AutostartError::WindowsRegistry {
                operation: "RegCreateKeyExW",
                code: ERROR_FILE_NOT_FOUND,
            })
    }

    #[allow(unsafe_code)]
    fn open_run_key(access: u32) -> Result<Option<RegKeyGuard>, AutostartError> {
        let subkey = to_wide_nul(WINDOWS_RUN_KEY_PATH);
        let mut raw_key: HKEY = std::ptr::null_mut();

        // SAFETY: `subkey` is nul-terminated UTF-16 and `raw_key` points to stack storage.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                access,
                std::ptr::addr_of_mut!(raw_key),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(AutostartError::WindowsRegistry {
                operation: "RegOpenKeyExW",
                code: status,
            });
        }

        Ok(NonNull::new(raw_key).map(RegKeyGuard))
    }

    #[allow(unsafe_code)]
    pub(super) fn install_autostart(command: &str) -> Result<(), AutostartError> {
        let key = create_run_key()?;
        let value_name = to_wide_nul(AUTOSTART_VALUE_NAME);
        let wide_cmd = to_wide_nul(command);
        let byte_len = u32::try_from(wide_cmd.len().saturating_mul(std::mem::size_of::<u16>()))
            .map_err(|_| AutostartError::InvalidCommand("autostart command is too long"))?;

        // SAFETY: `key` is an open registry key, `value_name` is nul-terminated UTF-16,
        // and `wide_cmd` contains `byte_len` valid bytes including nul terminator.
        let status = unsafe {
            RegSetValueExW(
                key.raw(),
                value_name.as_ptr(),
                0,
                REG_SZ,
                wide_cmd.as_ptr().cast::<u8>(),
                byte_len,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(AutostartError::WindowsRegistry {
                operation: "RegSetValueExW",
                code: status,
            });
        }
        Ok(())
    }

    #[allow(unsafe_code)]
    pub(super) fn uninstall_autostart() -> Result<bool, AutostartError> {
        let Some(key) = open_run_key(KEY_SET_VALUE)? else {
            return Ok(false);
        };
        let value_name = to_wide_nul(AUTOSTART_VALUE_NAME);

        // SAFETY: `key` is an open registry key and `value_name` is nul-terminated UTF-16.
        let status = unsafe { RegDeleteValueW(key.raw(), value_name.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        if status != ERROR_SUCCESS {
            return Err(AutostartError::WindowsRegistry {
                operation: "RegDeleteValueW",
                code: status,
            });
        }
        Ok(true)
    }

    #[allow(unsafe_code)]
    pub(super) fn query_autostart() -> Result<Option<String>, AutostartError> {
        let Some(key) = open_run_key(KEY_READ)? else {
            return Ok(None);
        };
        let value_name = to_wide_nul(AUTOSTART_VALUE_NAME);
        let mut buf = vec![0u8; 1024];
        let mut byte_len = u32::try_from(buf.len()).unwrap_or(u32::MAX);

        // SAFETY: `key` is open with `KEY_READ`, `value_name` is nul-terminated UTF-16,
        // and `buf` provides `byte_len` writable bytes.
        let mut status = unsafe {
            RegGetValueW(
                key.raw(),
                std::ptr::null(),
                value_name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast::<c_void>(),
                std::ptr::addr_of_mut!(byte_len),
            )
        };

        if status == ERROR_MORE_DATA && byte_len > 0 {
            buf.resize(byte_len as usize, 0);
            // SAFETY: `buf` has been resized to the exact byte count reported by Windows.
            status = unsafe {
                RegGetValueW(
                    key.raw(),
                    std::ptr::null(),
                    value_name.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast::<c_void>(),
                    std::ptr::addr_of_mut!(byte_len),
                )
            };
        }

        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(AutostartError::WindowsRegistry {
                operation: "RegGetValueW",
                code: status,
            });
        }

        let used = (byte_len as usize).min(buf.len());
        Ok(Some(decode_wide_reg_sz(&buf[..used])))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::AutostartError;

    pub(super) fn install_autostart(_command: &str) -> Result<(), AutostartError> {
        Err(AutostartError::UnsupportedPlatform)
    }

    pub(super) fn uninstall_autostart() -> Result<bool, AutostartError> {
        Err(AutostartError::UnsupportedPlatform)
    }

    pub(super) fn query_autostart() -> Result<Option<String>, AutostartError> {
        Err(AutostartError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;

    #[test]
    fn formats_autostart_command_with_quoted_path_and_optional_bind() {
        let exe = PathBuf::from(r"C:\Program Files\NexDeck\nexdeck-agent.exe");
        let cmd = format_autostart_command(&exe, None).expect("valid path");
        assert_eq!(
            cmd,
            r#""C:\Program Files\NexDeck\nexdeck-agent.exe" --daemon"#
        );

        let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9443);
        let cmd_bind = format_autostart_command(&exe, Some(bind)).expect("valid path with bind");
        assert_eq!(
            cmd_bind,
            r#""C:\Program Files\NexDeck\nexdeck-agent.exe" --daemon --bind 127.0.0.1:9443"#
        );
    }

    #[test]
    fn rejects_empty_or_quoted_autostart_paths() {
        assert!(matches!(
            format_autostart_command(Path::new(""), None),
            Err(AutostartError::InvalidCommand(_))
        ));
        assert!(matches!(
            format_autostart_command(Path::new("bad\"path.exe"), None),
            Err(AutostartError::InvalidCommand(_))
        ));
    }

    #[test]
    fn utf16_round_trip_trims_trailing_nuls() {
        let wide = to_wide_nul("nexdeck-agent --daemon");
        let bytes: Vec<u8> = wide.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        assert_eq!(decode_wide_reg_sz(&bytes), "nexdeck-agent --daemon");
    }
}

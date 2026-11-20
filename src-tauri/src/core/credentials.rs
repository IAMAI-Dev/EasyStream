// EasyStream - GPL-3.0-or-later
// Secure secret storage (Windows Credential Manager), ACL-restricted temp files,
// and log/diagnostic secret redaction.

use std::path::{Path, PathBuf};
use rand::Rng;
use regex::Regex;
use std::sync::OnceLock;

const CRED_PREFIX: &str = "EasyStream/";

/// Generate a high-entropy random secret suitable for EasyTier room keys or Sunshine admin passwords.
pub fn generate_high_entropy_secret(len: usize) -> String {
    const ALPHABET: &[u8] =
        b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut rng = rand::rngs::OsRng;
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}

/// Generate a friendly random room identifier (e.g., "es-room-7k4m9p").
pub fn generate_room_name() -> String {
    const LOWER: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::rngs::OsRng;
    let suffix: String = (0..8)
        .map(|_| LOWER[rng.gen_range(0..LOWER.len())] as char)
        .collect();
    format!("easystream-{suffix}")
}

/// Store a UTF-8 secret in Windows Credential Manager (CRED_TYPE_GENERIC, local machine/user scoped).
#[cfg(windows)]
pub fn save_secret(key: &str, username: &str, secret: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let target_name: Vec<u16> = OsStr::new(&format!("{CRED_PREFIX}{key}"))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let user_name: Vec<u16> = OsStr::new(username)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut blob = secret.as_bytes().to_vec();

    let cred = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target_name.as_ptr() as *mut u16,
        Comment: std::ptr::null_mut(),
        LastWritten: unsafe { std::mem::zeroed() },
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: user_name.as_ptr() as *mut u16,
    };

    let ok = unsafe { CredWriteW(&cred, 0) };
    // Zeroize temporary secret buffer
    for b in &mut blob {
        *b = 0;
    }
    if ok == 0 {
        return Err(format!(
            "Failed to store credential '{}' in Windows Credential Manager (err={})",
            key,
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Read a UTF-8 secret + username from Windows Credential Manager.
#[cfg(windows)]
pub fn read_secret(key: &str) -> Result<Option<(String, String)>, String> {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target_name: Vec<u16> = OsStr::new(&format!("{CRED_PREFIX}{key}"))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut cred_ptr: *mut CREDENTIALW = std::ptr::null_mut();

    let ok = unsafe { CredReadW(target_name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred_ptr) };
    if ok == 0 {
        return Ok(None);
    }

    let result = unsafe {
        let cred = &*cred_ptr;
        let secret_slice =
            std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize);
        let secret = String::from_utf8_lossy(secret_slice).to_string();
        let username = if !cred.UserName.is_null() {
            let mut len = 0usize;
            while *cred.UserName.add(len) != 0 {
                len += 1;
            }
            OsString::from_wide(std::slice::from_raw_parts(cred.UserName, len))
                .to_string_lossy()
                .to_string()
        } else {
            String::new()
        };
        CredFree(cred_ptr as *const _);
        (username, secret)
    };

    Ok(Some(result))
}

/// Delete a stored secret from Windows Credential Manager.
#[cfg(windows)]
pub fn delete_secret(key: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target_name: Vec<u16> = OsStr::new(&format!("{CRED_PREFIX}{key}"))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        CredDeleteW(target_name.as_ptr(), CRED_TYPE_GENERIC, 0);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn save_secret(_key: &str, _username: &str, _secret: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn read_secret(_key: &str) -> Result<Option<(String, String)>, String> {
    Ok(None)
}

#[cfg(not(windows))]
pub fn delete_secret(_key: &str) -> Result<(), String> {
    Ok(())
}

/// Write a temporary runtime configuration file (e.g. EasyTier TOML containing room secret)
/// and lock its NTFS ACL so only the current user and SYSTEM can read it.
pub fn write_restricted_runtime_file(dir: &Path, filename: &str, content: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create runtime dir: {e}"))?;
    let path = dir.join(filename);
    std::fs::write(&path, content).map_err(|e| format!("Failed to write runtime config: {e}"))?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let user = std::env::var("USERNAME").unwrap_or_default();
        if !user.is_empty() {
            // Remove inherited ACLs and grant full control exclusively to current user & SYSTEM.
            let _ = std::process::Command::new("icacls")
                .args([
                    path.to_string_lossy().as_ref(),
                    "/inheritance:r",
                    "/grant:r",
                    &format!("{user}:(R,W)"),
                    "SYSTEM:(F)",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
    }

    Ok(path)
}

/// Securely overwrite and delete a temporary runtime configuration file.
pub fn secure_delete_file(path: &Path) {
    if path.exists() {
        if let Ok(meta) = std::fs::metadata(path) {
            let zeros = vec![0u8; meta.len() as usize];
            let _ = std::fs::write(path, zeros);
        }
        let _ = std::fs::remove_file(path);
    }
}

/// Scrub room secrets, admin passwords, pairing PINs, Authorization headers, and private keys
/// before anything enters logs or exported diagnostic bundles (Section 4 & Section 6).
pub fn redact_secrets(input: &str) -> String {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        vec![
            (
                Regex::new(r#"(?i)(network_secret\s*=\s*)"[^"]*""#).unwrap(),
                r#"$1"[REDACTED]""#,
            ),
            (
                Regex::new(r#"(?i)(--pin\s+)(\d{4})"#).unwrap(),
                "$1[REDACTED_PIN]",
            ),
            (
                Regex::new(r#"(?i)("pin"\s*:\s*)"[^"]*""#).unwrap(),
                r#"$1"[REDACTED_PIN]""#,
            ),
            (
                Regex::new(r#"(?i)("?(?:room_?secret|password|newPassword|currentPassword|confirmNewPassword)"?\s*[:=]\s*)"?[^",\s}]+"?"#).unwrap(),
                r#"$1"[REDACTED]""#,
            ),
            (
                Regex::new(r#"(?i)(Authorization:\s*(?:Basic|Bearer)\s+)\S+"#).unwrap(),
                "$1[REDACTED]",
            ),
            (
                Regex::new(r#"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----"#).unwrap(),
                "[REDACTED_PRIVATE_KEY]",
            ),
        ]
    });

    let mut out = input.to_string();
    for (re, replacement) in patterns {
        out = re.replace_all(&out, *replacement).to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_pin_secret_and_auth() {
        let raw = r#"Moonlight.exe pair 10.144.144.2 --pin 0429 network_secret = "supersecret123" Authorization: Basic dXNlcjpwYXNz"#;
        let scrubbed = redact_secrets(raw);
        assert!(!scrubbed.contains("0429"));
        assert!(!scrubbed.contains("supersecret123"));
        assert!(!scrubbed.contains("dXNlcjpwYXNz"));
        assert!(scrubbed.contains("[REDACTED_PIN]"));
        assert!(scrubbed.contains("[REDACTED]"));
    }

    #[test]
    fn generates_high_entropy_secret_length() {
        let s = generate_high_entropy_secret(24);
        assert_eq!(s.len(), 24);
    }
}

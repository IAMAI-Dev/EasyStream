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
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_NOT_FOUND as i32) {
            return Ok(None);
        }
        return Err(format!("CredReadW failed: {err}"));
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
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    // No room data is written until the file has its private ACL.
    drop(options.open(&path).map_err(|e| format!("无法创建房间配置: {e}"))?);
    #[cfg(windows)]
    if let Err(err) = restrict_runtime_file_acl(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(err);
    }
    if let Err(err) = std::fs::write(&path, content) {
        secure_delete_file(&path);
        return Err(format!("无法写入房间配置: {err}"));
    }
    Ok(path)
}

#[cfg(windows)]
fn restrict_runtime_file_acl(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let system = std::env::var_os("SystemRoot").ok_or("无法定位 Windows 系统目录")?;
    let system32 = PathBuf::from(system).join("System32");
    let identity = Command::new(system32.join("whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("无法读取当前账户 SID: {e}"))?;
    let sid = String::from_utf8_lossy(&identity.stdout);
    let sid = sid.split('"').find(|field| {
        field.strip_prefix("S-1-").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit() || b == b'-')
        })
    });
    let sid = sid.filter(|_| identity.status.success()).ok_or("无法识别当前账户 SID")?;
    let rights = format!("*{sid}:M");
    let acl = Command::new(system32.join("icacls.exe"))
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &rights, "*S-1-5-18:F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("无法设置房间配置的权限: {e}"))?;
    if !acl.status.success() {
        return Err(format!("无法限制房间配置的访问权限（icacls 退出码 {:?}）", acl.status.code()));
    }
    Ok(())
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

    #[cfg(windows)]
    #[test]
    fn restricted_room_config_is_readable_by_the_current_user() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let content = "dhcp = true\n";
        let path = write_restricted_runtime_file(&dir, "room.toml", content).unwrap();
        let saved = std::fs::read_to_string(&path);
        secure_delete_file(&path);
        assert!(!path.exists());
        std::fs::remove_dir(&dir).unwrap();
        assert_eq!(saved.unwrap(), content);
    }

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

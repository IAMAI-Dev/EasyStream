// EasyStream - GPL-3.0-or-later
// Owned process lifecycle management with Windows Job Object (KILL_ON_JOB_CLOSE)
// and scoped Windows Service Manager (SCM) ownership controls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// Wrapper for Windows Job Object handle with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.
#[cfg(windows)]
struct JobObjectHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for JobObjectHandle {}
#[cfg(windows)]
unsafe impl Sync for JobObjectHandle {}

#[cfg(windows)]
impl Drop for JobObjectHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

pub struct ManagedProcess {
    pub label: String,
    pub pid: u32,
    pub child: Child,
    pub sensitive_args: bool,
}

pub struct ProcessManager {
    #[cfg(windows)]
    job: Option<JobObjectHandle>,
    owned: Mutex<HashMap<u32, ManagedProcess>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        #[cfg(windows)]
        let job = Self::create_kill_on_close_job().ok();

        Self {
            #[cfg(windows)]
            job,
            owned: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(windows)]
    fn create_kill_on_close_job() -> Result<JobObjectHandle, String> {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(format!(
                "CreateJobObjectW failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
            return Err(format!(
                "SetInformationJobObject failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(JobObjectHandle(handle))
    }

    /// Spawn an owned child process with discrete argv (never shell interpolation).
    /// On Windows, the child is created suspended, bound to the Job Object BEFORE execution starts,
    /// and terminated immediately if binding fails (Section 5 requirement).
    pub fn spawn_owned(
        &self,
        label: &str,
        program: &Path,
        args: &[String],
        working_dir: Option<&Path>,
        env_vars: &[(&str, &str)],
        hide_window: bool,
        sensitive_args: bool,
    ) -> Result<u32, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        for (k, v) in env_vars {
            cmd.env(k, v);
        }

        #[cfg(windows)]
        {
            let mut flags = CREATE_SUSPENDED;
            if hide_window {
                flags |= CREATE_NO_WINDOW;
            }
            cmd.creation_flags(flags);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn {} ({}): {e}", label, program.display()))?;
        let pid = child.id();

        #[cfg(windows)]
        {
            if let Some(ref job) = self.job {
                let proc_handle = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
                let assigned = unsafe {
                    windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
                        job.0,
                        proc_handle,
                    )
                };
                if assigned == 0 {
                    // Section 5: binding failure MUST terminate startup immediately.
                    let err = std::io::Error::last_os_error();
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "Failed to bind {} (pid={}) to Job Object; startup aborted: {err}",
                        label, pid
                    ));
                }
            }

            // Resume main thread of suspended process via Toolhelp snapshot or NtResumeProcess.
            if let Err(e) = resume_process_threads(pid) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed to resume {} (pid={}): {e}", label, pid));
            }
        }

        if !sensitive_args {
            tracing::info!(label = %label, pid = pid, program = %program.display(), "Spawned owned process");
        } else {
            tracing::info!(label = %label, pid = pid, program = %program.display(), "Spawned owned process (args redacted)");
        }

        let mut map = self.owned.lock().unwrap();
        map.insert(
            pid,
            ManagedProcess {
                label: label.to_string(),
                pid,
                child,
                sensitive_args,
            },
        );
        Ok(pid)
    }

    /// Poll whether an owned child process has exited, returning its exit code if finished.
    pub fn try_wait_owned(&self, pid: u32) -> Option<i32> {
        let mut map = self.owned.lock().unwrap();
        let proc = map.get_mut(&pid)?;
        match proc.child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                map.remove(&pid);
                Some(code)
            }
            _ => None,
        }
    }

    /// Take stdout/stderr handles from a running owned process (if not yet taken).
    pub fn take_output_pipes(
        &self,
        pid: u32,
    ) -> (Option<std::process::ChildStdout>, Option<std::process::ChildStderr>) {
        let mut map = self.owned.lock().unwrap();
        if let Some(proc) = map.get_mut(&pid) {
            return (proc.child.stdout.take(), proc.child.stderr.take());
        }
        (None, None)
    }

    /// Gracefully stop a specific owned child process by PID (never bulk-kill by image name!).
    pub fn stop_owned(&self, pid: u32) -> Result<(), String> {
        let mut proc = {
            let mut map = self.owned.lock().unwrap();
            match map.remove(&pid) {
                Some(p) => p,
                None => return Ok(()),
            }
        };

        // Check if already exited
        if let Ok(Some(_)) = proc.child.try_wait() {
            return Ok(());
        }

        let _ = proc.child.kill();
        let _ = proc.child.wait();
        tracing::info!(label = %proc.label, pid = pid, "Stopped owned process");
        Ok(())
    }

    /// Stop all owned processes (called on clean application exit).
    pub fn stop_all_owned(&self) {
        let pids: Vec<u32> = {
            let map = self.owned.lock().unwrap();
            map.keys().copied().collect()
        };
        for pid in pids {
            let _ = self.stop_owned(pid);
        }
    }

    /// Resolve a bundled or installed binary path in deterministic order:
    /// 1. Executable sibling `resources/<subdir>/<name>`
    /// 2. Repo dev `src-tauri/resources/<subdir>/<name>`
    /// 3. Executable sibling `binaries/<name>` / target-triple sidecar
    /// 4. `%LOCALAPPDATA%\EasyStream\components\<subdir>\<name>`
    pub fn resolve_component_binary(&self, subdir: &str, filename: &str) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("resources").join(subdir).join(filename));
                candidates.push(dir.join(subdir).join(filename));
                candidates.push(dir.join(filename));
                // Dev mode: target/debug/easystream.exe -> src-tauri/resources/...
                if let Some(p1) = dir.parent().and_then(|p| p.parent()) {
                    candidates.push(p1.join("resources").join(subdir).join(filename));
                    candidates.push(p1.join("binaries").join(filename));
                    let stem = filename.trim_end_matches(".exe");
                    candidates.push(
                        p1.join("binaries")
                            .join(format!("{stem}-x86_64-pc-windows-msvc.exe")),
                    );
                    candidates.push(
                        p1.join("binaries")
                            .join(format!("{stem}-x86_64-pc-windows-gnu.exe")),
                    );
                }
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("src-tauri").join("resources").join(subdir).join(filename));
            candidates.push(cwd.join("resources").join(subdir).join(filename));
        }

        let local_components = crate::state::app_data_dir().join("components");
        candidates.push(local_components.join(subdir).join(filename));

        candidates.into_iter().find(|p| p.is_file())
    }
}

/// Resume all threads of a newly created suspended process on Windows.
#[cfg(windows)]
fn resume_process_threads(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    // Use NtResumeProcess via ntdll if available; fallback to CreateToolhelp32Snapshot isn't needed
    // because ntdll!NtResumeProcess is present on all modern Windows x64 versions.
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SUSPEND_RESUME};

    #[link(name = "ntdll")]
    extern "system" {
        fn NtResumeProcess(process_handle: windows_sys::Win32::Foundation::HANDLE) -> i32;
    }

    let handle = unsafe { OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid) };
    if handle.is_null() {
        // Keep OpenThread/ResumeThread referenced for clippy cleanliness
        let _ = (OpenThread, ResumeThread, THREAD_SUSPEND_RESUME);
        return Err(format!(
            "OpenProcess(PROCESS_SUSPEND_RESUME) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let status = unsafe { NtResumeProcess(handle) };
    unsafe { CloseHandle(handle) };
    if status != 0 {
        return Err(format!("NtResumeProcess failed with NTSTATUS 0x{status:08X}"));
    }
    Ok(())
}

/// Windows Service status summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsServiceState {
    NotInstalled,
    Stopped,
    Starting,
    Running,
    Stopping,
    Unknown,
}

/// Query Windows Service Control Manager for a named service (e.g. "SunshineService").
#[cfg(windows)]
pub fn query_windows_service(service_name: &str) -> WindowsServiceState {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus,
        SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START_PENDING,
        SERVICE_STATUS, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    };

    let sc = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
    if sc.is_null() {
        return WindowsServiceState::Unknown;
    }

    let wide: Vec<u16> = OsStr::new(service_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let svc = unsafe { OpenServiceW(sc, wide.as_ptr(), SERVICE_QUERY_STATUS) };
    if svc.is_null() {
        unsafe { CloseServiceHandle(sc) };
        return WindowsServiceState::NotInstalled;
    }

    let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { QueryServiceStatus(svc, &mut status) };
    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(sc);
    }

    if ok == 0 {
        return WindowsServiceState::Unknown;
    }

    match status.dwCurrentState {
        SERVICE_RUNNING => WindowsServiceState::Running,
        SERVICE_START_PENDING => WindowsServiceState::Starting,
        SERVICE_STOPPED => WindowsServiceState::Stopped,
        SERVICE_STOP_PENDING => WindowsServiceState::Stopping,
        _ => WindowsServiceState::Unknown,
    }
}

#[cfg(not(windows))]
pub fn query_windows_service(_service_name: &str) -> WindowsServiceState {
    WindowsServiceState::NotInstalled
}

/// Start a Windows service directly if current token has sufficient privileges.
#[cfg(windows)]
pub fn start_windows_service(service_name: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW, SC_MANAGER_CONNECT,
        SERVICE_START,
    };

    let sc = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
    if sc.is_null() {
        return Err(format!(
            "OpenSCManagerW failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let wide: Vec<u16> = OsStr::new(service_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let svc = unsafe { OpenServiceW(sc, wide.as_ptr(), SERVICE_START) };
    if svc.is_null() {
        let err = std::io::Error::last_os_error();
        unsafe { CloseServiceHandle(sc) };
        return Err(format!("OpenServiceW({}) failed: {err}", service_name));
    }
    let ok = unsafe { StartServiceW(svc, 0, std::ptr::null()) };
    let err = std::io::Error::last_os_error();
    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(sc);
    }
    if ok == 0 {
        // ERROR_SERVICE_ALREADY_RUNNING (1056) is treated as success
        if err.raw_os_error() == Some(1056) {
            return Ok(());
        }
        return Err(format!("StartServiceW({}) failed: {err}", service_name));
    }
    Ok(())
}

/// Stop a Windows service directly if current token has sufficient privileges.
#[cfg(windows)]
pub fn stop_windows_service(service_name: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CONTROL_STOP, SERVICE_STATUS, SERVICE_STOP,
    };

    let sc = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
    if sc.is_null() {
        return Err(format!(
            "OpenSCManagerW failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let wide: Vec<u16> = OsStr::new(service_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let svc = unsafe { OpenServiceW(sc, wide.as_ptr(), SERVICE_STOP) };
    if svc.is_null() {
        let err = std::io::Error::last_os_error();
        unsafe { CloseServiceHandle(sc) };
        return Err(format!("OpenServiceW({}) failed: {err}", service_name));
    }
    let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut status) };
    let err = std::io::Error::last_os_error();
    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(sc);
    }
    if ok == 0 {
        // ERROR_SERVICE_NOT_ACTIVE (1062) is treated as success
        if err.raw_os_error() == Some(1062) {
            return Ok(());
        }
        return Err(format!("ControlService(STOP, {}) failed: {err}", service_name));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn start_windows_service(_service_name: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn stop_windows_service(_service_name: &str) -> Result<(), String> {
    Ok(())
}

/// Path of the ownership marker indicating Sunshine was installed/adopted by EasyStream.
pub fn sunshine_ownership_marker_path() -> PathBuf {
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
    PathBuf::from(program_data)
        .join("EasyStream")
        .join("sunshine-owner.json")
}

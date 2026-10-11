//! Platform process isolation shared by the official CLI readers.
use std::path::Path;
use tokio::process::Command;

pub(crate) fn command(executable: &Path) -> Command {
    // Node's entry-point resolver rejects canonical Win32 verbatim path prefixes.
    #[cfg(windows)]
    let executable = dunce::simplified(executable);
    #[cfg(windows)]
    if executable
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"))
    {
        // Run the official npm entry point directly: no cmd.exe quoting or shell.
        let directory = executable.parent().expect("absolute CLI path");
        let name = executable.file_stem().unwrap_or_default();
        let entry = if name.eq_ignore_ascii_case("codex") {
            "node_modules/@openai/codex/bin/codex.js"
        } else {
            "node_modules/@anthropic-ai/claude-code/cli.js"
        };
        let node = directory.join("node.exe");
        let node = if node.is_file() {
            node
        } else {
            std::env::var_os("ProgramFiles")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "C:\\Program Files".into())
                .join("nodejs/node.exe")
        };
        let mut command = Command::new(node);
        command.arg(directory.join(entry));
        return command;
    }
    Command::new(executable)
}

pub(crate) fn isolate(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.as_std_mut().creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

#[cfg(windows)]
pub(crate) struct ProcessJob(std::os::windows::io::OwnedHandle);

#[cfg(windows)]
impl ProcessJob {
    pub(crate) fn attach(child: &tokio::process::Child) -> std::io::Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::*;
        // SAFETY: own the new handle immediately; the child handle is borrowed.
        unsafe {
            let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if raw.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let job = OwnedHandle::from_raw_handle(raw);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let process = child
                .raw_handle()
                .ok_or_else(|| std::io::Error::other("child exited"))?;
            if SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(job.as_raw_handle(), process) == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self(job))
        }
    }

    pub(crate) fn terminate(&self) {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: this handle is owned exclusively by the reader's process job.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0.as_raw_handle(), 1);
        }
    }
}

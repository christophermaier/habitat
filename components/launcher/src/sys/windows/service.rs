// Copyright (c) 2017 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use core::os::process::handle_from_pid;
use core::os::process::windows_child::{Child, ExitStatus};
use kernel32;
use protocol;
use winapi;

use error::{Error, Result};
use service::Service;

pub struct Process {
    handle: Option<winapi::HANDLE>,
    last_status: Option<u32>,
    pid: u32,
}

impl Process {
    // On windows we need the process handle to capture status
    // Here we will attempt to get the handle from the pid but if the
    // process dies before we can get it, we will just wait() on the
    // std::process::Child and cache the exit_status which we will return
    // when status is called.
    fn new(child: Child) -> Self {
        let (win_handle, status) = match handle_from_pid(child.id()) {
            Some(handle) => (Some(handle), Ok(None)),
            _ => {
                (None, {
                    match child.wait() {
                        Ok(exit) => Ok(Some(exit.code().unwrap() as u32)),
                        Err(e) => {
                            Err(format!(
                                "Failed to retrieve exit code for pid {} : {}",
                                child.id(),
                                e
                            ))
                        }
                    }
                })
            }
        };

        match status {
            Ok(status) => {
                Ok(Child {
                    handle: win_handle,
                    last_status: status,
                    pid: child.id(),
                })
            }
            Err(e) => Err(Error::GetHabChildFailed(e)),
        }

        Process {
            child: child,
            status: None,
        }
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn status(&mut self) -> Result<HabExitStatus> {
        if self.last_status.is_some() {
            return Ok(HabExitStatus { status: Some(self.last_status.unwrap()) });
        }

        let exit_status = exit_status(self.handle.unwrap())?;

        if exit_status == STILL_ACTIVE {
            return Ok(HabExitStatus { status: None });
        };

        Ok(HabExitStatus { status: Some(exit_status) })
    }

    pub fn kill(&mut self) -> Result<ShutdownMethod> {
        if self.last_status.is_some() {
            return Ok(ShutdownMethod::AlreadyExited);
        }

        let ret;
        unsafe {
            // Send a ctrl-BREAK
            ret = kernel32::GenerateConsoleCtrlEvent(1, self.pid);
            if ret == 0 {
                debug!(
                    "Failed to send ctrl-break to pid {}: {}",
                    self.pid,
                    io::Error::last_os_error()
                );
            }
        }

        let stop_time = SteadyTime::now() + Duration::seconds(8);

        let result;
        loop {
            if ret == 0 || SteadyTime::now() > stop_time {
                let proc_table = self.build_proc_table()?;
                self.terminate_process_descendants(&proc_table, self.pid)?;
                result = Ok(ShutdownMethod::Killed);
                break;
            }

            if let Ok(status) = self.status() {
                if !status.no_status() {
                    result = Ok(ShutdownMethod::GracefulTermination);
                    break;
                }
            }
        }
        result
    }

    fn terminate_process_descendants(
        &self,
        table: &HashMap<winapi::DWORD, Vec<winapi::DWORD>>,
        pid: winapi::DWORD,
    ) -> Result<()> {
        if let Some(children) = table.get(&pid) {
            for child in children {
                self.terminate_process_descendants(table, child.clone())?;
            }
        }
        unsafe {
            match handle_from_pid(pid) {
                Some(h) => {
                    if kernel32::TerminateProcess(h, 1) == 0 {
                        return Err(Error::TerminateProcessFailed(format!(
                            "Failed to call TerminateProcess on pid {}: {}",
                            pid,
                            io::Error::last_os_error()
                        )));
                    }
                }
                None => {}
            }
        }
        Ok(())
    }

    fn build_proc_table(&self) -> Result<HashMap<winapi::DWORD, Vec<winapi::DWORD>>> {
        let processes_snap_handle =
            unsafe { kernel32::CreateToolhelp32Snapshot(winapi::TH32CS_SNAPPROCESS, 0) };

        if processes_snap_handle == winapi::INVALID_HANDLE_VALUE {
            return Err(Error::CreateToolhelp32SnapshotFailed(format!(
                "Failed to call CreateToolhelp32Snapshot: {}",
                io::Error::last_os_error()
            )));
        }

        let mut table: HashMap<winapi::DWORD, Vec<winapi::DWORD>> = HashMap::new();
        let mut process_entry = winapi::PROCESSENTRY32W {
            dwSize: mem::size_of::<winapi::PROCESSENTRY32W>() as u32,
            cntUsage: 0,
            th32ProcessID: 0,
            th32DefaultHeapID: 0,
            th32ModuleID: 0,
            cntThreads: 0,
            th32ParentProcessID: 0,
            pcPriClassBase: 0,
            dwFlags: 0,
            szExeFile: [0; winapi::MAX_PATH],
        };

        // Get the first process from the snapshot.
        match unsafe { kernel32::Process32FirstW(processes_snap_handle, &mut process_entry) } {
            1 => {
                // First process worked, loop to find the process with the correct name.
                let mut process_success: i32 = 1;

                // Loop through all processes until we find one hwere `szExeFile` == `name`.
                while process_success == 1 {
                    let children = table.entry(process_entry.th32ParentProcessID).or_insert(
                        Vec::new(),
                    );
                    (*children).push(process_entry.th32ProcessID);

                    process_success = unsafe {
                        kernel32::Process32NextW(processes_snap_handle, &mut process_entry)
                    };
                }

                unsafe { kernel32::CloseHandle(processes_snap_handle) };
            }
            0 | _ => {
                unsafe { kernel32::CloseHandle(processes_snap_handle) };
            }
        }

        Ok(table)
    }

    pub fn kill(&mut self) -> protocol::ShutdownMethod {
        self.child.kill()
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        self.child.wait()
    }

    pub fn wait(&mut self) -> Result<ExitStatus> {
        self.child.wait()
    }
}

unsafe impl Send for Process {}
unsafe impl Sync for Process {}

impl Drop for Process {
    fn drop(&mut self) {
        match self.handle {
            None => {}
            Some(handle) => unsafe {
                let _ = kernel32::CloseHandle(handle);
            },
        }
    }
}

pub fn run(msg: protocol::Spawn) -> Result<Service> {
    let ps_cmd = format!("iex $(gc {} | out-string)", msg.get_binary());
    let child = Child::spawn(
        "powershell.exe",
        vec!["-command", ps_cmd.as_str()],
        msg.get_env(),
    ).map_err(Error::Spawn)?;
    let process = Process::new(child);
    Ok(Service::new(msg, process, child.stdout, child.stderr))
}

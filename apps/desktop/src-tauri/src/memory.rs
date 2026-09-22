//! Memory accounting for the overview, and returning freed memory to the OS.
//!
//! The web views do not run inside the app process: WebKit starts XPC
//! services (WebContent, GPU, Networking) whose parent is launchd. macOS
//! records which process is "responsible" for each of them (Activity
//! Monitor uses it to group processes). WebKit processes that share the
//! app's responsible process and started after the app are counted as
//! the app's web views.

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize)]
pub struct MemoryDto {
    /// Resident memory of the app process (Rust core + UI shell).
    pub app_bytes: u64,
    /// Resident memory of the WebKit processes serving the app's windows.
    pub webview_bytes: u64,
    pub webview_processes: usize,
    /// Resident memory of the AI worker process, while it runs.
    pub ai_bytes: u64,
    pub system_total: u64,
    pub system_used: u64,
}

pub struct Meter {
    system: System,
    pid: Pid,
}

impl Meter {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            pid: Pid::from_u32(std::process::id()),
        }
    }

    /// Measures the app, its web views and, if given, the AI worker.
    pub fn snapshot(&mut self, worker: Option<u32>) -> MemoryDto {
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let (app_bytes, started) = self
            .system
            .process(self.pid)
            .map(|p| (p.memory(), p.start_time()))
            .unwrap_or_default();
        let owner = responsible_pid(self.pid.as_u32());

        let webviews: Vec<u64> = self
            .system
            .processes()
            .values()
            .filter(|p| p.name().to_string_lossy().starts_with("com.apple.WebKit"))
            .filter(|p| p.start_time() >= started)
            .filter(|p| owner.is_some() && responsible_pid(p.pid().as_u32()) == owner)
            .map(|p| p.memory())
            .collect();

        let ai_bytes = worker
            .and_then(|pid| self.system.process(Pid::from_u32(pid)))
            .map_or(0, |p| p.memory());

        MemoryDto {
            app_bytes,
            ai_bytes,
            webview_bytes: webviews.iter().sum(),
            webview_processes: webviews.len(),
            system_total: self.system.total_memory(),
            system_used: self.system.used_memory(),
        }
    }
}

#[cfg(target_os = "macos")]
fn responsible_pid(pid: u32) -> Option<i32> {
    unsafe extern "C" {
        // libSystem; stable since macOS 10.14, used by Activity Monitor.
        fn responsibility_get_pid_responsible_for_pid(pid: i32) -> i32;
    }
    let owner = unsafe { responsibility_get_pid_responsible_for_pid(pid as i32) };
    (owner > 0).then_some(owner)
}

#[cfg(not(target_os = "macos"))]
fn responsible_pid(_pid: u32) -> Option<i32> {
    None
}

/// Asks the allocator to return free pages to the OS, so dropped data shows
/// up as a lower resident size. Returns the bytes released when known.
pub fn release_free_pages() -> usize {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // A null zone means every zone; a goal of 0 means as much as possible.
        unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) }
    }
    #[cfg(not(target_os = "macos"))]
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_own_process() {
        let snapshot = Meter::new().snapshot(None);
        assert!(snapshot.app_bytes > 0);
        assert!(snapshot.system_total >= snapshot.system_used);
    }

    #[test]
    fn freed_memory_is_released() {
        let big = vec![1u8; 64 * 1024 * 1024];
        drop(std::hint::black_box(big));
        // Nothing to assert on the amount (the allocator may already have
        // unmapped a large block); it must simply not crash.
        let _ = release_free_pages();
    }
}

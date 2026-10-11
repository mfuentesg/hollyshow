//! Live health numbers for the operator footer: CPU and memory of this process (sampled with
//! `sysinfo`) and the frame statistics the output window reports.

use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthInfo {
    pub fps: f64,
    pub dropped_frames: u32,
    pub cpu_percent: f64,
    pub ram_mb: f64,
}

pub struct Health {
    sys: Mutex<System>,
    fps_bits: AtomicU32,
    dropped: AtomicU32,
}

impl Health {
    pub fn new() -> Self {
        Self {
            sys: Mutex::new(System::new()),
            fps_bits: AtomicU32::new(0f32.to_bits()),
            dropped: AtomicU32::new(0),
        }
    }

    /// Called by the output window every couple of seconds.
    pub fn report_frames(&self, fps: f64, dropped: u32) {
        let fps = if fps.is_finite() {
            fps.clamp(0.0, 1000.0)
        } else {
            0.0
        };
        self.fps_bits
            .store((fps as f32).to_bits(), Ordering::Relaxed);
        self.dropped.store(dropped, Ordering::Relaxed);
    }

    pub fn sample(&self) -> HealthInfo {
        let mut sys = self.sys.lock().unwrap_or_else(|p| p.into_inner());
        let pid = Pid::from_u32(std::process::id());
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let (cpu, mem) = sys
            .process(pid)
            .map_or((0.0, 0), |p| (f64::from(p.cpu_usage()), p.memory()));
        HealthInfo {
            fps: f64::from(f32::from_bits(self.fps_bits.load(Ordering::Relaxed))),
            dropped_frames: self.dropped.load(Ordering::Relaxed),
            cpu_percent: cpu,
            ram_mb: mem as f64 / (1024.0 * 1024.0),
        }
    }
}

impl Default for Health {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_reports_are_clamped_and_sampled() {
        let h = Health::new();
        h.report_frames(59.94, 3);
        let s = h.sample();
        assert!((s.fps - 59.94).abs() < 0.01);
        assert_eq!(s.dropped_frames, 3);
        h.report_frames(f64::NAN, 0);
        assert_eq!(h.sample().fps, 0.0);
        assert!(h.sample().ram_mb > 0.0, "this process uses some memory");
    }
}

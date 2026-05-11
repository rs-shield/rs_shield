use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use battery::units::ratio::percent;
use sysinfo::{RefreshKind, System};
use tracing::{info, warn};

pub fn spawn_resource_monitor(
    battery_threshold: Option<u8>,
    cpu_threshold: Option<u8>,
) -> (
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    let should_pause = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));

    let pause_clone = should_pause.clone();
    let running_clone = running.clone();

    let handle = thread::spawn(move || {
        let mut system = System::new_with_specifics(RefreshKind::everything());
        let battery_manager = battery::Manager::new().ok();

        system.refresh_cpu_all();
        thread::sleep(Duration::from_millis(500));

        while running_clone.load(Ordering::Relaxed) {
            let mut pause = false;
            let mut reason = String::new();

            // Battery check
            if let Some(threshold) = battery_threshold {
                if let Some(manager) = &battery_manager {
                    if let Ok(mut batteries) = manager.batteries() {
                        if let Some(Ok(battery)) = batteries.next() {
                            if battery.state() == battery::State::Discharging {
                                let charge = battery.state_of_charge().get::<percent>();
                                if charge < threshold as f32 {
                                    pause = true;
                                    reason = format!("Low battery ({:.0}%)", charge);
                                }
                            }
                        }
                    }
                }
            }

            // CPU check
            if !pause {
                if let Some(threshold) = cpu_threshold {
                    system.refresh_cpu_all();
                    let usage = system.global_cpu_usage();
                    if usage > threshold as f32 {
                        pause = true;
                        reason = format!("High CPU usage ({:.1}%)", usage);
                    }
                }
            }

            let was_paused = pause_clone.swap(pause, Ordering::Relaxed);
            if pause && !was_paused {
                warn!("{}. Pausing backup...", reason);
            } else if !pause && was_paused {
                info!("Resources normalized. Resuming backup.");
            }

            thread::sleep(Duration::from_secs(5));
        }
    });

    (should_pause, running, handle)
}

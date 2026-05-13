// resource_monitor.rs - Versão ajustada para backups reais
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use battery::units::ratio::percent;
use sysinfo::{CpuRefreshKind, RefreshKind, System};
use tracing::{info, warn};

pub fn spawn_resource_monitor(
    battery_threshold: Option<u8>,
    cpu_threshold: Option<u8>,
) -> (
    Arc<AtomicBool>, // should_pause
    Arc<AtomicBool>, // running
    std::thread::JoinHandle<()>,
) {
    let should_pause = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));

    let pause_clone = should_pause.clone();
    let running_clone = running.clone();

    let handle = thread::spawn(move || {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_cpu_usage()),
        );
        let battery_manager = battery::Manager::new().ok();

        system.refresh_cpu_all();

        let mut last_battery_check = std::time::Instant::now();
        const BATTERY_CHECK_INTERVAL: Duration = Duration::from_secs(25);

        while running_clone.load(Ordering::Relaxed) {
            let mut pause = false;
            let mut reason = String::new();

            // ==================== BATTERY ====================
            if let Some(threshold) = battery_threshold {
                if last_battery_check.elapsed() >= BATTERY_CHECK_INTERVAL {
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
                    last_battery_check = std::time::Instant::now();
                }
            }

            // ==================== CPU - THRESHOLDS MAIS REALISTAS ====================
            if !pause {
                if let Some(threshold) = cpu_threshold {
                    system.refresh_cpu_all();
                    let usage = system.global_cpu_usage();

                    // Threshold mais sensato para backups intensivos
                    let effective_threshold = threshold.max(65); // mínimo 65% se o usuário definiu algo baixo

                    if usage > effective_threshold as f32 {
                        pause = true;
                        reason = format!("High CPU usage ({:.1}%)", usage);
                    }
                }
            }

            let was_paused = pause_clone.swap(pause, Ordering::Relaxed);

            if pause && !was_paused {
                warn!("⏸️  {} → Pausing backup...", reason);
            } else if !pause && was_paused {
                info!("▶️  Resources OK → Resuming backup.");
            }

            thread::sleep(Duration::from_secs(4));
        }
    });

    (should_pause, running, handle)
}

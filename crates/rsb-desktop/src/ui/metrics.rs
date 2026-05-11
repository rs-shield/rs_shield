use sysinfo::{Disks, System};

#[derive(Clone, Debug)]
pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub memory_total_gb: f64,
    pub memory_used_gb: f64,
    pub disk_usage: f32,
    pub disk_total_gb: f64,
    pub disk_used_gb: f64,
    pub disk_free_gb: f64,
}

pub fn get_system_metrics() -> SystemMetrics {
    let system = System::new_all();

    let cpu_usage = system.global_cpu_usage();

    let memory_total_gb = system.total_memory() as f64 / 1_073_741_824.0;
    let memory_used_gb = system.used_memory() as f64 / 1_073_741_824.0;
    let memory_usage = if memory_total_gb > 0.0 {
        (memory_used_gb / memory_total_gb * 100.0).min(100.0) as f32
    } else {
        0.0
    };

    let mut disk_total_gb = 0.0;
    let mut disk_used_gb = 0.0;
    let mut disk_free_gb = 0.0;
    let mut disk_usage = 0.0;

    // Since we can't easily iterate disks in sysinfo 0.30 without additional methods,
    // we'll estimate based on the filesystem size
    // For now, set realistic defaults that can be updated
    let disks = Disks::new_with_refreshed_list();

    // Tenta encontrar o disco raiz ("/") para evitar duplicação em sistemas como macOS (APFS) ou Linux
    let root_disk = disks
        .iter()
        .find(|d| d.mount_point() == std::path::Path::new("/"));

    if let Some(disk) = root_disk {
        let total = disk.total_space() as f64 / 1_073_741_824.0;
        let available = disk.available_space() as f64 / 1_073_741_824.0;
        let used = total - available;

        disk_total_gb = total;
        disk_used_gb = used;
        disk_free_gb = available;
    } else {
        for disk in &disks {
            let total = disk.total_space() as f64 / 1_073_741_824.0; // Convert to GB
            let available = disk.available_space() as f64 / 1_073_741_824.0;
            let used = total - available;

            disk_total_gb += total;
            disk_used_gb += used;
            disk_free_gb += available;
        }
    }

    if disk_total_gb > 0.0 {
        disk_usage = (disk_used_gb / disk_total_gb * 100.0) as f32;
    }

    SystemMetrics {
        cpu_usage,
        memory_usage,
        memory_total_gb,
        memory_used_gb,
        disk_usage,
        disk_total_gb,
        disk_used_gb,
        disk_free_gb,
    }
}

pub fn format_percentage_color(percentage: f32) -> &'static str {
    match percentage {
        p if p < 50.0 => "text-green-600 dark:text-green-400",
        p if p < 75.0 => "text-yellow-600 dark:text-yellow-400",
        p if p < 90.0 => "text-orange-600 dark:text-orange-400",
        _ => "text-red-600 dark:text-red-400",
    }
}

pub fn format_percentage_bg(percentage: f32) -> &'static str {
    match percentage {
        p if p < 50.0 => "bg-green-100 dark:bg-green-900/30",
        p if p < 75.0 => "bg-yellow-100 dark:bg-yellow-900/30",
        p if p < 90.0 => "bg-orange-100 dark:bg-orange-900/30",
        _ => "bg-red-100 dark:bg-red-900/30",
    }
}

pub fn format_bytes_gb(bytes: f64) -> String {
    format!("{:.2} GB", bytes)
}

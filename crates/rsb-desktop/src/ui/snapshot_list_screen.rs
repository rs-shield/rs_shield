use dioxus::prelude::*;
use crate::ui::{app::AppConfig, i18n::get_texts};
use rsb_sdk::snapshot::repository::SnapshotRepository;
use rsb_sdk::snapshot::snapshot::Snapshot;

#[component]
pub fn SnapshotListScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut snapshots = use_signal(Vec::<Snapshot>::new);
    let mut is_loading = use_signal(|| false);

    let load_snapshots = move |_| {
        is_loading.set(true);
        spawn(async move {
             if let Some(mut base) = dirs::home_dir() {
                base.push(".rsb");
                let repo = SnapshotRepository::new(base);
                if let Ok(list) = repo.list() {
                    snapshots.set(list);
                }
            }
            is_loading.set(false);
        });
    };

    use_effect(move || {
        load_snapshots(());
    });

    rsx! {
        div { class: "card",
            div { class: "flex justify-between items-center mb-6",
                h2 { class: "text-2xl font-bold text-slate-900 dark:text-white", "📸 {texts.nav_snapshots}" }
                button {
                    class: "btn-secondary py-2 px-4 flex items-center gap-2",
                    onclick: load_snapshots,
                    disabled: is_loading(),
                    if is_loading() { "⏳ ..." } else { "🔄 Refresh" }
                }
            }

            if snapshots.read().is_empty() {
                div { class: "text-center py-12 bg-slate-50 dark:bg-slate-800/50 rounded-lg border-2 border-dashed border-slate-200 dark:border-slate-700",
                    span { class: "text-4xl mb-4 block", "📂" }
                    p { class: "text-slate-500 dark:text-slate-400", "{texts.no_snapshots_found}" }
                }
            } else {
                div { class: "overflow-x-auto rounded-lg border border-slate-200 dark:border-slate-700",
                    table { class: "w-full text-sm text-left",
                        thead { class: "text-xs text-slate-700 dark:text-slate-300 uppercase bg-slate-100 dark:bg-slate-800",
                            tr {
                                th { class: "px-4 py-3", "{texts.snapshot_id_col}" }
                                th { class: "px-4 py-3", "{texts.snapshot_date_col}" }
                                th { class: "px-4 py-3", "{texts.snapshot_host_col}" }
                                th { class: "px-4 py-3", "{texts.snapshot_files_col}" }
                                th { class: "px-4 py-3 text-right", "{texts.snapshot_size_col}" }
                            }
                        }
                        tbody { class: "divide-y divide-slate-200 dark:divide-slate-700",
                            for s in snapshots.read().iter() {
                                tr { class: "bg-white dark:bg-slate-900 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors",
                                    td { class: "px-4 py-3 font-mono text-indigo-600 dark:text-indigo-400", "{s.id}" }
                                    td { class: "px-4 py-3 text-slate-600 dark:text-slate-400", "{s.created_at.format(\"%Y-%m-%d %H:%M:%S\")}" }
                                    td { class: "px-4 py-3 text-slate-600 dark:text-slate-400", "{s.hostname}" }
                                    td { class: "px-4 py-3 text-slate-600 dark:text-slate-400", "{s.files_count}" }
                                    td { class: "px-4 py-3 text-right font-medium text-slate-900 dark:text-white", "{human_bytes(s.total_size)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;

    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}
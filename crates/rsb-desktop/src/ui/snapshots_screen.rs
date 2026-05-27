use dioxus::prelude::*;
use crate::ui::{app::AppConfig, i18n::get_texts};

#[derive(Clone, Debug, PartialEq)]
struct SnapshotInfo {
    id: String,
    created_at: String,
    file_count: usize,
    total_size: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum SnapshotMode {
    List,
    Diff,
    Details,
}

#[component]
pub fn SnapshotsScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    
    let mut snapshots = use_signal(Vec::<SnapshotInfo>::new);
    let mut is_loading = use_signal(|| false);
    let mut mode = use_signal(|| SnapshotMode::List);
    let mut selected_snapshot = use_signal(|| Option::<String>::None);
    let mut selected_from = use_signal(|| Option::<String>::None);
    let mut selected_to = use_signal(|| Option::<String>::None);
    let mut diff_result = use_signal(|| Option::<String>::None);
    let mut error_msg = use_signal(|| Option::<String>::None);

    let mut load_snapshots = move |_| {
        is_loading.set(true);
        error_msg.set(None);
        spawn(async move {
            // TODO: Implement snapshot loading via SDK
            // For now, showing placeholder
            is_loading.set(false);
        });
    };

    let on_compare = move |_| {
        if selected_from.read().is_some() && selected_to.read().is_some() {
            mode.set(SnapshotMode::Diff);
            error_msg.set(None);
        } else {
            error_msg.set(Some("Selecione dois snapshots para comparar".to_string()));
        }
    };

    let on_delete = move |snapshot_id: String| {
        // TODO: Implement snapshot deletion via SDK
        error_msg.set(Some(format!("Snapshot {} será deletado", snapshot_id)));
    };

    use_effect(move || {
        load_snapshots(());
    });

    rsx! {
        div { class: "space-y-6",
            // Header
            div { class: "flex justify-between items-center",
                h2 { class: "text-2xl font-bold text-slate-900 dark:text-white", "📸 {texts.nav_snapshots}" }
                button {
                    class: "btn-secondary py-2 px-4 flex items-center gap-2",
                    onclick: move |_| load_snapshots(()),
                    disabled: is_loading(),
                    if is_loading() { "⏳ Carregando..." } else { "🔄 Atualizar" }
                }
            }

            // Error message
            if let Some(error) = error_msg.read().as_ref() {
                div { class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-lg text-red-700 dark:text-red-300 text-sm",
                    "{error}"
                }
            }

            // Mode selector
            div { class: "flex gap-2 mb-4",
                button {
                    class: if *mode.read() == SnapshotMode::List { "btn-primary" } else { "btn-secondary" },
                    onclick: move |_| mode.set(SnapshotMode::List),
                    "📋 Listar"
                }
                button {
                    class: if *mode.read() == SnapshotMode::Diff { "btn-primary" } else { "btn-secondary" },
                    onclick: move |_| mode.set(SnapshotMode::Diff),
                    "📊 Comparar"
                }
            }

            // Content based on mode
            match *mode.read() {
                SnapshotMode::List => rsx! {
                    SnapshotsListView {
                        snapshots: snapshots.read().clone(),
                        is_loading: is_loading(),
                        on_select: move |id: String| selected_snapshot.set(Some(id)),
                        on_delete: on_delete,
                        on_show: move |id: String| {
                            selected_snapshot.set(Some(id.clone()));
                            mode.set(SnapshotMode::Details);
                        }
                    }
                },
                SnapshotMode::Diff => rsx! {
                    SnapshotsDiffView {
                        snapshots: snapshots.read().clone(),
                        selected_from: selected_from.read().clone(),
                        selected_to: selected_to.read().clone(),
                        on_select_from: move |id: String| selected_from.set(Some(id)),
                        on_select_to: move |id: String| selected_to.set(Some(id)),
                        on_compare: on_compare,
                        diff_result: diff_result.read().clone(),
                    }
                },
                SnapshotMode::Details => rsx! {
                    SnapshotsDetailsView {
                        snapshot_id: selected_snapshot.read().clone(),
                        on_back: move |_| mode.set(SnapshotMode::List),
                    }
                },
            }
        }
    }
}

#[component]
fn SnapshotsListView(
    snapshots: Vec<SnapshotInfo>,
    is_loading: bool,
    on_select: EventHandler<String>,
    on_delete: EventHandler<String>,
    on_show: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "space-y-4",
            if snapshots.is_empty() {
                div { class: "text-center py-12 bg-slate-50 dark:bg-slate-800/50 rounded-lg border-2 border-dashed border-slate-200 dark:border-slate-700",
                    span { class: "text-4xl mb-4 block", "📂" }
                    p { class: "text-slate-500 dark:text-slate-400", "Nenhum snapshot encontrado" }
                }
            } else {
                div { class: "grid gap-4",
                    for snapshot in snapshots {
                        {
                            let id_for_show = snapshot.id.clone();
                            let id_for_delete = snapshot.id.clone();
                            rsx! {
                                div { key: "{snapshot.id}", class: "p-4 bg-gradient-to-r from-slate-50 to-slate-100 dark:from-slate-800/50 dark:to-slate-800 border border-slate-200 dark:border-slate-700 rounded-lg hover:shadow-md transition-shadow",
                                    div { class: "flex items-start justify-between mb-3",
                                        div { class: "flex-1",
                                            h3 { class: "font-semibold text-slate-900 dark:text-white text-sm", "📸 {snapshot.id}" }
                                            p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1", "Criado: {snapshot.created_at}" }
                                        }
                                        div { class: "flex gap-2",
                                            button {
                                                class: "btn-secondary text-xs py-1 px-2",
                                                onclick: move |_| on_show.call(id_for_show.clone()),
                                                "👁️ Ver"
                                            }
                                            button {
                                                class: "btn-secondary text-xs py-1 px-2",
                                                onclick: move |_| on_delete.call(id_for_delete.clone()),
                                                "🗑️ Deletar"
                                            }
                                        }
                                    }
                                    div { class: "flex gap-4 text-xs",
                                        div { class: "flex items-center gap-1 text-slate-600 dark:text-slate-400",
                                            span { "📄" }
                                            span { "{snapshot.file_count} arquivos" }
                                        }
                                        div { class: "flex items-center gap-1 text-slate-600 dark:text-slate-400",
                                            span { "💾" }
                                            span { "{format_bytes(snapshot.total_size)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SnapshotsDiffView(
    snapshots: Vec<SnapshotInfo>,
    selected_from: Option<String>,
    selected_to: Option<String>,
    on_select_from: EventHandler<String>,
    on_select_to: EventHandler<String>,
    on_compare: EventHandler<()>,
    diff_result: Option<String>,
) -> Element {
    rsx! {
        div { class: "space-y-6",
            div { class: "grid grid-cols-2 gap-6",
                div { class: "space-y-2",
                    label { class: "block text-sm font-semibold text-slate-700 dark:text-slate-300", "Snapshot Inicial" }
                    select {
                        class: "w-full px-3 py-2 border border-slate-300 dark:border-slate-600 rounded-lg bg-white dark:bg-slate-800 text-slate-900 dark:text-white",
                        onchange: move |e| on_select_from.call(e.value()),
                        option { value: "", "Selecione um snapshot..." }
                        for snapshot in &snapshots {
                            option { 
                                value: snapshot.id.clone(),
                                selected: selected_from.as_ref() == Some(&snapshot.id),
                                "{snapshot.id}"
                            }
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "block text-sm font-semibold text-slate-700 dark:text-slate-300", "Snapshot Final" }
                    select {
                        class: "w-full px-3 py-2 border border-slate-300 dark:border-slate-600 rounded-lg bg-white dark:bg-slate-800 text-slate-900 dark:text-white",
                        onchange: move |e| on_select_to.call(e.value()),
                        option { value: "", "Selecione um snapshot..." }
                        for snapshot in &snapshots {
                            option { 
                                value: snapshot.id.clone(),
                                selected: selected_to.as_ref() == Some(&snapshot.id),
                                "{snapshot.id}"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-primary w-full",
                onclick: move |_| on_compare.call(()),
                "📊 Comparar Snapshots"
            }

            if let Some(result) = diff_result {
                div { class: "p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800/50 rounded-lg text-blue-900 dark:text-blue-100 text-sm font-mono whitespace-pre-wrap overflow-auto max-h-96",
                    "{result}"
                }
            }
        }
    }
}

#[component]
fn SnapshotsDetailsView(
    snapshot_id: Option<String>,
    on_back: EventHandler<()>,
) -> Element {
    let _app_config = use_context::<AppConfig>();

    rsx! {
        div { class: "space-y-4",
            button {
                class: "btn-secondary py-2 px-4 flex items-center gap-2",
                onclick: move |_| on_back.call(()),
                "← Voltar"
            }

            if let Some(id) = snapshot_id {
                div { class: "p-6 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-700 rounded-lg",
                    h3 { class: "font-semibold text-slate-900 dark:text-white mb-4", "📸 Detalhes do Snapshot: {id}" }
                    div { class: "space-y-3 text-sm text-slate-600 dark:text-slate-400",
                        p { "Carregando detalhes..." }
                    }
                }
            } else {
                div { class: "text-center py-12 bg-slate-50 dark:bg-slate-800/50 rounded-lg",
                    p { class: "text-slate-500 dark:text-slate-400", "Selecione um snapshot para visualizar detalhes" }
                }
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
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

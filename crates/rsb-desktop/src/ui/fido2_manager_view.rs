use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

// Importações do SDK (ajuste os nomes conforme a estrutura real do seu rsb_sdk)
use rsb_sdk::credentials::Fido2Manager;

// Importações internas do seu projeto
use crate::ui::app::AppConfig;
use crate::ui::i18n::get_texts;

#[component]
pub fn Fido2ManagerView() -> Element {
    let mut user_id = use_signal(|| String::new());
    let mut credentials = use_signal(|| Vec::new());
    let mut message = use_signal(|| String::new());
    let fido2_manager_arc = use_context::<Arc<Mutex<Fido2Manager>>>();

    // 1. Clonamos para o use_effect
    let fido2_manager_for_effect = fido2_manager_arc.clone();
    use_effect(move || {
        let mgr_arc = fido2_manager_for_effect.clone();
        spawn(async move {
            let mgr = mgr_arc.lock().await;
            credentials.set(mgr.list_credentials());
        });
    });

    // 2. Clonamos para a closure on_register
    let fido2_manager_for_register = fido2_manager_arc.clone();
    let on_register = move |_| {
        // 3. Clonamos novamente para dentro do spawn (futuro assíncrono)
        let mgr_arc = fido2_manager_for_register.clone();

        spawn(async move {
            let id = user_id.read().clone();

            if id.is_empty() {
                message.set("⚠️ Digite um User ID para registrar".into());
                return;
            }

            let mut mgr = mgr_arc.lock().await;
            match mgr.start_registration(&id, &id, &id) {
                Ok(_) => {
                    message.set(format!(
                        "✅ Registro iniciado para {}. Interaja com sua chave USB.",
                        id
                    ));

                    if let Ok(path) = Fido2Manager::default_storage_path() {
                        let _ = mgr.save_to_file(&path);
                    }
                    // Atualizamos a lista enquanto ainda temos o lock
                    credentials.set(mgr.list_credentials());
                }
                Err(e) => message.set(format!("❌ Erro no registro: {}", e)),
            }
        });
    };

    rsx! {
        div { class: "p-6 bg-white rounded-lg shadow-sm border border-gray-200",
            h2 { class: "text-2xl font-bold mb-4 text-gray-800", "🛡️ Gerenciar Chaves de Segurança" }

            div { class: "mb-6 flex gap-2",
                input {
                    class: "border border-gray-300 p-2 rounded flex-1 focus:ring-2 focus:ring-blue-500 outline-none",
                    placeholder: "Identificador de Usuário (ex: admin@rsb)",
                    value: "{user_id}",
                    oninput: move |evt| user_id.set(evt.value())
                }
                button {
                    class: "bg-blue-600 text-white px-4 py-2 rounded font-semibold hover:bg-blue-700 transition-colors",
                    onclick: on_register,
                    "Adicionar Chave"
                }
            }

            if !message.read().is_empty() {
                div { class: "mb-4 p-3 bg-blue-50 border-l-4 border-blue-500 text-blue-700 text-sm", "{message}" }
            }

            h3 { class: "font-bold text-gray-700 mb-3", "Dispositivos Confiáveis ({credentials.read().len()})" }
            ul { class: "bg-gray-50 rounded divide-y divide-gray-200",
                for cred in credentials.read().iter() {
                    li { key: "{cred.user_id}", class: "p-4 flex justify-between items-center hover:bg-gray-100 transition-colors",
                        div {
                            p { class: "font-semibold text-gray-900", "{cred.user_id}" }
                            p { class: "text-xs text-gray-500", "Registrada em: {cred.created_at}" }
                        }
                        div { class: "flex items-center gap-2",
                             span { class: "px-2 py-1 text-xs font-medium bg-green-100 text-green-700 rounded-full", "Ativa" }
                             if let Some(last) = &cred.last_used {
                                 span { class: "text-xs text-gray-400", "Último uso: {last}" }
                             }
                        }
                    }
                }
            }
        }
    }
}

use dioxus::prelude::*;
use rsb_sdk::verify_s3_connection;

#[component]
pub fn S3ConnectionTester(
    bucket: Signal<String>,
    region: Signal<String>,
    endpoint: Signal<String>,
    access_key: Signal<String>,
    secret_key: Signal<String>,
) -> Element {
    let mut is_testing = use_signal(|| false);
    let mut result_message = use_signal(|| None::<String>);
    let mut is_success = use_signal(|| false);

    let on_test_click = move |_| {
        is_testing.set(true);
        result_message.set(None);

        // Captura valores atuais dos signals
        let b = bucket();
        let r = region();
        let e = endpoint();
        let a = access_key();
        let s = secret_key();

        spawn(async move {
            let result = verify_s3_connection(&b, &r, &e, &a, &s).await;

            is_testing.set(false);
            match result {
                Ok(msg) => {
                    is_success.set(true);
                    result_message.set(Some(msg));
                }
                Err(err) => {
                    is_success.set(false);
                    result_message.set(Some(err));
                }
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-3 mt-2",
            div { class: "flex items-center justify-between",
                label { class: "text-sm font-medium text-gray-700 dark:text-gray-300", "Teste de Conexão" }
                button {
                    class: "px-3 py-1.5 text-sm font-medium text-white bg-indigo-600 rounded-md hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors shadow-sm",
                    onclick: on_test_click,
                    disabled: is_testing(),
                    if is_testing() { "Verificando..." } else { "Testar Conexão S3" }
                }
            }

            if let Some(msg) = result_message() {
                div {
                    class: if is_success() {
                        "p-3 text-sm text-green-700 bg-green-50 rounded-md border border-green-200 dark:bg-green-900/20 dark:text-green-400 dark:border-green-800"
                    } else {
                        "p-3 text-sm text-red-700 bg-red-50 rounded-md border border-red-200 dark:bg-red-900/20 dark:text-red-400 dark:border-red-800 break-all"
                    },
                    "{msg}"
                }
            }
        }
    }
}

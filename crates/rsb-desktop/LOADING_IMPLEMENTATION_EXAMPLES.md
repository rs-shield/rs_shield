# Exemplos Práticos de LoadingStyle e OperationTimer

## Exemplo 1: Backup Screen com LoadingStyle

```rust
use dioxus::prelude::*;
use rsb_sdk::CancellationToken;
use crate::ui::{
    app::AppConfig,
    loading_state::{LoadingState, LoadingStyle},
    operation_timer::OperationTimer,
    i18n::get_texts,
};

#[component]
pub fn BackupScreenWithLoading() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    
    let mut is_running = use_signal(|| false);
    let mut progress = use_signal(|| 0.0);
    let mut status_msg = use_signal(|| texts.ready.to_string());
    let mut timer = use_signal(OperationTimer::new);
    let mut elapsed_time = use_signal(String::new);
    let mut estimated_time = use_signal(Option::<String>::None);

    let handle_backup = move |_| {
        if is_running() {
            return;
        }

        is_running.set(true);
        progress.set(0.0);
        status_msg.set(texts.starting.to_string());
        timer.set(OperationTimer::new());  // Reset timer
        elapsed_time.set(String::new());
        estimated_time.set(None);

        // Simular operação de backup
        spawn(async move {
            // Seu código de backup aqui
            for i in 0..100 {
                // Atualizar progresso
                progress.set(i as f64 / 100.0);
                status_msg.set(format!("{} - {}%", texts.executing, i));
                
                // Atualizar timer
                elapsed_time.set(timer.read().elapsed_string());
                estimated_time.set(timer.write().estimate_remaining(progress()));
                
                // Simular trabalho
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            
            is_running.set(false);
            status_msg.set("✅ Backup concluído!".to_string());
        });
    };

    rsx! {
        div { class: "backup-container p-6",
            // Mostrar LoadingState quando operação está rodando
            if is_running() {
                LoadingState {
                    message: status_msg(),
                    style: LoadingStyle::ProgressBar,
                    progress: progress(),
                    elapsed_time: Some(elapsed_time()),
                    estimated_time: estimated_time(),
                }
            } else {
                // UI normal quando não está rodando
                div { class: "form-group",
                    button {
                        class: "w-full btn-primary",
                        onclick: handle_backup,
                        "{texts.start_backup}"
                    }
                }
            }

            p { class: "text-center mt-4 text-sm text-gray-500",
                "{status_msg}"
            }
        }
    }
}
```

## Exemplo 2: Restore Screen com Overlay

```rust
use dioxus::prelude::*;
use crate::ui::{
    loading_state::{LoadingOverlay, LoadingStyle},
    operation_timer::OperationTimer,
};

#[component]
pub fn RestoreScreenWithOverlay() -> Element {
    let mut is_restoring = use_signal(|| false);
    let mut progress = use_signal(|| 0.0);
    let mut message = use_signal(|| "Iniciando restore...".to_string());
    let mut timer = use_signal(OperationTimer::new);
    let mut elapsed = use_signal(String::new);

    let handle_restore = move |_| {
        is_restoring.set(true);
        progress.set(0.0);
        timer.set(OperationTimer::new());

        spawn(async move {
            for i in 0..100 {
                progress.set(i as f64 / 100.0);
                message.set(format!("Restaurando arquivos... {}%", i));
                elapsed.set(timer.read().elapsed_string());
                
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            
            is_restoring.set(false);
        });
    };

    rsx! {
        div { class: "restore-container",
            button {
                class: "btn-primary",
                onclick: handle_restore,
                disabled: is_restoring(),
                "🔄 Iniciar Restore"
            }

            // Overlay que cobre a tela inteira durante restore
            LoadingOverlay {
                is_visible: is_restoring(),
                message: message(),
                style: LoadingStyle::ProgressBar,
                progress: progress(),
                elapsed_time: Some(elapsed()),
            }
        }
    }
}
```

## Exemplo 3: Múltiplos Estilos

```rust
use dioxus::prelude::*;
use crate::ui::loading_state::{LoadingState, LoadingStyle};

#[component]
pub fn LoadingStylesDemo() -> Element {
    rsx! {
        div { class: "p-8 space-y-8",
            h1 { class: "text-2xl font-bold mb-8", "Loading Styles Demo" }

            // Spinner
            div { class: "border p-4 rounded",
                h2 { "Spinner Style" }
                LoadingState {
                    message: "Fazendo backup com spinner...".to_string(),
                    style: LoadingStyle::Spinner,
                }
            }

            // Dots
            div { class: "border p-4 rounded",
                h2 { "Dots Style" }
                LoadingState {
                    message: "Processando com dots...".to_string(),
                    style: LoadingStyle::Dots,
                }
            }

            // Progress Bar
            div { class: "border p-4 rounded",
                h2 { "Progress Bar Style" }
                LoadingState {
                    message: "Progresso do backup...".to_string(),
                    style: LoadingStyle::ProgressBar,
                    progress: 0.65,
                    elapsed_time: Some("2m 30s".to_string()),
                    estimated_time: Some("1m 15s".to_string()),
                }
            }

            // Pulse
            div { class: "border p-4 rounded",
                h2 { "Pulse Style" }
                LoadingState {
                    message: "Sincronizando com pulse...".to_string(),
                    style: LoadingStyle::Pulse,
                }
            }
        }
    }
}
```

## Exemplo 4: Com Timer em Tempo Real

```rust
use dioxus::prelude::*;
use crate::ui::{
    loading_state::LoadingState,
    operation_timer::OperationTimer,
};

#[component]
pub fn RealTimeProgressScreen() -> Element {
    let mut progress = use_signal(|| 0.0);
    let mut is_running = use_signal(|| true);
    let mut timer = use_signal(OperationTimer::new);
    let mut elapsed = use_signal(String::new);
    let mut estimated = use_signal(Option::<String>::None);

    // Atualizar timer a cada 100ms
    use_effect(move || {
        if !is_running() {
            return;
        }

        let interval = gloo_timers::callback::interval(100, move || {
            if is_running() {
                progress.with_mut(|p| {
                    *p += 0.01;
                    if *p >= 1.0 {
                        *p = 1.0;
                        is_running.set(false);
                    }
                });
                
                elapsed.set(timer.read().elapsed_string());
                estimated.set(timer.write().estimate_remaining(progress()));
            }
        });

        move || {
            interval.cancel();
        }
    });

    rsx! {
        LoadingState {
            message: "Operação em progresso...".to_string(),
            style: if progress() >= 1.0 { 
                crate::ui::loading_state::LoadingStyle::Pulse 
            } else { 
                crate::ui::loading_state::LoadingStyle::ProgressBar 
            },
            progress: progress(),
            elapsed_time: Some(elapsed()),
            estimated_time: estimated(),
        }
    }
}
```

## Checklist de Integração

Ao integrar `LoadingStyle` e `OperationTimer` em uma tela:

- [ ] Importar `LoadingState`, `LoadingOverlay`, `LoadingStyle` de `crate::ui::loading_state`
- [ ] Importar `OperationTimer` de `crate::ui::operation_timer`
- [ ] Criar signals para `is_running`, `progress`, `message`, `timer`
- [ ] Criar signals para `elapsed_time` e `estimated_time`
- [ ] Renderizar `LoadingState` ou `LoadingOverlay` quando `is_running` for true
- [ ] Atualizar `timer` ao iniciar operação
- [ ] Atualizar `elapsed_time` e `estimated_time` durante progresso
- [ ] Testar em dark mode
- [ ] Verificar performance com animações

## Dicas de Performance

1. **Atualize o timer com moderação** - Não atualize a cada pixel de progresso, use intervals de 100-200ms
2. **Use `LoadingOverlay` para operações críticas** - Previne que o usuário interaja durante operações importantes
3. **Escolha o estilo apropriado**:
   - `Spinner`: Operações com duração desconhecida
   - `Dots`: Operações mais leves
   - `ProgressBar`: Operações longas com progresso conhecido
   - `Pulse`: Sincronizações em tempo real

## Próximos Passos

1. Atualizar `backup_screen.rs` para usar `LoadingStyle`
2. Atualizar `restore_screen.rs` para usar `LoadingOverlay`
3. Atualizar `prune_screen.rs` para usar timer com progresso
4. Adicionar testes para `OperationTimer`

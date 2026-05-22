# Exemplo de Integração: Backup Screen com LoadingStyle

Este arquivo mostra como integrar `LoadingStyle` e `OperationTimer` no `backup_screen.rs`.

## Mudanças Necessárias

### 1. Adicionar imports no topo de `backup_screen.rs`

```rust
// Adicionar estas linhas após os imports existentes:
use crate::ui::{
    loading_state::{LoadingState, LoadingStyle, LoadingOverlay},
    operation_timer::OperationTimer,
    // ... outros imports
};
```

### 2. Adicionar signals no componente

```rust
#[component]
pub fn BackupScreen() -> Element {
    // ... signals existentes ...
    
    let mut is_running = use_signal(|| false);
    let mut progress = use_signal(|| 0.0);
    
    // ADICIONAR ESTAS 3 LINHAS:
    let mut timer = use_signal(OperationTimer::new);
    let mut elapsed_time = use_signal(String::new);
    let mut estimated_time = use_signal(Option::<String>::None);
    
    // ... resto do código ...
}
```

### 3. Atualizar `handle_backup` para inicializar timer

```rust
let handle_backup = move |_| {
    if is_running() {
        return;
    }

    is_running.set(true);
    progress.set(0.0);
    
    // ADICIONAR ESTAS LINHAS:
    timer.set(OperationTimer::new());  // Reset timer ao iniciar
    elapsed_time.set(String::new());
    estimated_time.set(None);
    
    status_msg.set(texts.starting.to_string());
    
    // ... resto do código ...
}
```

### 4. Atualizar callback de progresso

Encontre a parte com o callback de progresso e atualize:

```rust
// Dentro de handle_backup, encontre este código:
spawn(async move {
    spawn(async move {
        while let Some((cur, tot, msg)) = rx.recv().await {
            if tot > 0 {
                progress.set(cur as f64 / tot as f64);
            }
            status_msg.set(msg);
            
            // ADICIONAR ESTAS LINHAS:
            elapsed_time.set(timer.read().elapsed_string());
            estimated_time.set(timer.write().estimate_remaining(progress()));
        }
    });
    
    // ... resto do código ...
});
```

### 5. Atualizar a renderização

Substitua a parte de renderização de progresso:

**Antes:**
```rust
if is_running() || progress() > 0.0 {
    ProgressBar { progress: progress() }
}
```

**Depois:**
```rust
// Se preferir um overlay (recomendado para backup):
if is_running() {
    LoadingOverlay {
        is_visible: true,
        message: status_msg(),
        style: LoadingStyle::ProgressBar,
        progress: progress(),
        elapsed_time: Some(elapsed_time()),
    }
} else if progress() > 0.0 {
    // Mostrar resultado quando terminar
    div { class: "status-box mt-6",
        p { class: "font-semibold mb-2", "✅ Backup Concluído!" }
        p { "Tempo total: {elapsed_time()}" }
    }
}
```

## Código Completo da Mudança na Renderização

```rust
rsx! {
    div { class: "backup-screen-container p-6",
        h1 { class: "text-2xl font-bold mb-6", "🔄 Backup" }

        // ... formulário de inputs (manter como está) ...
        
        // SEÇÃO DE LOADING COM OVERLAY
        if is_running() {
            LoadingOverlay {
                is_visible: true,
                message: status_msg(),
                style: LoadingStyle::ProgressBar,
                progress: progress(),
                elapsed_time: Some(elapsed_time()),
            }
        }

        // Botões de controle
        if is_running() {
            div { class: "flex gap-3 mb-4",
                button {
                    class: "flex-1 px-4 py-3 bg-red-500 hover:bg-red-600 text-white font-semibold rounded-lg",
                    onclick: handle_cancel,
                    "⏹️ Cancel Backup"
                }
            }
        } else {
            button {
                class: "w-full btn-primary mb-4",
                onclick: handle_backup,
                disabled: is_running(),
                "{texts.start_backup}"
            }
        }

        // Resultado após conclusão
        if !is_running() && progress() > 0.0 {
            div { class: "mt-6 p-4 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg",
                p { class: "font-semibold text-green-800 dark:text-green-300",
                    "✅ Backup Concluído com Sucesso!"
                }
                p { class: "text-sm text-green-700 dark:text-green-400 mt-2",
                    "Tempo total: {elapsed_time()}"
                }
            }
        }

        // Report button (manter como está)
        if let Some(path) = last_report_path() {
            button {
                class: "w-full mt-4 px-4 py-2 bg-slate-500 hover:bg-slate-600 text-white font-semibold rounded-lg",
                onclick: move |_| {
                    let _ = open::that(&path);
                },
                "📄 Open Report"
            }
        }
    }
}
```

## Alternativa: Sem Overlay (Inline Loading)

Se preferir mostrar o loading inline em vez de overlay:

```rust
rsx! {
    div { class: "backup-screen-container p-6",
        h1 { class: "text-2xl font-bold mb-6", "🔄 Backup" }

        if is_running() {
            // Loading inline
            LoadingState {
                message: status_msg(),
                style: LoadingStyle::ProgressBar,
                progress: progress(),
                elapsed_time: Some(elapsed_time()),
                estimated_time: estimated_time(),
            }
        } else {
            // Formulário normal quando não está rodando
            div { class: "form-section",
                // ... inputs de origem, destino, etc ...
            }
        }

        // Botões
        if is_running() {
            button {
                class: "w-full btn-danger",
                onclick: handle_cancel,
                "⏹️ Cancel"
            }
        } else {
            button {
                class: "w-full btn-primary",
                onclick: handle_backup,
                "{texts.start_backup}"
            }
        }
    }
}
```

## Testando a Integração

1. **Compile:**
   ```bash
   cargo check --package rsb-desktop
   ```

2. **Execute:**
   ```bash
   cargo run -p rsb-desktop
   ```

3. **Teste:**
   - Clique em "Start Backup"
   - Verifique se o overlay/loading aparece
   - Verifique se o tempo decorrido atualiza
   - Verifique se o tempo estimado aparece após alguns segundos
   - Teste o botão de cancelamento
   - Teste em dark mode

## Checklist de Integração

- [ ] Imports adicionados
- [ ] Signals adicionados (`timer`, `elapsed_time`, `estimated_time`)
- [ ] Timer inicializado em `handle_backup`
- [ ] Callback de progresso atualiza timer
- [ ] Renderização atualizada com `LoadingState` ou `LoadingOverlay`
- [ ] Compile sem erros
- [ ] Teste em light mode
- [ ] Teste em dark mode
- [ ] Verifique responsividade
- [ ] Commit das mudanças

## Próximas Screens para Integração

1. **restore_screen.rs** - Usar com `LoadingOverlay`
2. **prune_screen.rs** - Usar com `LoadingStyle::Spinner`
3. **realtime_sync_screen.rs** - Usar com `LoadingStyle::Pulse`
4. **schedule_screen.rs** - Usar com `LoadingStyle::Dots`

---

Siga este guia para integrar LoadingStyle em qualquer tela que tenha operações assíncronas!

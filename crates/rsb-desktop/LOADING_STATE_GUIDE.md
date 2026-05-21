# Loading State Components - Desktop UI Improvements

## Overview

O projeto agora possui componentes de **Loading State** para melhorar o feedback visual enquanto operações estão rodando. Isso soluciona o problema de operações demorarem e ficarem sem feedback visual.

## Componentes Disponíveis

### 1. **LoadingState** - Componente básico de loading

```rust
use crate::ui::loading_state::{LoadingState, LoadingStyle};

// Uso simples com spinner
rsx! {
    LoadingState {
        message: "Fazendo backup...".to_string(),
        style: LoadingStyle::Spinner,
    }
}

// Com progresso
rsx! {
    LoadingState {
        message: "Fazendo backup...".to_string(),
        style: LoadingStyle::ProgressBar,
        progress: 0.65,  // 0.0 a 1.0
        elapsed_time: Some("2m 30s".to_string()),
        estimated_time: Some("1m 15s".to_string()),
    }
}
```

### 2. **LoadingOverlay** - Overlay modal com loading

```rust
// Cobre toda a tela com um overlay semi-transparente
rsx! {
    LoadingOverlay {
        is_visible: is_running(),
        message: format!("Processando... {}%", (progress() * 100.0) as u32),
        style: LoadingStyle::ProgressBar,
        progress: progress(),
        elapsed_time: Some(elapsed_time.clone()),
    }
}
```

## Estilos de Loading Disponíveis

1. **Spinner** - Ícone giratório (padrão)
   ```rust
   style: LoadingStyle::Spinner
   ```

2. **Dots** - Três pontos piscando
   ```rust
   style: LoadingStyle::Dots
   ```

3. **ProgressBar** - Barra de progresso com percentual
   ```rust
   style: LoadingStyle::ProgressBar
   ```

4. **Pulse** - Animação de pulso suave
   ```rust
   style: LoadingStyle::Pulse
   ```

## Operation Timer - Calcular Tempo

Use o `OperationTimer` para rastrear tempo decorrido e estimar tempo restante:

```rust
use crate::ui::operation_timer::OperationTimer;

let mut timer = OperationTimer::new();

// No seu loop de progresso:
// ... atualizar progress ...

let elapsed = timer.elapsed_string();  // Ex: "2m 30s"
let estimated = timer.estimate_remaining(0.65);  // Ex: Some("1m 15s")
```

## Exemplo Prático - Backup Screen

```rust
use crate::ui::loading_state::{LoadingState, LoadingStyle};
use crate::ui::operation_timer::OperationTimer;

let mut timer = use_signal(OperationTimer::new);

rsx! {
    if is_running() {
        LoadingState {
            message: format!("{} - {}%", texts.executing, (progress() * 100.0) as u32),
            style: LoadingStyle::ProgressBar,
            progress: progress(),
            elapsed_time: Some(timer.read().elapsed_string()),
            estimated_time: timer.read_mut().estimate_remaining(progress()),
        }
    } else {
        div { /* conteúdo normal */ }
    }
}
```

## Melhorias Visuais

- ✅ **Animações suaves** - Spinners, dots, pulsos
- ✅ **Feedback em tempo real** - Progresso, tempo decorrido, estimativa
- ✅ **Dark mode** - Todas as animações adaptadas ao tema
- ✅ **Responsivo** - Funciona bem em qualquer tamanho de tela
- ✅ **Acessível** - Cores contrastantes, sem dependências de JavaScript

## Estrutura CSS

Todas as animações estão em `styles.css`:

- `.spinner` - Animação giratória
- `.dots-container` - Três pontos piscando
- `.pulse` - Pulso suave
- `.loading-state-container` - Container principal
- `.loading-overlay` - Overlay modal
- `.progress-bar` - Barra de progresso com animação

## Como Integrar em Suas Screens

1. Importe o componente:
```rust
use crate::ui::loading_state::LoadingState;
use crate::ui::operation_timer::OperationTimer;
```

2. Adicione sinais:
```rust
let mut timer = use_signal(OperationTimer::new);
let mut elapsed = use_signal(String::new);
let mut estimated = use_signal(Option::<String>::None);
```

3. Atualize durante a operação:
```rust
spawn(async move {
    // ... sua operação ...
    
    // Periodicamente:
    elapsed.set(timer.read().elapsed_string());
    estimated.set(timer.read_mut().estimate_remaining(progress()));
});
```

4. Renderize o loading state:
```rust
rsx! {
    if is_running() {
        LoadingState {
            message: "Processando...".to_string(),
            elapsed_time: Some(elapsed()),
            estimated_time: estimated(),
            progress,
            style: LoadingStyle::ProgressBar,
        }
    } else {
        // Conteúdo normal
    }
}
```

## Performance

As animações CSS usam `transform` e `opacity` que são otimizadas pelo navegador. Impacto mínimo na performance mesmo com múltiplas operações simultâneas.

## Próximas Melhorias

- [ ] Suporte a status detalhado (ex: "Comprimindo", "Criptografando")
- [ ] Sons de notificação opcionais
- [ ] Histórico de operações com times
- [ ] Cancelamento visual (com countdown antes de realmente cancelar)

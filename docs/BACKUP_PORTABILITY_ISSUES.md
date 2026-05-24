# Análise de Problemas de Portabilidade de Backups

## Bugs Identificados

### 🐛 Bug 1: Lista vazia retornada quando pasta não existe
**Arquivo:** `crates/rsb-sdk/src/storage/mod.rs`  
**Função:** `LocalStorage::list()`  
**Linha:** 59-67

**Problema:**
Quando a pasta `snapshots/` não existe (backup corrompido ou incompleto), a função retorna uma lista vazia em vez de reportar um erro. Isso causa mensagem confusa "No snapshots found" quando deveria ser mais clara.

**Código atual:**
```rust
async fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
    let dir = self.base_path.join(prefix);
    let mut results = Vec::new();

    if !dir.exists() {
        return Ok(results);  // ⚠️ Retorna vazio silenciosamente
    }
    // ...
}
```

**Impacto:**
- Usuário move backup incompleto para outro PC
- Tenta restaurar
- Recebe "No snapshots found" (confuso)
- Não sabe se é problema de permissões ou backup corrompido

**Solução recomendada:**
```rust
async fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
    let dir = self.base_path.join(prefix);
    let mut results = Vec::new();

    if !dir.exists() {
        // ✅ Retorna erro informativo em vez de lista vazia
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Backup structure incomplete: {} directory not found. This may indicate a corrupted backup or incomplete copy.", prefix)
        ));
    }
    // ...
}
```

---

### 🐛 Bug 2: Validação fraca de estrutura de backup
**Arquivo:** `crates/rsb-sdk/src/core/restore.rs`  
**Função:** `perform_restore_with_cancellation()`  
**Linha:** 52-120

**Problema:**
A função não valida se a estrutura do backup está completa antes de começar a restauração. Se a pasta `data/` não existe, o erro só aparece quando tenta restaurar cada arquivo.

**Cenário:**
1. Usuário copia apenas `snapshots/` e `data/enc/` (esqueceu `data/clear/`)
2. Inicia restauração
3. Tudo funciona até encontrar um arquivo não-encriptado
4. Restauração falha no meio do caminho

**Solução recomendada:**
Adicione validação de estrutura antes de começar:

```rust
pub async fn perform_restore_with_cancellation(...) -> Result<ReportData, Box<dyn std::error::Error>> {
    // ✅ Adicionar validação de estrutura
    validate_backup_structure(&*storage).await?;
    
    // ... resto do código
}

async fn validate_backup_structure(storage: &dyn Storage) -> Result<(), Box<dyn std::error::Error>> {
    // Verifica snapshots
    let snapshots = storage.list("snapshots/").await?;
    if snapshots.is_empty() {
        return Err("No snapshots found in backup".into());
    }
    
    // Verifica data directory
    if !storage.exists("data/").await? {
        return Err("Backup data directory missing. Backup may be corrupted or incomplete.".into());
    }
    
    Ok(())
}
```

---

### 🐛 Bug 3: Erro de desencriptação não diferencia chave errada vs dados corrompidos
**Arquivo:** `crates/rsb-sdk/src/core/manifest.rs`  
**Função:** `read_manifest()`  
**Linha:** 95-105

**Problema:**
Quando a desencriptação falha, a mensagem é genérica "Decryption failed". Não diferencia:
- Chave errada ❌
- Dados corrompidos ⚠️
- Arquivo não existe 🚫

**Código atual:**
```rust
Err(e) => {
    error!("❌ Decryption failed for manifest {}: {}", path, e);
    return Err(format!("Decryption failed. Ensure the key is correct: {}", e).into());
}
```

**Solução recomendada:**
```rust
Err(e) => {
    let error_msg = format!("{}", e).to_lowercase();
    let user_message = if error_msg.contains("authentication") || error_msg.contains("tag") {
        "❌ Decryption failed: Wrong encryption key. Use the SAME key that was used to create the backup."
    } else if error_msg.contains("invalid") {
        "⚠️ Backup may be corrupted. Try verifying the backup integrity first."
    } else {
        "❌ Decryption error. Ensure the backup is not corrupted and the key is correct."
    };
    error!("Decryption error for {}: {}", path, e);
    return Err(user_message.into());
}
```

---

### 🐛 Bug 4: Permitir restauração de backup com estrutura parcialmente faltando
**Arquivo:** `crates/rsb-sdk/src/core/restore.rs`  
**Função:** `perform_restore_with_cancellation()`  
**Linha:** 125-145

**Problema:**
Se um arquivo está referenciado no manifest mas não existe na pasta `data/`, a restauração continua e cria um arquivo vazio ou simplesmente pula.

**Código atual:**
```rust
if !storage.exists(&data_path).await? {
    let msg = format!("Missing data for {}: {}", rel_path.display(), metadata.hash);
    info!("{}", msg);
    errors.push(msg);
    continue;  // ⚠️ Continua restauração mesmo com dados faltando
}
```

**Problema:** Usuário pensa que restauração foi sucesso, mas alguns arquivos estão faltando.

**Solução recomendada:**
Adicione flag de tolerância ou modo strict:

```rust
// Novo parâmetro: allow_partial_restore
pub async fn perform_restore_with_cancellation(
    ...
    allow_partial_restore: bool,  // Se false, falha em qualquer arquivo faltando
) -> Result<ReportData, Box<dyn std::error::Error>> {
    
    for (rel_path, metadata) in manifest {
        // ...
        if !storage.exists(&data_path).await? {
            let msg = format!("Missing data for {}: {}", rel_path.display(), metadata.hash);
            
            if !allow_partial_restore {
                return Err(msg.into());  // Falha imediatamente
            }
            
            errors.push(msg);
            continue;
        }
    }
}
```

---

## Recomendações de Melhorias

### 1️⃣ Adicionar modo "Verify Backup"
```bash
rsb verify --backup /caminho/backup --key "senha_se_necessario"
```

Deve:
- ✅ Validar estrutura de diretórios
- ✅ Validar todos os snapshots
- ✅ Verificar integridade dos arquivos (hash)
- ✅ Relatar arquivos faltando ou corrompidos

### 2️⃣ Melhorar mensagens de erro
Adicione logs estruturados com:
- O que foi procurado
- Onde foi procurado
- O que fazer a seguir

**Exemplo melhorado:**
```
❌ Backup restauration failed

Reason: No snapshots found
Location: /path/to/backup/snapshots/

Possible causes:
  1. Backup copied incompletely
  2. Backup corrupted or invalid
  3. Permission denied

Solutions:
  1. Verify backup directory exists: ls -la /path/to/backup/
  2. Try: rsb verify --backup /path/to/backup
  3. Re-copy the backup from the original computer
```

### 3️⃣ Adicionar "Backup Portability Mode"
Para backups que serão movidos entre computadores:

```bash
# Criar backup portável (sem caminhos absolutos)
rsb backup myprofile.toml --portable

# Restaurar de qualquer lugar
rsb restore --backup /qualquer/caminho/backup --output ~/restaurado
```

### 4️⃣ Adicionar ferramenta de diagnóstico
```bash
rsb diagnose --backup /caminho/backup

Output:
✅ Structure: Valid
✅ Snapshots: 5 found
✅ Data integrity: 100% (1000/1000 files verified)
⚠️  Warning: Large file > 4GB detected
ℹ️  Info: Backup can be restored
```

---

## Teste de Cenários Problematic

### ✓ Cenário 1: Backup copiado incompleto
```bash
# No PC A
cp -r /backups/meus-dados/snapshots /media/backup/
# Esqueceu de copiar /data/

# No PC B
rsb restore --backup /media/backup --output ~/restaurado
# ❌ Resultado: Erro no meio da restauração
# ✅ Deveria: Detectar falta de /data/ ANTES de começar
```

### ✓ Cenário 2: Chave de encriptação errada
```bash
# No PC A
rsb backup myprofile.toml --password "senha1"

# No PC B
rsb restore --backup /backup --output ~/restaurado --password "senha2"
# ❌ Resultado: Mensagem genérica "Decryption failed"
# ✅ Deveria: "Wrong encryption key. Use the same key."
```

### ✓ Cenário 3: Pasta snapshots vazia
```bash
# No PC A - Backup corrompido
rm /backups/meus-dados/snapshots/*

# No PC B
rsb restore --backup /backups/meus-dados --output ~/restaurado
# ❌ Resultado: "No snapshots found"
# ✅ Deveria: "Backup structure corrupted: snapshots directory empty"
```

---

## Prioridade de Correção

| Prioridade | Bug | Impacto | Dificuldade |
|-----------|-----|--------|------------|
| 🔴 Alta | Bug 2 (Validação de estrutura) | Restaurações parciais/falhadas | Média |
| 🔴 Alta | Bug 3 (Erro de encriptação) | Usuários confusos | Baixa |
| 🟡 Média | Bug 1 (Lista vazia) | Diagnóstico difícil | Baixa |
| 🟡 Média | Bug 4 (Permitir parcial) | Silent failures | Média |
| 🟢 Baixa | Melhorias gerais | Experiência do usuário | Varia |


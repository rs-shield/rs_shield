# RS Shield - Guia de Portabilidade de Backups

## Problema: "Não consigo mover o backup para outro computador e restaurar"

### Causas Comuns

#### 1. **Falta da Chave de Encriptação**
Se o backup foi criado **com encriptação**, você PRECISA da mesma chave para restaurar em outro computador.

**Solução:**
```bash
# No computador original - Anote a chave usada no backup
rsb list-profiles  # Ver qual perfil foi usado

# No novo computador - Use a MESMA chave
rsb restore --backup /caminho/do/backup --output ~/restaurado --password "SUA_CHAVE"
```

**Desktop UI:**
- Abra a aba "Restaurar"
- Selecione a pasta do backup
- Preencha o campo "Chave de Encriptação" com a **MESMA chave** usada no backup original

**⚠️ Importante:** Se perdeu a chave, os dados encriptados não podem ser recuperados!

---

#### 2. **Backup Sem Snapshots Válidos**
O backup pode estar corrompido ou incompleto.

**Solução:**
Verifique a integridade do backup ANTES de restaurar:

```bash
# CLI
rsb verify --backup /caminho/do/backup --password "CHAVE_SE_ENCRIPTADO"

# Desktop UI
- Aba "Verificar Integridade"
- Selecione a pasta do backup
- Clique em "Iniciar Verificação"
```

Se a verificação falhar, o backup está corrompido e não pode ser restaurado.

---

#### 3. **Estrutura do Backup Inválida**
Um backup válido deve ter esta estrutura:

```
/caminho/do/backup/
├── snapshots/
│   ├── 2026-05-20T...Z.toml  (metadados do backup)
│   └── ...
└── data/
    ├── clear/               (dados não-encriptados)
    │   └── [hash dos arquivos]
    └── enc/                 (dados encriptados)
        └── [hash dos arquivos]
```

**Solução:**
- Verifique se todas as pastas acima existem
- Se faltam pastas, o backup foi copiado incorretamente
- Copie a pasta **INTEIRA** do backup para o outro computador

---

#### 4. **Problemas com S3 (Backup na Nuvem)**
Se o backup original está em S3, o novo computador precisa das credenciais S3.

**Solução:**
```bash
# CLI - Defina as variáveis de ambiente
export AWS_ACCESS_KEY_ID="seu_access_key"
export AWS_SECRET_ACCESS_KEY="seu_secret_key"

# Depois execute o restore
rsb restore --backup s3://seu-bucket/pasta-backup --output ~/restaurado
```

**Desktop UI:**
- Aba "Restaurar"
- Desça até a seção "S3 (Opcional)"
- Preencha:
  - **Bucket S3:** `seu-bucket/pasta-backup`
  - **Região:** `us-east-1` (ou sua região)
  - **Chave de Acesso:** seu access key
  - **Chave Secreta:** seu secret key
  - **Endpoint:** deixe em branco para AWS (ou `https://minio-url:9000` para MinIO)

---

#### 5. **Permissions/Permissões Insuficientes**
Você pode não ter permissão de leitura na pasta do backup ou escrita na pasta de destino.

**Solução Linux/macOS:**
```bash
# Dar permissão de leitura no backup
chmod -R 755 /caminho/do/backup

# Dar permissão de escrita no destino
chmod 755 ~/restaurado
```

**Solução Windows:**
- Clique direito na pasta do backup
- Propriedades → Segurança → Editar
- Selecione seu usuário
- Marque "Controle Total"

---

#### 6. **Erro: "Decryption failed - Chave incorreta"**
A chave inserida está errada ou não corresponde à do backup original.

**Solução:**
- Verifique se está usando a **mesma chave exatamente**
- Sem criptografia? Deixe o campo vazio
- Se o backup foi encriptado mas perdeu a chave, não há recuperação

---

#### 7. **Erro: "No snapshots found"**
O sistema não consegue encontrar os metadados do backup.

**Razões possíveis:**
- ❌ A pasta `snapshots/` está vazia
- ❌ A pasta `snapshots/` não existe
- ❌ O backup foi copiado apenas parcialmente
- ❌ Os arquivos `.toml` foram deletados

**Solução:**
1. Verifique se a pasta tem a estrutura correta
2. Se tiver acesso ao computador original, refaça o backup
3. Caso contrário, o backup está irrecuperável

---

#### 8. **Erro: "Data missing for file"**
O snapshot referencia um arquivo que não existe na pasta `data/`.

**Razões possíveis:**
- O backup foi interrompido durante a cópia
- Alguns arquivos foram deletados depois do backup
- Transferência de rede interrompida

**Solução:**
- Copie o backup novamente do computador original
- Se não tiver acesso, o backup está parcialmente corrompido

---

## Checklist para Mover um Backup com Segurança

✅ **Antes de sair do computador original:**
1. [ ] Anote a **chave de encriptação** (se usou)
2. [ ] Verifique a integridade do backup
   ```bash
   rsb verify --backup /caminho/backup --password "CHAVE"
   ```
3. [ ] Copie a **PASTA INTEIRA** do backup
   ```bash
   # Não copie só um arquivo individual!
   cp -r /caminho/backup /media/pendrive/backup_portavel
   ```

✅ **No novo computador:**
1. [ ] Copie a pasta do backup para um local local
2. [ ] Verifique a integridade novamente
3. [ ] Tente uma restauração de teste em uma pasta vazia
4. [ ] Se der erro, note a mensagem completa

---

## Melhores Práticas

### 🏆 Usar S3 para Portabilidade
Se precisa mover backups entre máquinas frequentemente, **use S3**:

```bash
# Criar backup em S3
rsb backup seu_perfil.toml

# Restaurar de qualquer computador com AWS credentials
rsb restore --backup s3://seu-bucket/dados --output ~/restaurado
```

**Vantagens:**
- Sem precisar copiar arquivos fisicamente
- Acesso de qualquer lugar
- Melhor para segurança (backup remoto)

### 🔄 Backup Portável (Sem S3)
Se vai usar pastas locais:

```bash
# Formato: Sempre use a mesma estrutura
# Computador A: /backups/meus-dados
# Computador B: /backups/meus-dados (ou qualquer caminho)
# O caminho não importa, só a ESTRUTURA INTERNA

# Copie assim:
rsync -av /backups/meus-dados/ /media/backup_portavel/meus-dados/
```

### 🔐 Gerenciar Chaves com Segurança
```bash
# NUNCA deixe a chave em texto plano
# Use um gerenciador de senhas ou .env seguro

# Em um arquivo .env (adicione ao .gitignore):
BACKUP_PASSWORD="sua_senha_super_segura"

# Use assim:
rsb restore --backup /backup --password $BACKUP_PASSWORD --output ~/restaurado
```

---

## Quando Procurar Ajuda

Se após seguir este guia ainda tiver problemas:

1. **Recolha informações:**
   ```bash
   # Verifique a estrutura do backup
   ls -la /caminho/backup/
   ls -la /caminho/backup/snapshots/
   ls -la /caminho/backup/data/
   
   # Verifique permissões
   stat /caminho/backup/
   ```

2. **Execute com verbosidade:**
   ```bash
   # CLI com logs detalhados
   RUST_LOG=debug rsb restore --backup /backup --output ~/restaurado 2>&1 | tee restore.log
   ```

3. **Abra uma issue** com:
   - Output do `ls -la` dos diretórios
   - Mensagem de erro completa
   - Log de execução (restore.log)
   - Qual era o computador original e qual é o novo


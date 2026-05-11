# Testes do RS Shield

Diretório contendo testes de integração e unitários para o projeto RS Shield.

## Estatísticas

- **Total de Testes**: 140
- **Total de Linhas de Código**: 2670
- **Ficheiros de Teste**: 12

## Estrutura dos Testes

### `config_tests.rs`
- Testes de carregamento e serialização de configurações
- Testes de perfis de S3
- Validação de valores padrão
- Testes com encriptação

### `crypto_tests.rs`
- Testes de hashing com Blake3
- Testes de encriptação/desencriptação com AES-256-GCM
- Roundtrip de dados criptografados
- Casos especiais: dados vazios, dados grandes
- Testes de falha com senha errada
- Testes com diferentes tamanhos de senha

### `utils_tests.rs`
- Testes de memory-mapping de ficheiros
- Testes de file walking com filtros
- Testes com .gitignore
- Testes de padrões de exclusão múltipla
- Testes de estruturas profundas de diretórios

### `report_tests.rs`
- Testes de estrutura e campos do relatório
- Testes de diferentes operações (Backup, Restore, Verify, Prune)
- Testes com erros múltiplos
- Testes de timestamps e durações
- Casos edge: zero ficheiros, sem erros

### `realtime_tests.rs`
- Testes de estruturas de sincronização em tempo real
- Testes de tipos de mudanças (Created, Modified, Deleted)
- Testes de estratégias de sincronização
- Testes de fila de mudanças
- Testes de estatísticas de sincronização
- Testes de paths aninhadas

### `integration_tests.rs`
- Testes de cenários completos
- Testes de múltiplos componentes funcionando juntos
- Testes de workflow realístico de backup
- Testes de integração com S3
- Testes de operações de ficheiros + criptografia

### `core_types_tests.rs`
- Testes de constantes (MULTIPART_THRESHOLD, CHUNK_SIZE)
- Testes de ChunkMetadata
- Testes de FileMetadata com e sem chunks
- Testes de encriptação e compressão
- Testes de serialização/desserialização
- Testes de clonagem de estruturas

### `core_resource_monitor_tests.rs`
- Testes de atomic flags (pause, running)
- Testes de swap operations
- Testes de thread-safe access
- Testes de detecção de transição pause/resume
- Testes de monitorização simulada de longa duração
- Testes de múltiplas threads acessando flags

### `core_manifest_tests.rs`
- Testes de geração de snapshot paths
- Testes de formatação de timestamps
- Testes de ordenação de snapshots
- Testes de filtragem de ficheiros .toml
- Testes de busca de snapshot mais recente
- Testes de serialização/desserialização TOML

### `core_prune_tests.rs`
- Testes de filtragem de snapshots para prune
- Testes de keep_last logic
- Testes de detecção de dados órfãos
- Testes de separação de diretórios (clear/enc)
- Testes de coleta de hashes de chunks
- Testes de cálculo de prune em vários cenários

### `core_storage_backend_tests.rs`
- Testes de seleção entre Local e S3 storage
- Testes de S3Config com estrutura aninhada
- Testes de credentials (access_key, secret_key)
- Testes de endpoint override para MinIO
- Testes de fallback de bucket/region/endpoint
- Testes de configuração S3 completa

### `core_operations_tests.rs`
- Testes de validação de caminho de origem
- Testes de modos de backup (full, incremental)
- Testes de filtragem de ficheiros (exclude patterns)
- Testes de ordenação de ficheiros por prioridade
- Testes de operações de restore
- Testes de operações de verify (full vs lite)
- Testes de encriptação durante backup
- Testes de callbacks de progresso
- Testes de flags resume e dry-run

## Como Executar os Testes

### Todos os testes
```bash
cargo test
```

### Testes específicos do diretório
```bash
cargo test --lib
cargo test --bin
```

### Teste específico por padrão de nome
```bash
cargo test test_encrypt_decrypt_roundtrip
cargo test core_ -- --nocapture
```

### Com output detalhado
```bash
cargo test -- --nocapture --test-threads=1
```

### Com threads sequenciais (útil para debugging)
```bash
cargo test -- --test-threads=1
```

## Distribuição de Testes por Módulo

| Módulo | Testes |
|--------|--------|
| config_tests.rs | 4 |
| core_manifest_tests.rs | 14 |
| core_operations_tests.rs | 23 |
| core_prune_tests.rs | 14 |
| core_resource_monitor_tests.rs | 12 |
| core_storage_backend_tests.rs | 13 |
| core_types_tests.rs | 12 |
| crypto_tests.rs | 10 |
| integration_tests.rs | 7 |
| realtime_tests.rs | 13 |
| report_tests.rs | 8 |
| utils_tests.rs | 10 |
| **TOTAL** | **140** |

## Cobertura de Testes

- **Config**: Serialização TOML, S3, encriptação, valores padrão
- **Crypto**: Hashing Blake3, encriptação AES-256-GCM, PBKDF2
- **Utils**: Memory-mapping, file walking, filtros gitignore
- **Report**: Estruturas de dados, timestamps, erros, operações
- **Realtime**: Sincronização, mudanças de ficheiros, estratégias
- **Integration**: Cenários realísticos, múltiplos componentes
- **Core Types**: Metadados de ficheiros, chunks, constantes
- **Core Resource Monitor**: Monitorização de bateria/CPU, atomic flags
- **Core Manifest**: Snapshots, TOML, timestamps, ordenação
- **Core Prune**: Lógica de prune, dados órfãos, hash tracking
- **Core Storage Backend**: Seleção de storage (Local vs S3), configuração
- **Core Operations**: Backup, restore, verify, filtragem, priorização

## Dependências de Teste

- `tempfile`: Criação de diretórios temporários
- `toml`: Serialização/desserialização
- `chrono`: Timestamps
- `rsb_core`: Módulo principal do projeto

## Notas de Desenvolvimento

- Os testes de real-time usam funcionalidades de sistema de ficheiros que podem ser limitadas em alguns ambientes
- Alguns testes usam `TempDir` que é automaticamente limpo quando sair do escopo
- Os testes de encriptação são computacionalmente intensivos (PBKDF2 com 600.000 iterações)

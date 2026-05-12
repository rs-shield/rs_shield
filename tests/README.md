# RS Shield Tests

Directory containing integration and unit tests for the RS Shield project.

## Statistics

- **Total Tests**: 140
- **Total Lines of Code**: 2670
- **Test Files**: 12

## Test Structure

### `config_tests.rs`
- Configuration loading and serialization tests
- S3 profile tests
- Default value validation
- Encryption tests

### `crypto_tests.rs`
- Hashing tests with Blake3
- Encryption/decryption tests with AES-256-GCM
- Encrypted data roundtrip
- Special cases: empty data, large data
- Failure tests with wrong password
- Tests with different password sizes

### `utils_tests.rs`
- File memory-mapping tests
- File walking tests with filters
- .gitignore tests
- Multiple exclusion pattern tests
- Deep directory structure tests

### `report_tests.rs`
- Report structure and field tests
- Different operations tests (Backup, Restore, Verify, Prune)
- Multiple error tests
- Timestamp and duration tests
- Edge cases: zero files, no errors

### `realtime_tests.rs`
- Real-time synchronization structure tests
- Change types tests (Created, Modified, Deleted)
- Synchronization strategy tests
- Change queue tests
- Synchronization statistics tests
- Nested path tests

### `integration_tests.rs`
- Full scenario tests
- Multiple components working together tests
- Realistic backup workflow tests
- S3 integration tests
- File operations + encryption tests

### `core_types_tests.rs`
- Constants tests (MULTIPART_THRESHOLD, CHUNK_SIZE)
- ChunkMetadata tests
- FileMetadata tests with and without chunks
- Encryption and compression tests
- Serialization/deserialization tests
- Structure cloning tests

### `core_resource_monitor_tests.rs`
- Atomic flags tests (pause, running)
- Swap operations tests
- Thread-safe access tests
- Pause/resume transition detection tests
- Simulated long-duration monitoring tests
- Multiple threads accessing flags tests

### `core_manifest_tests.rs`
- Snapshot path generation tests
- Timestamp formatting tests
- Snapshot sorting tests
- .toml file filtering tests
- Latest snapshot search tests
- TOML serialization/deserialization tests

### `core_prune_tests.rs`
- Snapshot filtering for prune tests
- keep_last logic tests
- Orphan data detection tests
- Directory separation tests (clear/enc)
- Chunk hash collection tests
- Prune calculation tests in various scenarios

### `core_storage_backend_tests.rs`
- Selection between Local and S3 storage tests
- S3Config with nested structure tests
- Credentials tests (access_key, secret_key)
- Endpoint override tests for MinIO
- Bucket/region/endpoint fallback tests
- Full S3 configuration tests

### `core_operations_tests.rs`
- Source path validation tests
- Backup modes tests (full, incremental)
- File filtering tests (exclude patterns)
- File sorting by priority tests
- Restore operations tests
- Verify operations tests (full vs lite)
- Encryption during backup tests
- Progress callback tests
- Resume and dry-run flags tests

## How to Run Tests

### All tests
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

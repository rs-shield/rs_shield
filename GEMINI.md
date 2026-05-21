# Estratégia de Recuperação de Acesso e Fallback (Patch FIDO2)

## Problema
Atualmente, o RS Shield utiliza FIDO2 como método primário de autenticação. Se o utilizador perder o dispositivo físico (telemóvel ou chave de segurança) ou se o QR code falhar, o acesso à aplicação é permanentemente bloqueado, pois não existe um mecanismo de redundância ou recuperação.

## Objetivos
1. Implementar **Backup Codes** (Códigos de Recuperação) gerados no momento do registo.
2. Permitir o **Registo de Múltiplas Chaves** FIDO2 para redundância.
3. Manter o nível de segurança "Phishing-resistant" sem retroceder para senhas fracas.

## Estratégia de Implementação

### 1. Backend (`rsb-sdk`)
- **Novas Rotas no Axum**:
    - `/auth/recovery`: Validar um código de uso único.
    - `/register/extra`: Permitir adicionar uma nova credencial a um utilizador existente.
- **Persistência**: 
    - Atualizar o `Fido2Manager` para armazenar uma lista de hashes de códigos de recuperação (ex: PBKDF2 ou Argon2).
    - Alterar o armazenamento de credenciais de 1:1 para 1:N (um utilizador para múltiplas chaves).

### 2. Frontend (`rsb-desktop`)
- **I18n**: Adicionar strings para "Recovery Codes", "Generate Backups" e "Add New Key".
- **Fido2ManagerView**:
    - Adicionar um botão para "Gerar Códigos de Backup".
    - Exibir um modal para o utilizador copiar/imprimir os códigos.
    - Adicionar lista de chaves registadas com opção de remover/adicionar.
- **Login Screen**:
    - Adicionar link "Perdeu o acesso? Use um código de recuperação".

### 3. Fluxo de Recuperação (Backup Codes)
1. O utilizador gera 10 códigos alfa-numéricos de uso único.
2. O SDK guarda apenas o hash destes códigos.
3. No login, se o FIDO2 falhar, o utilizador insere um código.
4. O sistema valida o hash e invalida aquele código específico após o uso.

## Roadmap do Patch

### Fase 1: Redundância de Dispositivos
- [ ] Modificar `Fido2Manager` para suportar `Vec<Credential>`.
- [ ] UI para gerir múltiplas chaves.

### Fase 2: Códigos de Recuperação
- [ ] Lógica de geração de segredos e hashing no `rsb-sdk`.
- [ ] UI de exibição e verificação no `rsb-desktop`.

## Considerações de Segurança
- Os códigos de recuperação devem ter entropia suficiente (ex: 12 caracteres aleatórios).
- O uso de um código de recuperação deve gerar um log de auditoria via `AuditLogger`.
- Incentivar o utilizador a guardar os códigos offline.

---
*Documento gerado para guiar a criação do branch `feature/auth-recovery-options`.*
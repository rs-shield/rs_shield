# RSB Desktop

The modern graphical interface for Rust Shield Backup, built with **Dioxus** and styled with **Tailwind CSS**. Offers a user-friendly experience for managing complex backups.

## 🎨 Design & UX

- **Framework**: Dioxus (Rust).
- **Style**: Tailwind CSS with native **Dark Mode** support.
- **Components**: Standardized Buttons, Inputs, and Cards (see `DESIGN_GUIDE.md`).

## 🖥️ Screens and Features

### 1. Create Profile (`CreateProfileScreen`)
A step-by-step wizard to configure new backups.
- Visual selection of source and destination folders.
- Automatic configuration of default exclusions (e.g., `node_modules`, `*.tmp`).
- Immediate path validation.

### 2. Real-Time Synchronization (`RealtimeSyncScreen`)
A powerful dashboard for continuous monitoring.
- **Status Dashboard**:
  - Real-time counters: Synced Files, Changes, Backups Created, Errors.
  - Visual feedback (Colors: Green for success, Red for error, Blue for info).
- **Workflow**:
  1. Monitors the source folder every 2 seconds.
  2. Syncs changed files to a destination folder (mirror).
  3. Automatically creates an encrypted backup (`.tar.gz` or RSB snapshot) upon every change detection.

### 3. Scheduling (`ScheduleScreen`)
Facilitates backup automation without needing to memorize cron syntax.
- **Cron Generator**: Creates the exact line to add to `crontab` (Linux/macOS).
- **Systemd Generator**: Creates content for `.service` and `.timer` files (Linux).
- Support for automatic inclusion of the encryption key in the generated command.

### 4. Security Keys (`Fido2ManagerView`)
Hardware-based authentication management.
- Register new FIDO2/WebAuthn devices (YubiKeys, etc.).
- List and manage trusted security keys.
- Secure local storage of credentials using AES-256-GCM.

## 🛠️ Development

### File Structure
```
src/ui/
├── realtime_sync_screen.rs  # Lógica de monitoramento e dashboard
├── schedule_screen.rs       # Gerador de comandos de agendamento
├── fido2_manager_view.rs    # Gerenciamento de chaves FIDO2
├── create_profile_screen.rs # Wizard de criação de perfil
├── styles.css               # Definições Tailwind customizadas
└── ...
```

### Compilação

Para compilar com o suporte a Tailwind:

```bash
cargo build --release
```

*Nota: O processo de build integra automaticamente o processamento do Tailwind CSS.*

## 🌍 Internacionalização (i18n)

A interface suporta múltiplos idiomas através do módulo `i18n`. Os textos são carregados dinamicamente com base na configuração do usuário.

## 🔒 Segurança na UI

- Campos de senha (`input type="password"`) para chaves de criptografia.
- As senhas não são salvas em texto plano na configuração, sendo passadas apenas para a execução em memória ou comandos gerados (com aviso ao usuário).
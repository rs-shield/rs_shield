use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Pt,
    En,
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    #[default]
    Dark,
}

#[allow(dead_code)] // textos já traduzidos para ecrãs ainda por ligar
pub struct Translations {
    pub nav_backup: &'static str,
    pub nav_restore: &'static str,
    pub nav_verify: &'static str,
    pub nav_prune: &'static str,
    pub nav_schedule: &'static str,
    pub nav_snapshots: &'static str,
    pub nav_config: &'static str,
    pub nav_fido2: &'static str,
    pub control_panel_title: &'static str,
    pub control_panel_subtitle: &'static str,
    pub backup_title: &'static str,
    pub source_label: &'static str,
    pub dest_label: &'static str,
    pub key_label: &'static str,
    pub key_label_opt: &'static str,
    pub start_backup: &'static str,
    pub executing: &'static str,
    pub restore_title: &'static str,
    pub backup_folder_label: &'static str,
    pub restore_target_label: &'static str,
    pub snapshot_label: &'static str,
    pub restore_files_label: &'static str,
    pub restore_date_label: &'static str,
    pub start_restore: &'static str,
    pub restore_success: &'static str,
    pub verify_title: &'static str,
    pub backup_folder_simple_label: &'static str,
    pub start_verify: &'static str,
    pub schedule_title: &'static str,
    pub schedule_format_label: &'static str,
    pub generate_schedule: &'static str,
    pub schedule_output_label: &'static str,
    pub schedule_hint: &'static str,
    pub config_title: &'static str,
    pub ignore_files_label: &'static str,
    pub backup_mode_label: &'static str,
    pub s3_title: &'static str,
    pub bucket_label: &'static str,
    pub region_label: &'static str,
    pub endpoint_label: &'static str,
    pub s3_hint: &'static str,
    pub s3_remember_button: &'static str,
    pub s3_remember_hint: &'static str,
    pub s3_remember_saving: &'static str,
    pub s3_remember_ok: &'static str,
    pub s3_remember_missing: &'static str,
    pub language_label: &'static str,
    pub generate_report_label: &'static str,
    pub encrypt_patterns_label: &'static str,
    pub low_battery_pause_label: &'static str,
    pub high_cpu_pause_label: &'static str,
    pub compression_level_label: &'static str,
    pub theme_label: &'static str,
    pub theme_system: &'static str,
    pub theme_light: &'static str,
    pub theme_dark: &'static str,
    pub status_label: &'static str,
    pub ready: &'static str,
    pub starting: &'static str,
    pub error_prefix: &'static str,
    pub please_define_paths: &'static str,
    pub define_backup_or_s3: &'static str,
    pub prune_title: &'static str,
    pub prune_path_label: &'static str,
    pub retention_label: &'static str,
    pub start_prune: &'static str,
    pub prune_success: &'static str,
    pub reports_title: &'static str,
    pub real_time_label: &'static str,
    pub system_title: &'static str,
    pub activity_title: &'static str,
    pub stats_title: &'static str,
    pub alerts_title: &'static str,
    pub total_ops_label: &'static str,
    pub last_op_label: &'static str,
    pub backups_count_label: &'static str,
    pub restores_count_label: &'static str,
    pub verifies_count_label: &'static str,
    pub prunes_count_label: &'static str,
    pub system_ok_msg: &'static str,
    pub used_label: &'static str,
    pub total_label: &'static str,
    pub profile_name_label: &'static str,
    pub profile_name_required: &'static str,
    pub select_source_dir: &'static str,
    pub select_dest_dir: &'static str,
    pub source_dir_hint: &'static str,
    pub dest_dir_hint: &'static str,
    pub exclude_patterns_label: &'static str,
    pub exclude_patterns_hint: &'static str,
    pub create_profile_title: &'static str,
    pub create_profile_success: &'static str,
    pub next_steps_title: &'static str,
    pub s3_config_optional: &'static str,
    pub source_folder_selected: &'static str,
    pub dest_folder_selected: &'static str,
    pub select_source_folder: &'static str,
    pub select_dest_folder: &'static str,
    pub error_creating_profile: &'static str,
    pub please_select_config_file: &'static str,
    pub select_config_file: &'static str,
    pub folder_source_selected: &'static str,
    pub folder_dest_selected: &'static str,
    pub advanced_settings_title: &'static str,
    pub compression_hint: &'static str,
    pub battery_pause_hint: &'static str,
    pub cpu_pause_hint: &'static str,
    pub incremental_mode: &'static str,
    pub full_mode: &'static str,
    pub encryption_section_title: &'static str,
    pub exclude_section_title: &'static str,
    pub s3_config_title: &'static str,
    pub crypto_hint: &'static str,
    pub exclude_hint: &'static str,
    pub source_dir_label: &'static str,
    pub dest_dir_label: &'static str,
    pub profile_saved_as: &'static str,
    pub next_steps_hint: &'static str,
    pub backup_folder_selected: &'static str,
    pub select_backup_folder: &'static str,
    pub enter_backup_password: &'static str,
    pub config_file_label: &'static str,
    pub select_config_file_hint: &'static str,
    pub copy_content_to_file: &'static str,
    pub login_title: &'static str,
    pub login_button: &'static str,
    pub auth_required_msg: &'static str,
    pub logout_button: &'static str,
    pub user_identifier: &'static str,
    pub manage_fido2_keys_title: &'static str,
    pub add_new_fido2_key_button: &'static str,
    pub recovery_codes_label: &'static str,
    pub generate_recovery_codes_button: &'static str,
    pub recovery_codes_generated_success: &'static str,
    pub use_recovery_code_link: &'static str,
    pub portable_mode_label: &'static str,
    pub select_backup_directly: &'static str,
    pub portable_backup_loaded: &'static str,
    pub backup_validation_failed: &'static str,
    pub snapshots_found: &'static str,
    pub data_files_found: &'static str,
    pub portable_restore_hint: &'static str,
    pub load_backup_directly: &'static str,
    // Snapshots screen
    pub snapshots_list_label: &'static str,
    pub snapshots_compare_label: &'static str,
    pub snapshots_loading: &'static str,
    pub snapshots_refresh: &'static str,
    pub snapshots_view_button: &'static str,
    pub snapshots_delete_button: &'static str,
    pub snapshots_created: &'static str,
    pub snapshots_files: &'static str,
    pub snapshots_no_found: &'static str,
    pub snapshots_from: &'static str,
    pub snapshots_to: &'static str,
    pub snapshots_compare_button: &'static str,
    pub snapshots_select_two: &'static str,
    pub snapshots_back_button: &'static str,
    // Diagnostics screen
    pub diagnostics_title: &'static str,
    pub diagnostics_run: &'static str,
    pub diagnostics_running: &'static str,
    pub diagnostics_healthy: &'static str,
    pub diagnostics_warnings: &'static str,
    pub diagnostics_critical: &'static str,
    pub diagnostics_storage: &'static str,
    pub diagnostics_config: &'static str,
    pub diagnostics_encryption: &'static str,
    pub diagnostics_fido2: &'static str,
    pub diagnostics_backup: &'static str,
    pub diagnostics_recommendations: &'static str,
    // Backup Integrity
    pub integrity_report_title: &'static str,
    pub integrity_snapshots_count: &'static str,
    pub integrity_normal_files: &'static str,
    pub integrity_encrypted_files: &'static str,
    pub integrity_ready_to_restore: &'static str,
}

pub const PT_STRINGS: Translations = Translations {
    nav_backup: "Backup",
    nav_restore: "Restaurar",
    nav_verify: "Verificar",
    nav_prune: "Limpeza",
    nav_schedule: "Agendar",
    nav_snapshots: "Snapshots",
    nav_config: "Configurações",
    control_panel_title: "Painel de Controlo",
    control_panel_subtitle: "Configure e monitorize as suas tarefas de segurança em tempo real.",
    backup_title: "Executar Backup",
    source_label: "Origem (Pasta a salvar)",
    dest_label: "Destino (Onde guardar o backup)",
    key_label: "Chave de Encriptação",
    key_label_opt: "Chave de Encriptação (Opcional)",
    start_backup: "Iniciar Backup",
    executing: "A Executar...",
    restore_title: "Restaurar Dados",
    backup_folder_label: "Pasta do Backup (Onde estão os dados)",
    restore_target_label: "Restaurar Para (Opcional)",
    start_restore: "Iniciar Restauro",
    snapshot_label: "ID do Snapshot (Opcional, padrão: mais recente)",
    restore_files_label: "Ficheiros a restaurar (Opcional, um padrão por linha)",
    restore_date_label: "Data (Opcional, último snapshot até esta data)",
    restore_success: "Restauro concluído!",
    verify_title: "Verificar Integridade",
    backup_folder_simple_label: "Pasta do Backup",
    start_verify: "Iniciar Verificação",
    schedule_title: "Agendar Backup",
    schedule_format_label: "Formato de Agendamento",
    generate_schedule: "Gerar Comando",
    schedule_output_label: "Comando Gerado (Copie e cole no seu agendador)",
    schedule_hint: "Este comando deve ser configurado no seu sistema (ex: crontab -e).",
    config_title: "Configurações Globais",
    ignore_files_label: "Ficheiros Ignorados (Glob patterns, um por linha)",
    backup_mode_label: "Modo de Backup",
    s3_title: "Armazenamento S3 (Opcional)",
    bucket_label: "Bucket Name",
    region_label: "Region (ex: us-east-1)",
    endpoint_label: "Endpoint URL (para MinIO/R2)",
    s3_hint: "Nota: Se o S3 estiver configurado, o caminho de destino local será ignorado.",
    s3_remember_button: "💾 Lembrar credenciais",
    s3_remember_hint: "Opcional: guarda as chaves cifradas em ~/.rs-shield/s3_credentials.enc (password mestra no keyring do sistema). Sem isto, as chaves ficam só em memória durante esta sessão.",
    s3_remember_saving: "A guardar credenciais...",
    s3_remember_ok: "✅ Credenciais S3 guardadas de forma cifrada.",
    s3_remember_missing: "Preencha a Access Key e a Secret Key.",
    language_label: "Idioma / Language",
    generate_report_label: "Gerar Relatório",
    encrypt_patterns_label: "Padrões de Encriptação (um por linha)",
    low_battery_pause_label: "Pausar com bateria abaixo de (%)",
    high_cpu_pause_label: "Pausar com CPU acima de (%)",
    theme_label: "Tema",
    theme_system: "Sistema",
    theme_light: "Claro",
    theme_dark: "Escuro",
    status_label: "Status:",
    ready: "Pronto",
    starting: "A iniciar...",
    error_prefix: "Erro:",
    please_define_paths: "Por favor, defina a origem e o destino.",
    define_backup_or_s3: "Defina a pasta do backup ou configure S3.",
    compression_level_label: "Nível de Compressão (0-22)",
    prune_title: "Limpeza de Backups",
    prune_path_label: "Pasta do Backup",
    retention_label: "Retenção (Dias)",
    start_prune: "Iniciar Limpeza",
    prune_success: "Limpeza concluída!",
    reports_title: "Relatórios",
    real_time_label: "Tempo Real",
    system_title: "🖥️ Sistema",
    activity_title: "Atividade",
    stats_title: "Estatísticas",
    alerts_title: "⚠️ Alertas",
    total_ops_label: "Total de Operações:",
    last_op_label: "Última Operação:",
    backups_count_label: "Backups",
    restores_count_label: "Restaurações",
    verifies_count_label: "Verificações",
    prunes_count_label: "Limpezas",
    system_ok_msg: "Sistema operacional normalmente. Nenhum alerta detectado.",
    used_label: "Usado",
    total_label: "Total",
    profile_name_label: "Nome do Perfil",
    profile_name_required: "❌ Nome do perfil é obrigatório",
    select_source_dir: "❌ Selecione um diretório de origem",
    select_dest_dir: "❌ Selecione um diretório de destino",
    source_dir_hint: "Todos os ficheiros nesta pasta serão incluídos no backup",
    dest_dir_hint: "Este diretório armazenará todos os snapshots dos backups",
    exclude_patterns_label: "🚫 Padrões de Exclusão",
    exclude_patterns_hint: "Especifique ficheiros e pastas a excluir do backup. Use um padrão por linha",
    create_profile_title: "📋 Criar Novo Perfil de Backup",
    create_profile_success: "✅ Perfil '{0}' criado com sucesso! Use 'rsb backup ~/.rsb-desktop/{0}.toml' na linha de comandos.",
    next_steps_title: "✅ Próximos Passos",
    s3_config_optional: "☁️ Configuração S3 (Opcional)",
    source_folder_selected: "Pasta de origem selecionada",
    dest_folder_selected: "Pasta de destino selecionada",
    select_source_folder: "Selecione a pasta a ser feita backup",
    select_dest_folder: "Selecione onde guardar os backups",
    error_creating_profile: "❌ Erro ao criar perfil: {0}",
    please_select_config_file: "Por favor, selecione um ficheiro de configuração.",
    select_config_file: "❌ Selecione um ficheiro de configuração",
    folder_source_selected: "Pasta de origem selecionada",
    folder_dest_selected: "Pasta de destino selecionada",
    advanced_settings_title: "⚙️ Configurações Avançadas",
    compression_hint: "0 = Sem compressão, 11 = Máxima compressão",
    battery_pause_hint: "Pausa o backup quando a bateria cai abaixo deste nível",
    cpu_pause_hint: "Pausa o backup quando o uso de CPU ultrapassa este nível",
    incremental_mode: "📈 Incremental (apenas mudanças)",
    full_mode: "📦 Completo (Full)",
    encryption_section_title: "🔐 Padrões de Criptografia",
    exclude_section_title: "🚫 Padrões de Exclusão",
    s3_config_title: "☁️ Configuração S3",
    crypto_hint: "Um padrão por linha. Ex: *.pdf, *.docx, private/",
    exclude_hint: "Um padrão por linha. Ex: *.tmp, node_modules/, .git/",
    source_dir_label: "📁 Diretório de Origem",
    dest_dir_label: "Diretório de Destino",
    profile_saved_as: "O arquivo será salvo como: {0}.toml",
    next_steps_hint: "Após criar o perfil, poderá editá-lo para ajustar configurações adicionais de backup, criptografia, etc.",
    backup_folder_selected: "Pasta de backup selecionada",
    select_backup_folder: "❌ Selecione pasta de backup",
    enter_backup_password: "❌ Digite uma senha para o backup",
    config_file_label: "📋 Ficheiro de Configuração (.toml)",
    select_config_file_hint: "Selecione um ficheiro",
    copy_content_to_file: "Copie este conteúdo para um ficheiro .service",
    nav_fido2: "Chaves FIDO2",
    login_title: "Autenticação Requerida",
    login_button: "Entrar com Chave de Segurança",
    auth_required_msg: "Por favor, autentique-se para aceder às funcionalidades de backup e restauro.",
    logout_button: "Sair",
    user_identifier: "Identificador de Usuário",
    manage_fido2_keys_title: "Gerir Chaves FIDO2",
    add_new_fido2_key_button: "Adicionar Nova Chave de Segurança",
    recovery_codes_label: "Códigos de Recuperação",
    recovery_codes_generated_success: "Códigos de recuperação gerados com sucesso! Guarde-os em local seguro.",
    generate_recovery_codes_button: "Gerar Códigos de Recuperação",
    use_recovery_code_link: "Perdeu o acesso? Use um código de recuperação",
    portable_mode_label: "📱 Modo Portável",
    select_backup_directly: "Selecione a pasta de backup",
    portable_backup_loaded: "✅ Backup portável carregado",
    backup_validation_failed: "❌ Validação do backup falhou",
    snapshots_found: "Snapshots encontrados",
    data_files_found: "Ficheiros de dados encontrados",
    portable_restore_hint: "Restaure diretamente de uma pasta de backup sem ficheiro de configuração",
    load_backup_directly: "📁 Carregar Backup Diretamente",
    // Snapshots screen
    snapshots_list_label: "📋 Listar",
    snapshots_compare_label: "📊 Comparar",
    snapshots_loading: "⏳ Carregando...",
    snapshots_refresh: "🔄 Atualizar",
    snapshots_view_button: "👁️ Ver",
    snapshots_delete_button: "🗑️ Deletar",
    snapshots_created: "Criado",
    snapshots_files: "arquivos",
    snapshots_no_found: "Nenhum snapshot encontrado",
    snapshots_from: "Snapshot Inicial",
    snapshots_to: "Snapshot Final",
    snapshots_compare_button: "📊 Comparar Snapshots",
    snapshots_select_two: "Selecione dois snapshots para comparar",
    snapshots_back_button: "← Voltar",
    // Diagnostics screen
    diagnostics_title: "🔧 Diagnósticos do Sistema",
    diagnostics_run: "🔄 Executar Diagnóstico",
    diagnostics_running: "⏳ Executando...",
    diagnostics_healthy: "✅ Saudável",
    diagnostics_warnings: "⚠️ Avisos",
    diagnostics_critical: "❌ Crítico",
    diagnostics_storage: "Armazenamento",
    diagnostics_config: "Configuração",
    diagnostics_encryption: "Criptografia",
    diagnostics_fido2: "FIDO2",
    diagnostics_backup: "Último Backup",
    diagnostics_recommendations: "💡 Recomendações",
    // Backup Integrity
    integrity_report_title: "✅ Relatório de Integridade",
    integrity_snapshots_count: "Snapshots",
    integrity_normal_files: "Arquivos (normais)",
    integrity_encrypted_files: "Arquivos (encriptados)",
    integrity_ready_to_restore: "✅ Seu backup está pronto para ser restaurado em outro computador!",
};

pub const EN_STRINGS: Translations = Translations {
    nav_backup: "Backup",
    nav_restore: "Restore",
    nav_verify: "Verify",
    nav_prune: "Prune",
    nav_schedule: "Schedule",
    nav_snapshots: "Snapshots",
    nav_config: "Settings",
    control_panel_title: "Control Panel",
    control_panel_subtitle: "Configure and monitor your security tasks in real-time.",
    backup_title: "Run Backup",
    source_label: "Source (Folder to save)",
    dest_label: "Destination (Where to save backup)",
    key_label: "Encryption Key",
    key_label_opt: "Encryption Key (Optional)",
    start_backup: "Start Backup",
    executing: "Running...",
    restore_title: "Restore Data",
    backup_folder_label: "Backup Folder (Where data is located)",
    restore_target_label: "Restore To (Optional)",
    start_restore: "Start Restore",
    snapshot_label: "Snapshot ID (Optional, default: latest)",
    restore_files_label: "Files to restore (Optional, one pattern per line)",
    restore_date_label: "Date (Optional, latest snapshot at or before it)",
    restore_success: "Restore completed!",
    verify_title: "Verify Integrity",
    backup_folder_simple_label: "Backup Folder",
    start_verify: "Start Verification",
    schedule_title: "Schedule Backup",
    schedule_format_label: "Schedule Format",
    generate_schedule: "Generate Command",
    schedule_output_label: "Generated Command (Copy and paste into your scheduler)",
    schedule_hint: "This command must be configured in your system (e.g., crontab -e).",
    config_title: "Global Settings",
    ignore_files_label: "Ignored Files (Glob patterns, one per line)",
    backup_mode_label: "Backup Mode",
    s3_title: "S3 Storage (Optional)",
    bucket_label: "Bucket Name",
    region_label: "Region (ex: us-east-1)",
    endpoint_label: "Endpoint URL (for MinIO/R2)",
    s3_hint: "Note: If S3 is configured, the local destination path will be ignored.",
    s3_remember_button: "💾 Remember credentials",
    s3_remember_hint: "Optional: stores the keys encrypted in ~/.rs-shield/s3_credentials.enc (master password in the OS keyring). Otherwise the keys are kept in memory for this session only.",
    s3_remember_saving: "Saving credentials...",
    s3_remember_ok: "✅ S3 credentials saved encrypted.",
    s3_remember_missing: "Fill in the Access Key and the Secret Key.",
    language_label: "Language",
    generate_report_label: "Generate Report",
    encrypt_patterns_label: "Encryption Patterns (one per line)",
    low_battery_pause_label: "Pause when battery is below (%)",
    high_cpu_pause_label: "Pause when CPU is above (%)",
    theme_label: "Theme",
    theme_system: "System",
    theme_light: "Light",
    theme_dark: "Dark",
    status_label: "Status:",
    ready: "Ready",
    starting: "Starting...",
    error_prefix: "Error:",
    please_define_paths: "Please define source and destination.",
    define_backup_or_s3: "Define backup folder or configure S3.",
    compression_level_label: "Compression Level (0-22)",
    prune_title: "Prune Backups",
    prune_path_label: "Backup Folder",
    retention_label: "Retention (Days)",
    start_prune: "Start Prune",
    prune_success: "Prune completed!",
    reports_title: "Reports",
    real_time_label: "Real Time",
    system_title: "🖥️ System",
    activity_title: "Activity",
    stats_title: "Statistics",
    alerts_title: "⚠️ Alerts",
    total_ops_label: "Total Operations:",
    last_op_label: "Last Operation:",
    backups_count_label: "Backups",
    restores_count_label: "Restores",
    verifies_count_label: "Verifications",
    prunes_count_label: "Prunes",
    system_ok_msg: "System operating normally. No alerts detected.",
    used_label: "Used",
    total_label: "Total",
    profile_name_label: "Profile Name",
    profile_name_required: "❌ Profile name is required",
    select_source_dir: "❌ Select a source directory",
    select_dest_dir: "❌ Select a destination directory",
    source_dir_hint: "All files in this folder will be included in the backup",
    dest_dir_hint: "This directory will store all backup snapshots",
    exclude_patterns_label: "🚫 Exclude Patterns",
    exclude_patterns_hint: "Specify files and folders to exclude from backup. One pattern per line",
    create_profile_title: "📋 Create New Backup Profile",
    create_profile_success: "✅ Profile '{0}' created successfully! From the command line: 'rsb backup ~/.rsb-desktop/{0}.toml'.",
    next_steps_title: "✅ Next Steps",
    s3_config_optional: "☁️ S3 Configuration (Optional)",
    source_folder_selected: "Source folder selected",
    dest_folder_selected: "Destination folder selected",
    select_source_folder: "Select the folder to be backed up",
    select_dest_folder: "Select where to save backups",
    error_creating_profile: "❌ Error creating profile: {0}",
    please_select_config_file: "Please select a configuration file.",
    select_config_file: "❌ Select a configuration file",
    folder_source_selected: "Source folder selected",
    folder_dest_selected: "Destination folder selected",
    advanced_settings_title: "⚙️ Advanced Settings",
    compression_hint: "0 = No compression, 11 = Maximum compression",
    battery_pause_hint: "Pauses backup when battery drops below this level",
    cpu_pause_hint: "Pauses backup when CPU usage exceeds this level",
    incremental_mode: "📈 Incremental (changes only)",
    full_mode: "📦 Complete (Full)",
    encryption_section_title: "🔐 Encryption Patterns",
    exclude_section_title: "🚫 Exclude Patterns",
    s3_config_title: "☁️ S3 Configuration",
    crypto_hint: "One pattern per line. Ex: *.pdf, *.docx, private/",
    exclude_hint: "One pattern per line. Ex: *.tmp, node_modules/, .git/",
    source_dir_label: "📁 Source Directory",
    dest_dir_label: "Destination Directory",
    profile_saved_as: "The file will be saved as: {0}.toml",
    next_steps_hint: "After creating the profile, you can edit it to adjust additional backup, encryption settings, etc.",
    backup_folder_selected: "Backup folder selected",
    select_backup_folder: "❌ Select backup folder",
    enter_backup_password: "❌ Enter a password for the backup",
    config_file_label: "📋 Configuration File (.toml)",
    nav_fido2: "Keys",
    select_config_file_hint: "Select a file",
    copy_content_to_file: "Copy this content to a .service file",
    login_title: "Authentication Required",
    login_button: "Login with Security Key ",
    auth_required_msg: "Please authenticate to access backup and restore features.",
    logout_button: "Logout",
    user_identifier: "User Identifier",
    manage_fido2_keys_title: "Manage FIDO2 Keys",
    add_new_fido2_key_button: "Add New Security Key",
    recovery_codes_label: "Recovery Codes",
    recovery_codes_generated_success: "Recovery codes successfully generated! Store them in a safe place.",
    generate_recovery_codes_button: "Generate Recovery Codes",
    use_recovery_code_link: "Lost access? Use a recovery code",
    portable_mode_label: "📱 Portable Mode",
    select_backup_directly: "Select backup folder",
    portable_backup_loaded: "✅ Portable backup loaded",
    backup_validation_failed: "❌ Backup validation failed",
    snapshots_found: "Snapshots found",
    data_files_found: "Data files found",
    portable_restore_hint: "Restore directly from a backup folder without a configuration file",
    load_backup_directly: "📁 Load Backup Directly",
    // Snapshots screen
    snapshots_list_label: "📋 List",
    snapshots_compare_label: "📊 Compare",
    snapshots_loading: "⏳ Loading...",
    snapshots_refresh: "🔄 Refresh",
    snapshots_view_button: "👁️ View",
    snapshots_delete_button: "🗑️ Delete",
    snapshots_created: "Created",
    snapshots_files: "files",
    snapshots_no_found: "No snapshots found",
    snapshots_from: "Initial Snapshot",
    snapshots_to: "Final Snapshot",
    snapshots_compare_button: "📊 Compare Snapshots",
    snapshots_select_two: "Select two snapshots to compare",
    snapshots_back_button: "← Back",
    // Diagnostics screen
    diagnostics_title: "🔧 System Diagnostics",
    diagnostics_run: "🔄 Run Diagnostics",
    diagnostics_running: "⏳ Running...",
    diagnostics_healthy: "✅ Healthy",
    diagnostics_warnings: "⚠️ Warnings",
    diagnostics_critical: "❌ Critical",
    diagnostics_storage: "Storage",
    diagnostics_config: "Configuration",
    diagnostics_encryption: "Encryption",
    diagnostics_fido2: "FIDO2",
    diagnostics_backup: "Last Backup",
    diagnostics_recommendations: "💡 Recommendations",
    // Backup Integrity
    integrity_report_title: "✅ Integrity Report",
    integrity_snapshots_count: "Snapshots",
    integrity_normal_files: "Files (normal)",
    integrity_encrypted_files: "Files (encrypted)",
    integrity_ready_to_restore: "✅ Your backup is ready to be restored on another computer!",
};

pub fn get_texts(lang: Language) -> &'static Translations {
    match lang {
        Language::Pt => &PT_STRINGS,
        Language::En => &EN_STRINGS,
    }
}

/// Fills `{name}` placeholders of a translated template.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{}}}", name), value);
    }
    out
}

/// Declares [`KeyTexts`] with its Portuguese and English values side by
/// side, so no language can miss a string.
macro_rules! key_texts {
    ($($field:ident: $pt:expr, $en:expr;)*) => {
        /// Texts of the key lifecycle, repair and FIDO2 management screens.
        pub struct KeyTexts {
            $(pub $field: &'static str,)*
        }

        pub const PT_KEY_TEXTS: KeyTexts = KeyTexts { $($field: $pt,)* };
        pub const EN_KEY_TEXTS: KeyTexts = KeyTexts { $($field: $en,)* };

        impl KeyTexts {
            /// Every (field, text) pair, for consistency tests.
            #[cfg(test)]
            pub fn all(&self) -> Vec<(&'static str, &'static str)> {
                vec![$((stringify!($field), self.$field),)*]
            }
        }
    };
}

key_texts! {
    // Repository key screen
    title: "🔑 Chave do repositório", "🔑 Repository Key";
    intro:
        "keys/repo.key guarda a chave de dados de um repositório cifrado (formato v3), protegida pela sua palavra-passe. Perdê-lo torna todos os backups v3 ilegíveis, mesmo com a palavra-passe: exporte uma cópia. Equivalente a `rsb key init/migrate/rotate/export/import/change-password`.",
        "keys/repo.key holds the data key of an encrypted repository (format v3), wrapped by your password. Losing it makes every v3 backup unreadable, even with the password: export a copy. Same as `rsb key init/migrate/rotate/export/import/change-password`.";
    profile_label: "📋 Perfil (opcional)", "📋 Profile (optional)";
    profile_placeholder: "Selecione um perfil", "Select a profile";
    profile_loaded: "✅ Perfil carregado: {path}", "✅ Profile loaded: {path}";
    repo_label: "Pasta do repositório", "Repository folder";
    repo_placeholder:
        "Pasta do backup (repositório); vazio: bucket S3 das Configurações",
        "Backup (repository) folder; empty: S3 bucket of Settings";
    password_label: "Palavra-passe do repositório (atual)", "Repository password (current)";
    need_password: "Introduza a palavra-passe do repositório.", "Enter the repository password.";
    working: "A trabalhar na chave do repositório...", "Working on the repository key...";
    status_section: "Estado", "Status";
    check_button: "🔄 Analisar repositório", "🔄 Check repository";
    checking: "🔍 A analisar o repositório (só leitura)...", "🔍 Checking the repository (read-only)...";
    status_no_key:
        "Sem keys/repo.key: o repositório ainda não é v3 (crie a chave abaixo ou faça um backup cifrado).",
        "No keys/repo.key: the repository is not v3 yet (create the key below, or run an encrypted backup).";
    status_key: "✅ keys/repo.key presente (id da chave {id}).", "✅ keys/repo.key present (key id {id}).";
    status_rotating:
        "⚠️ Uma rotação da chave foi interrompida: execute Rodar de novo para a retomar.",
        "⚠️ A key rotation was interrupted: run Rotate again to resume it.";
    status_encrypted:
        "{objects} objeto(s) cifrado(s) ({bytes}) e {manifests} manifesto(s) cifrado(s).",
        "{objects} encrypted object(s) ({bytes}) and {manifests} encrypted manifest(s).";
    status_legacy:
        "⚠️ {objects} objeto(s) v1/v2 ({bytes}) e {manifests} manifesto(s) v1/v2 por migrar.",
        "⚠️ {objects} v1/v2 object(s) ({bytes}) and {manifests} v1/v2 manifest(s) still to migrate.";
    status_legacy_secret:
        "O ficheiro da chave ainda guarda a palavra-passe antiga (v1/v2); a migração remove-a.",
        "The key file still holds the legacy (v1/v2) password; the migration removes it.";
    // Init
    init_section: "Criar a chave (init)", "Create the key (init)";
    init_hint:
        "Cria keys/repo.key (formato v3). Se o repositório já tem backups cifrados, use a palavra-passe deles.",
        "Creates keys/repo.key (format v3). If this repository already has encrypted backups, use their password.";
    new_password: "Nova palavra-passe", "New password";
    confirm_password: "Confirmar a nova palavra-passe", "Confirm new password";
    password_empty: "A nova palavra-passe não pode estar vazia.", "The new password must not be empty.";
    password_mismatch: "As palavras-passe não coincidem.", "The passwords do not match.";
    init_button: "🆕 Criar o ficheiro da chave", "🆕 Create key file";
    init_exists:
        "keys/repo.key já existe; use Alterar palavra-passe.",
        "keys/repo.key already exists; use Change password.";
    init_done: "✅ Criado keys/repo.key (formato v3).", "✅ Created keys/repo.key (format v3).";
    init_legacy:
        "ℹ️ O repositório tem dados v1/v2: re-cifre-os com Migrar (até lá a palavra-passe não pode ser alterada).",
        "ℹ️ This repository holds v1/v2 data: re-encrypt it with Migrate (until then the password cannot be changed).";
    export_reminder:
        "⚠️ Perder keys/repo.key torna todos os backups v3 ilegíveis, mesmo com a palavra-passe: exporte agora uma cópia (Exportar ficheiro da chave) e guarde-a em segurança.",
        "⚠️ Losing keys/repo.key makes every v3 backup unreadable, even with the password: export a copy now (Export key file) and store it safely.";
    // Migrate
    migrate_section: "Migrar dados v1/v2 para v3", "Migrate v1/v2 data to v3";
    migrate_hint:
        "Re-cifra cada objeto e manifesto v1/v2 como v3, verifica o repositório e remove a palavra-passe antiga. Pode ser retomada: se for interrompida ou parada, execute-a de novo.",
        "Re-encrypts every v1/v2 object and manifest as v3, verifies the repository and drops the legacy password. Resumable: if interrupted or stopped, run it again.";
    migrate_button: "🔁 Migrar…", "🔁 Migrate…";
    migrate_confirm:
        "Vai re-cifrar como v3 {objects} objeto(s) v1/v2 ({bytes}) e {manifests} manifesto(s) v1/v2 e verificar todo o repositório. Nada é apagado.",
        "This re-encrypts {objects} v1/v2 object(s) ({bytes}) and {manifests} v1/v2 manifest(s) as v3 and verifies the whole repository. Nothing is deleted.";
    migrate_nothing:
        "Não há dados v1/v2: a migração só verifica o repositório e remove a palavra-passe antiga, se estiver guardada.",
        "No v1/v2 data left: the migration only verifies the repository and removes the legacy password, if stored.";
    migrate_running: "🔁 A migrar para v3...", "🔁 Migrating to v3...";
    migrate_done:
        "✅ Migração concluída: {migrated} objeto(s) re-cifrado(s) como v3 ({already} já eram v3), {manifests} manifesto(s) reescrito(s), {verified} objeto(s) verificado(s).",
        "✅ Migration complete: {migrated} object(s) re-encrypted as v3 ({already} already v3), {manifests} manifest(s) rewritten, {verified} object(s) verified.";
    migrate_secret_removed:
        "🔑 A palavra-passe antiga (v1/v2) foi removida de keys/repo.key.",
        "🔑 The legacy (v1/v2) password was removed from keys/repo.key.";
    migrate_unreferenced:
        "ℹ️ {count} objeto(s) v1/v2 não são usados por nenhum snapshot (a limpeza remove-os).",
        "ℹ️ {count} v1/v2 object(s) are not referenced by any snapshot (prune garbage-collects them).";
    migrate_unreadable:
        "⚠️ {count} objeto(s) não referenciado(s) não puderam ser decifrados e ficaram como estavam.",
        "⚠️ {count} unreferenced object(s) could not be decrypted and were left as they are.";
    // Rotate
    rotate_section: "Rodar a chave de dados", "Rotate the data key";
    rotate_hint:
        "Gera uma nova chave de dados e re-cifra cada objeto e manifesto com ela; a chave antiga é removida depois de tudo verificado. Pode ser retomada. Requer a migração v1/v2 concluída.",
        "Generates a new data key and re-encrypts every object and manifest under it; the old key is dropped once everything verifies. Resumable. Requires the v1/v2 migration to be complete.";
    rotate_button: "♻️ Rodar…", "♻️ Rotate…";
    rotate_confirm:
        "Vai ler e reescrever com uma chave nova {objects} objeto(s) ({bytes}) e {manifests} manifesto(s). As cópias antigas de keys/repo.key (exportações) deixam de abrir os dados.",
        "This reads and rewrites {objects} object(s) ({bytes}) and {manifests} manifest(s) under a new key. Old copies of keys/repo.key (exports) will no longer open the data.";
    rotate_resume:
        "Uma rotação interrompida vai ser retomada (os objetos já re-cifrados são saltados).",
        "An interrupted rotation will be resumed (objects already re-encrypted are skipped).";
    rotate_running: "♻️ A rodar a chave...", "♻️ Rotating the key...";
    rotate_done:
        "✅ Chave rodada: {old} → {new} ({objects} objeto(s) e {manifests} manifesto(s) re-cifrados, {verified} objeto(s) verificado(s)).",
        "✅ Key rotated: {old} → {new} ({objects} object(s) and {manifests} manifest(s) re-encrypted, {verified} object(s) verified).";
    rotate_unreadable:
        "⚠️ {count} objeto(s) não referenciado(s) não puderam ser re-cifrados e ficaram como estavam (a limpeza remove-os).",
        "⚠️ {count} unreferenced object(s) could not be re-encrypted and were left as they are (prune garbage-collects them).";
    rotate_export_reminder:
        "⚠️ As cópias antigas de keys/repo.key já não abrem os dados: exporte agora o novo ficheiro da chave.",
        "⚠️ Old copies of keys/repo.key no longer open the data: export the new key file now.";
    // Shared
    confirm_title: "Confirmar", "Confirm";
    confirm_button: "✅ Confirmar", "✅ Confirm";
    cancel_button: "✖ Cancelar", "✖ Cancel";
    stop_button: "⏹️ Parar", "⏹️ Stop";
    stopping:
        "⏹️ A parar depois do objeto atual...",
        "⏹️ Stopping after the current object...";
    stopped:
        "⏹️ Parado. O repositório continua legível; execute a mesma ação de novo para retomar.",
        "⏹️ Stopped. The repository stays readable; run the same action again to resume.";
    // Export / import / change password
    export_section: "Exportar / Importar", "Export / Import";
    paper_form: "Exportar a forma imprimível em papel (--paper)", "Export the printable paper form (--paper)";
    import_force:
        "Importar: substituir um ficheiro de chave diferente já existente (--force; o antigo é guardado)",
        "Import: replace a different existing key file (--force; the old one is backed up)";
    export_button: "📤 Exportar ficheiro da chave", "📤 Export key file";
    import_button: "📥 Importar ficheiro da chave", "📥 Import key file";
    change_section: "Alterar palavra-passe", "Change password";
    change_hint:
        "Só o ficheiro da chave é reescrito; os dados ficam como estão.",
        "Only the key file is rewritten; the data stays as it is.";
    change_button: "🔁 Alterar palavra-passe", "🔁 Change password";
    // Verify --repair
    repair_option:
        "🔧 Reparar (--repair): apagar objetos comprovadamente corrompidos, para o próximo backup os enviar de novo (sempre verificação completa)",
        "🔧 Repair (--repair): delete objects proven corrupt, so the next backup uploads them again (always a full check)";
    repair_checking:
        "🔍 A procurar objetos corrompidos (sem alterar nada)...",
        "🔍 Looking for corrupt objects (nothing is changed)...";
    repair_none:
        "✅ Nenhum objeto corrompido: a reparação não tem nada a apagar.",
        "✅ No corrupt object: repair has nothing to delete.";
    repair_will_delete:
        "A reparação vai APAGAR {count} objeto(s) comprovadamente corrompido(s); o próximo backup envia-os de novo se os ficheiros de origem ainda existirem:",
        "Repair will DELETE {count} object(s) proven corrupt; the next backup uploads them again if the source files still exist:";
    repair_missing:
        "{count} objeto(s) referenciado(s) em falta: a reparação não os recupera (o próximo backup envia-os se os ficheiros de origem existirem).",
        "{count} referenced object(s) are missing: repair cannot bring them back (the next backup uploads them if the source files still exist).";
    repair_more: "… mais {count}", "… {count} more";
    repair_used_by: "usado por", "used by";
    repair_confirm_button: "🗑️ Apagar {count} objeto(s) corrompido(s)", "🗑️ Delete {count} corrupt object(s)";
    // Diagnose --repair
    diag_repair_button: "🔧 Reparar estrutura…", "🔧 Repair structure…";
    diag_repair_confirm:
        "A reparação vai criar as pastas em falta: {list}. Nada mais é alterado.",
        "Repair will create the missing directories: {list}. Nothing else is changed.";
    diag_repair_none:
        "A estrutura do repositório está completa: nada a reparar.",
        "The repository structure is complete: nothing to repair.";
    diag_repair_done: "✅ Estrutura reparada: {list}", "✅ Structure repaired: {list}";
    diag_path_not_found:
        "❌ Pasta do backup não encontrada: {path}",
        "❌ Backup path not found: {path}";
    diag_repair_local_only:
        "A reparação da estrutura só se aplica a repositórios locais.",
        "Structure repair only applies to local repositories.";
    // FIDO2 keys
    fido2_register_button: "➕ Registar uma chave nova", "➕ Register a new key";
    fido2_register_hint:
        "Abre o browser para registar uma chave de segurança (como `rsb auth register`). Para juntar uma chave a um utilizador existente, autentique-se primeiro nessa página com uma das chaves dele.",
        "Opens the browser to register a security key (like `rsb auth register`). To add a key to an existing user, first authenticate in that page with one of its keys.";
    fido2_register_waiting:
        "🌐 À espera do registo no browser...",
        "🌐 Waiting for the registration in the browser...";
    fido2_register_done: "✅ Chave registada.", "✅ Key registered.";
    fido2_register_cancelled: "Registo cancelado.", "Registration cancelled.";
    fido2_delete_button: "🗑️ Apagar as minhas chaves…", "🗑️ Delete my keys…";
    fido2_delete_confirm:
        "Isto apaga deste computador todas as chaves FIDO2 de '{user}' (como `rsb auth delete`). A sessão termina e terá de registar uma chave para voltar a entrar. Escreva o identificador para confirmar:",
        "This deletes every FIDO2 key of '{user}' from this computer (like `rsb auth delete`). You will be signed out and need to register a key again to sign in. Type the identifier to confirm:";
    fido2_delete_mismatch: "O identificador não coincide.", "The identifier does not match.";
    fido2_delete_unknown: "Utilizador desconhecido: {user}", "Unknown user: {user}";
    fido2_delete_done: "✅ Chaves de '{user}' apagadas.", "✅ Keys of '{user}' deleted.";
    fido2_delete_action: "🗑️ Apagar", "🗑️ Delete";
}

pub fn get_key_texts(lang: Language) -> &'static KeyTexts {
    match lang {
        Language::Pt => &PT_KEY_TEXTS,
        Language::En => &EN_KEY_TEXTS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<String> {
        let mut v: Vec<String> = s
            .split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(name, _)| name.to_string()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn key_texts_have_the_same_placeholders_in_every_language() {
        let pt = PT_KEY_TEXTS.all();
        let en = EN_KEY_TEXTS.all();
        assert_eq!(pt.len(), en.len());
        for ((field, p), (_, e)) in pt.iter().zip(en.iter()) {
            assert!(!p.is_empty() && !e.is_empty(), "{field}");
            assert_eq!(placeholders(p), placeholders(e), "{field}");
        }
    }

    #[test]
    fn fill_replaces_named_placeholders() {
        assert_eq!(
            fill("{a} and {b} and {a}", &[("a", "1"), ("b", "2")]),
            "1 and 2 and 1"
        );
        assert_eq!(fill("no placeholder", &[("a", "1")]), "no placeholder");
    }
}

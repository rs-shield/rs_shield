use maud::{html, DOCTYPE};
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize, Debug, Default)]
pub struct ReportData {
    pub operation: String,
    pub profile_path: String,
    pub timestamp: String,
    pub duration: Duration,
    pub mode: Option<String>,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub files_with_errors: usize,
    pub total_files: usize,
    pub errors: Vec<String>,
    pub status: String,
}

#[allow(dead_code)]
pub fn generate_html(data: &ReportData) -> String {
    let report_html = html! {
        (DOCTYPE)
        html lang="en-US" {
            head {
                meta charset="UTF-8";
                title { "RSB Report - " (data.operation) }
                style {
                    "body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; margin: 2em; background-color: #f9fafb; color: #1f2937; }"
                    ".container { max-width: 800px; margin: auto; background: white; padding: 20px 40px; border-radius: 8px; box-shadow: 0 4px 6px rgba(0,0,0,0.1); }"
                    "h1 { color: #4f46e5; border-bottom: 2px solid #e5e7eb; padding-bottom: 10px; }"
                    "table { width: 100%; border-collapse: collapse; margin-top: 20px; }"
                    "th, td { text-align: left; padding: 12px; border-bottom: 1px solid #e5e7eb; }"
                    "th { background-color: #f9fafb; font-weight: 600; }"
                    "tr:last-child td { border-bottom: none; }"
                    ".status-ok { color: #10b981; font-weight: bold; }"
                    ".status-fail { color: #ef4444; font-weight: bold; }"
                    ".errors { margin-top: 20px; background-color: #fef2f2; border: 1px solid #fecaca; padding: 15px; border-radius: 4px; }"
                    ".errors h2 { margin-top: 0; color: #991b1b; }"
                    "pre { white-space: pre-wrap; word-wrap: break-word; background: #fff; padding: 10px; border-radius: 4px; }"
                }
            }

         body {
            div class="container" {
                h1 { "RSB Operation Report" }
                p { "Generated on: " (data.timestamp) }

                table {
                    tr { th { "Operation" } td { (data.operation) } }
                    tr { th { "Profile" } td { code { (data.profile_path) } } }
                    @if let Some(mode) = &data.mode {
                        tr { th { "Mode" } td { (mode) } }
                    }
                    tr { th { "Duration" } td { (format!("{:.2?}", data.duration)) } }
                    // Alterado para "Success" no check e no texto
                    tr { th { "Status" } td class=(if data.status == "Success" { "status-ok" } else { "status-fail" }) { (data.status) } }
                }

                h2 { "Statistics" }
                table {
                    tr { th { "Files Processed" } td { (data.files_processed) } }
                    tr { th { "Files Skipped" } td { (data.files_skipped) } }
                    tr { th { "Files with Errors" } td { (data.files_with_errors) } }
                    tr { th { "Total Files Scanned" } td { (data.total_files) } }
                }

                @if !data.errors.is_empty() {
                    div class="errors" {
                        h2 { "Errors Found" }
                        pre { @for error in &data.errors { (error) "\n" } }
                    }
                }
            }
         }
        }

    };
    report_html.into_string()
}

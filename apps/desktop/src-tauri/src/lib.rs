//! Stora desktop shell.

mod advisor;
mod commands;
mod scheduler;
mod state;
mod tray;

use tauri::{Manager, WindowEvent};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("STORA_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| format!("could not resolve the app data folder: {err}"))?;

            let index = stora_index::Index::open(&data_dir.join("stora.db"))
                .map_err(|err| format!("could not open the Stora database: {err}"))?;

            // A scan row left mid-flight by a previous crash would otherwise
            // linger forever and keep its partial records from being pruned.
            match index.fail_interrupted_scans() {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, "closed out interrupted scans"),
                Err(err) => tracing::warn!(?err, "could not close out interrupted scans"),
            }

            // Mirror the curated knowledge file into the database so entries
            // can be queried alongside scan results. The file remains the
            // authoritative copy.
            let curated: Vec<stora_index::knowledge::KnowledgeRow> = stora_knowledge::entries()
                .into_iter()
                .map(|entry| stora_index::knowledge::KnowledgeRow {
                    id: entry.id,
                    pattern: entry.pattern,
                    title: entry.title,
                    written_by: entry.written_by,
                    if_removed: entry.if_removed,
                    removable: entry.removable,
                    source_title: entry.source_title,
                    source_url: entry.source_url,
                })
                .collect();

            match index.seed_knowledge(&curated) {
                Ok(count) => tracing::info!(count, "seeded curated location knowledge"),
                Err(err) => tracing::warn!(?err, "could not seed location knowledge"),
            }

            app.manage(AppState::new(index, data_dir));

            if let Err(err) = tray::build(app.handle()) {
                // A missing tray is not fatal; the window still works.
                tracing::warn!(?err, "could not create the tray icon");
            }

            refresh_tray_tooltip(app.handle());
            scheduler::start(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let close_to_tray = state
                    .settings()
                    .map(|settings| settings.close_to_tray)
                    .unwrap_or(false);

                // Closing to the tray keeps background monitoring alive; the
                // user exits explicitly from the tray menu.
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan::list_drives,
            commands::scan::start_scan,
            commands::scan::pause_scan,
            commands::scan::resume_scan,
            commands::scan::cancel_scan,
            commands::scan::get_scan_summary,
            commands::storage::get_folder_children,
            commands::storage::get_folder_details,
            commands::storage::get_large_files,
            commands::storage::get_storage_breakdown,
            commands::storage::get_relocation_candidates,
            commands::storage::build_relocation_plan,
            commands::storage::execute_relocation,
            commands::storage::reveal_in_explorer,
            commands::cleanup::get_cleanup_categories,
            commands::cleanup::build_cleanup_plan,
            commands::cleanup::execute_cleanup_plan,
            commands::cleanup::cancel_cleanup,
            commands::cleanup::get_plan_items,
            commands::cleanup::get_cleanup_history,
            commands::cleanup::get_cleanup_errors,
            commands::cleanup::get_locking_processes,
            commands::settings::get_settings,
            commands::settings::get_advisor_key_status,
            commands::settings::save_advisor_api_key,
            commands::settings::delete_advisor_api_key,
            commands::settings::update_settings,
            commands::settings::get_ui_state,
            commands::settings::save_ui_state,
            commands::settings::get_system_appearance,
            commands::settings::get_exclusions,
            commands::settings::create_exclusion,
            commands::settings::delete_exclusion,
            commands::settings::get_recovered_this_month,
            commands::settings::clear_local_data,
            commands::settings::get_data_folder,
            commands::developer::scan_developer_storage,
            commands::developer::cancel_developer_scan,
            commands::developer::get_virtual_disks,
            commands::developer::build_developer_cleanup_plan,
            commands::apps::get_installed_apps,
            commands::apps::get_app_footprint,
            commands::apps::preflight_uninstall,
            commands::apps::start_uninstall,
            commands::apps::scan_uninstall_leftovers,
            commands::apps::build_leftover_cleanup_plan,
            commands::apps::poll_application_activity,
            commands::apps::clear_application_activity,
            commands::advanced::find_duplicates,
            commands::advanced::cancel_duplicate_scan,
            commands::advanced::build_duplicate_cleanup_plan,
            commands::advanced::apply_keep_strategy,
            commands::advanced::record_growth_snapshot,
            commands::advanced::get_growth_history,
            commands::advanced::get_alerts,
            commands::advanced::get_automation_rules,
            commands::advanced::create_automation_rule,
            commands::advanced::set_rule_enabled,
            commands::advanced::delete_automation_rule,
            commands::advanced::get_rule_history,
            commands::advanced::evaluate_automation_rules,
            commands::advanced::get_quarantine_items,
            commands::advanced::restore_quarantine_item,
            commands::advanced::purge_quarantine_item,
            commands::advanced::get_quarantine_size,
            commands::knowledge::explain_location,
            commands::knowledge::advise_path,
            commands::knowledge::research_advisor_path,
            commands::knowledge::knowledge_entry_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Stora");
}

/// Shows free space on the system volume in the tray tooltip.
fn refresh_tray_tooltip(app: &tauri::AppHandle) {
    let summary = match stora_winapi::enumerate_drives() {
        Ok(drives) => drives
            .iter()
            .find(|drive| drive.root.eq_ignore_ascii_case("C:\\"))
            .map(|drive| {
                format!(
                    "Stora\nC: {} available",
                    stora_core::format_bytes(drive.free_bytes)
                )
            })
            .unwrap_or_else(|| "Stora".to_string()),
        Err(_) => "Stora".to_string(),
    };
    tray::update_tooltip(app, &summary);
}

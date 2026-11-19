#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod local_worlds;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(local_worlds::LocalGames::default())
        .invoke_handler(tauri::generate_handler![
            local_worlds::local_world_list,
            local_worlds::local_tutorial_catalog,
            local_worlds::local_world_start
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<local_worlds::LocalGames>().stop();
            }
        });
}

use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
};
use tauri::Manager;

#[derive(Default)]
pub struct LocalGames(Mutex<HashMap<String, LocalGame>>);
impl LocalGames {
    pub fn stop(&self) {
        if let Ok(mut games) = self.0.lock() {
            games.clear();
        }
    }
}
struct LocalGame {
    child: Child,
    connection: Value,
}
impl Drop for LocalGame {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn directory(app: &tauri::AppHandle, mode: &str) -> Result<PathBuf, String> {
    if !matches!(mode, "singleplayer" | "tutorial") {
        return Err("invalid local world mode".into());
    }
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("worlds")
        .join(mode);
    fs::create_dir_all(path.join("data/worlds")).map_err(|e| e.to_string())?;
    Ok(path)
}
#[tauri::command]
pub fn local_world_list(app: tauri::AppHandle, mode: String) -> Result<Value, String> {
    let path = directory(&app, &mode)?.join("data/worlds");
    read_saves(path, &mode)
}
fn read_saves(path: PathBuf, mode: &str) -> Result<Value, String> {
    let mut worlds = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
            return Err("invalid or oversized local world save".into());
        }
        let value: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("invalid save {}: {e}", path.display()))?;
        let id = value["id"]
            .as_str()
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .ok_or("invalid local world id")?;
        let tutorial = &value["world"]["tutorial"];
        worlds.push(json!({"id":id,"name":value["name"],"kind":value["kind"],"level_id":value["level_id"],"status":value["status"],"local_mode":mode,"tutorial_id":tutorial["definition"]["id"],"tutorial_completed":tutorial["completed"].as_bool().unwrap_or(false),"tutorial_definition":tutorial["definition"]}));
    }
    worlds.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!({"data":worlds}))
}

#[tauri::command]
pub fn local_tutorial_catalog(app: tauri::AppHandle, source: String, entries: Option<Value>) -> Result<Value, String> {
    cache_catalog(directory(&app, "tutorial")?.join("catalog.json"), source, entries)
}

fn cache_catalog(path: PathBuf, source: String, entries: Option<Value>) -> Result<Value, String> {
    if let Some(entries) = entries {
        if !entries.as_array().is_some_and(|items| items.len() <= 1024) { return Err("invalid tutorial catalog".into()); }
        let data = serde_json::to_vec(&json!({"source":source,"data":entries})).map_err(|e| e.to_string())?;
        if data.len() > 16 * 1024 * 1024 { return Err("tutorial catalog too large".into()); }
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary, data).map_err(|e| e.to_string())?;
        if let Err(error) = fs::rename(&temporary, &path) { let _ = fs::remove_file(temporary); return Err(error.to_string()); }
    }
    match fs::read(&path) {
        Ok(data) => {
            let cached: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
            Ok(json!({"data":if cached["source"] == source { cached["data"].clone() } else { json!([]) }}))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({"data":[]})),
        Err(error) => Err(error.to_string()),
    }
}
fn executable(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let name = if cfg!(windows) {
        "game-server.exe"
    } else {
        "game-server"
    };
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::current_exe() {
        if let Some(parent) = path.parent() {
            candidates.push(parent.join(name));
        }
    }
    if let Ok(path) = app.path().resource_dir() {
        candidates.push(path.join(name));
    }
    if cfg!(debug_assertions) {
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug")
                .join(name),
        );
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("找不到本地游戏引擎 game-server，请构建并将它放在客户端程序旁".into())
}
#[tauri::command]
pub fn local_world_start(
    app: tauri::AppHandle,
    games: tauri::State<'_, LocalGames>,
    mode: String,
) -> Result<Value, String> {
    let path = directory(&app, &mode)?;
    let mut games = games.0.lock().map_err(|e| e.to_string())?;
    if let Some(game) = games.get_mut(&mode) {
        if game.child.try_wait().map_err(|e| e.to_string())?.is_none() {
            return Ok(game.connection.clone());
        }
    }
    games.remove(&mode);
    let executable = executable(&app)?;
    let socket = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = socket.local_addr().map_err(|e| e.to_string())?.port();
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let ai_enabled = mode == "tutorial";
    if ai_enabled {
        let mut python = Command::new("python");
        python.arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; python.creation_flags(0x08000000); }
        if !python.status().is_ok_and(|status| status.success()) {
            return Err("教程脚本需要 Python 3，请安装 Python 并加入 PATH 后重试".into());
        }
    }
    let config = format!("server_id = \"local-{mode}\"\nhost = \"127.0.0.1\"\nport = {port}\ndata_dir = \"data\"\n[auth]\nmode = \"local\"\ntoken = {token:?}\n[ai]\nenabled = {ai_enabled}\n");
    let config_path = path.join("game-server.toml");
    fs::write(&config_path, config).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    drop(socket);
    let mut command = Command::new(executable);
    command
        .current_dir(&path)
        .env("LIFE_PAPER_GAME_CONFIG", &config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command.spawn().map_err(|e| e.to_string())?;
    let connection = json!({"api_url":format!("http://127.0.0.1:{port}/api/v1"),"token":token,"server_id":format!("local-{mode}")});
    games.insert(
        mode,
        LocalGame {
            child,
            connection: connection.clone(),
        },
    );
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_catalog_reads_only_save_metadata_and_keeps_modes_separate() {
        let root = std::env::temp_dir().join(format!("life-local-{}", uuid::Uuid::new_v4()));
        for mode in ["singleplayer", "tutorial"] {
            let path = root.join(mode);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("save.json"), serde_json::to_vec(&json!({"id":uuid::Uuid::new_v4().to_string(),"name":mode,"world":{"secret":"not-in-catalog"},"status":"paused"})).unwrap()).unwrap();
            fs::write(path.join("unfinished.tmp"), "invalid").unwrap();
            let result = read_saves(path, mode).unwrap();
            assert_eq!(result["data"].as_array().unwrap().len(), 1);
            assert_eq!(result["data"][0]["local_mode"], mode);
            assert_eq!(result["data"][0]["name"], mode);
            assert!(!result.to_string().contains("not-in-catalog"));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tutorial_cache_and_saved_completion_are_available_without_an_engine() {
        let root = std::env::temp_dir().join(format!("life-tutorial-cache-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("saves")).unwrap();
        let cache = root.join("catalog.json");
        let entries = json!([{"id":"downloaded","title":"Downloaded"}]);
        assert_eq!(cache_catalog(cache.clone(), "center-a".into(), Some(entries.clone())).unwrap()["data"], entries);
        assert_eq!(cache_catalog(cache.clone(), "center-a".into(), None).unwrap()["data"], entries);
        assert_eq!(cache_catalog(cache.clone(), "center-b".into(), None).unwrap()["data"], json!([]));
        assert_eq!(cache_catalog(cache.clone(), "center-a".into(), Some(json!([]))).unwrap()["data"], json!([]));
        assert!(cache_catalog(cache, "center-a".into(), Some(json!({}))).is_err());
        fs::write(root.join("saves/world.json"), serde_json::to_vec(&json!({"id":uuid::Uuid::new_v4(),"world":{"tutorial":{"definition":{"id":"downloaded","title":"Downloaded"},"completed":true}}})).unwrap()).unwrap();
        let saved = read_saves(root.join("saves"), "tutorial").unwrap();
        assert_eq!(saved["data"][0]["tutorial_id"], "downloaded");
        assert_eq!(saved["data"][0]["tutorial_completed"], true);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_saves_report_an_error_instead_of_becoming_empty_worlds() {
        let root = std::env::temp_dir().join(format!("life-local-bad-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("bad.json"), "{}").unwrap();
        assert!(read_saves(root.clone(), "singleplayer").is_err());
        fs::write(root.join("bad.json"), "{").unwrap();
        assert!(read_saves(root.clone(), "singleplayer").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

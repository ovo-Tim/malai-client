mod http_bridge;
use http_bridge::http_bridge;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

static GRACEFUL: LazyLock<kulfi_utils::Graceful> = LazyLock::new(kulfi_utils::Graceful::new);
static TASKLIST: LazyLock<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
async fn browse(
    app_handle: tauri::AppHandle,
    port: u16,
    url: String,
    open_browser: bool,
) -> String {
    if let Some(task) = TASKLIST
        .lock()
        .expect("Unable to unlock task list")
        .remove(&url)
    {
        match task.send(()) {
            Ok(_) => {
                println!("Stopped task for {url}");
                return "Stopped".to_string();
            }
            Err(_) => {
                println!("Error stopping task");
                return format!("Error stopping task");
            }
        }
    }

    let (id52, path) = match parse_url(&url) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = ?e, url, "Failed to parse URL");
            eprintln!("Failed to parse URL: {e}");
            return format!("Failed to parse URL: {}", e.to_string());
        }
    };

    let path = path.to_string();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(http_bridge(
        port,
        Some(id52.to_string()),
        GRACEFUL.clone(),
        shutdown_rx,
        move |port: u16| {
            if open_browser {
                let url = format!("http://127.0.0.1:{port}/{path}");
                app_handle
                    .opener()
                    .open_url(url, None::<&str>)
                    .map_err(Into::into)
            } else {
                Ok(())
            }
        },
    ));

    TASKLIST
        .lock()
        .expect("Unable to unlock task list")
        .insert(url, shutdown_tx);
    return "Ok".to_string();
}

#[tauri::command]
fn status(url: String) -> bool {
    TASKLIST
        .lock()
        .expect("Unable to unlock task list")
        .contains_key(&url)
}

/// This function extracts the id52 and the path from the URL
///
/// the path is the part after the first / in the URL
fn parse_url(url: &str) -> eyre::Result<(&str, &str)> {
    // check if url starts with kulfi://
    let rest = match url.split_once("kulfi://") {
        Some(("", rest)) => rest,
        Some((e, _rest)) => {
            return Err(eyre::anyhow!(
                "URL must start with kulfi://, got {e} in the beginning"
            ));
        }
        None => {
            return Err(eyre::anyhow!("URL must start with kulfi://"));
        }
    };

    Ok(rest.split_once('/').unwrap_or((rest, "")))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(mobile)]
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![browse, status]);

    #[cfg(desktop)]
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![browse, status]);

    #[cfg(mobile)]
    let builder = builder.setup(|app| {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title("Malai client")
            .body("I guess you need this to keep the app alive on android.")
            .ongoing()
            .show()
            .unwrap();
        Ok(())
    });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

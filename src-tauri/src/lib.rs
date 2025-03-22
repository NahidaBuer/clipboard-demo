// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use arboard::Clipboard;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

// 定义剪贴板条目结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    id: usize,
    content: String,
    timestamp: u64,
}

// 存储剪贴板历史记录的状态
pub struct ClipboardState {
    history: Vec<ClipboardItem>,
    last_content: Option<String>,
    next_id: usize,
}

impl ClipboardState {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            last_content: None,
            next_id: 0,
        }
    }
}

// 获取剪贴板历史记录
#[tauri::command]
fn get_clipboard_history(state: State<Arc<Mutex<ClipboardState>>>) -> Vec<ClipboardItem> {
    let state = state.lock().unwrap();
    state.history.clone()
}

// 获取当前剪贴板内容
#[tauri::command]
fn get_clipboard_content() -> Result<String, String> {
    match Clipboard::new() {
        Ok(mut clipboard) => match clipboard.get_text() {
            Ok(text) => Ok(text),
            Err(e) => Err(format!("获取剪贴板内容失败: {}", e)),
        },
        Err(e) => Err(format!("初始化剪贴板失败: {}", e)),
    }
}

// 清空剪贴板历史
#[tauri::command]
fn clear_clipboard_history(state: State<Arc<Mutex<ClipboardState>>>) {
    let mut state = state.lock().unwrap();
    state.history.clear();
    state.next_id = 0;
}

// 定期检查剪贴板变化
fn setup_clipboard_watcher(app: tauri::AppHandle) {
    // 启动后台线程监听剪贴板变化
    std::thread::spawn(move || {
        let state = app.state::<Arc<Mutex<ClipboardState>>>();
        let mut clipboard = Clipboard::new().expect("无法初始化剪贴板");

        loop {
            // 每半秒检查一次剪贴板
            std::thread::sleep(std::time::Duration::from_millis(500));

            // 尝试获取剪贴板文本
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    let mut state = state.lock().unwrap();

                    // 检查是否与上次内容相同
                    if state.last_content.as_ref() != Some(&text) {
                        // 添加新条目
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let new_item = ClipboardItem {
                            id: state.next_id,
                            content: text.clone(),
                            timestamp: now,
                        };

                        state.history.push(new_item.clone());
                        state.last_content = Some(text);
                        state.next_id += 1;

                        // 通知前端剪贴板更新了
                        let _ = app.emit("clipboard-changed", new_item);
                    }
                }
            }
        }
    });
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 初始化剪贴板状态
            let clipboard_state = Arc::new(Mutex::new(ClipboardState::new()));
            app.manage(clipboard_state);

            // 启动剪贴板监听
            setup_clipboard_watcher(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_clipboard_content,
            get_clipboard_history,
            clear_clipboard_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

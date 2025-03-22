// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use anyhow::Context;
use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher,
    ClipboardWatcherContext, ContentFormat,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

// 定义错误类型
#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("剪贴板初始化失败: {0}")]
    InitError(String),

    #[error("剪贴板读取失败: {0}")]
    ReadError(String),

    #[error("剪贴板写入失败: {0}")]
    WriteError(String),

    #[error("状态访问失败: {0}")]
    StateError(String),

    #[error("事件发送失败: {0}")]
    EventError(String),
}

// 为ClipboardError实现Serialize trait，使其可以在Tauri命令中作为错误返回
impl Serialize for ClipboardError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// 定义剪贴板条目结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardItem {
    id: usize,
    content: String,
    content_type: ClipboardContentType,
    html_content: Option<String>,
    rtf_content: Option<String>,
    image_path: Option<String>,
    timestamp: u64,
}

// 剪贴板内容类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardContentType {
    Text,
    RichText,
    Html,
    Image,
    File,
    Unknown,
}

// 存储剪贴板历史记录的状态
pub struct ClipboardState {
    history: Vec<ClipboardItem>,
    last_content_hash: Option<u64>,
    next_id: usize,
    // 配置选项
    max_history_size: usize,
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            last_content_hash: None,
            next_id: 0,
            max_history_size: 100, // 默认最多保存100条记录
        }
    }
}

impl ClipboardState {
    // 添加新的剪贴板条目
    fn add_item(&mut self, item: ClipboardItem) -> ClipboardItem {
        // 添加到历史记录
        self.history.push(item.clone());

        // 如果超过最大历史记录数，移除最旧的记录
        if self.history.len() > self.max_history_size {
            self.history.remove(0);
        }

        // 更新ID和哈希值
        self.last_content_hash = Some(self.calculate_hash(&item));
        self.next_id += 1;

        item
    }

    // 计算内容哈希值，用于检测变化
    fn calculate_hash(&self, item: &ClipboardItem) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        item.content.hash(&mut hasher);
        if let Some(html) = &item.html_content {
            html.hash(&mut hasher);
        }
        if let Some(rtf) = &item.rtf_content {
            rtf.hash(&mut hasher);
        }
        if let Some(img_path) = &item.image_path {
            img_path.hash(&mut hasher);
        }
        hasher.finish()
    }

    // 创建剪贴板条目
    fn create_item(
        &mut self,
        content: String,
        content_type: ClipboardContentType,
        html_content: Option<String>,
        rtf_content: Option<String>,
        image_path: Option<String>,
    ) -> ClipboardItem {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        ClipboardItem {
            id: self.next_id,
            content,
            content_type,
            html_content,
            rtf_content,
            image_path,
            timestamp: now,
        }
    }
}

// 剪贴板变化处理器
struct ClipboardStateManager {
    ctx: ClipboardContext,
    state: Arc<Mutex<ClipboardState>>,
    app: tauri::AppHandle,
}

impl ClipboardStateManager {
    fn new(
        state: Arc<Mutex<ClipboardState>>,
        app: tauri::AppHandle,
    ) -> Result<Self, ClipboardError> {
        let ctx = ClipboardContext::new().map_err(|e| ClipboardError::InitError(e.to_string()))?;

        Ok(Self { ctx, state, app })
    }

    // 处理剪贴板更新
    async fn handle_clipboard_update(&mut self) -> Result<bool, ClipboardError> {
        // 创建图片保存目录（如果不存在）
        let app_data_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| ClipboardError::InitError(format!("无法获取应用数据目录: {}", e)))?;
        let images_dir = app_data_dir.join("clipboard_images");
        if !images_dir.exists() {
            fs::create_dir_all(&images_dir)
                .map_err(|e| ClipboardError::InitError(format!("无法创建图片目录: {}", e)))?;
        }

        // 变量准备
        let mut text = String::new();
        let mut html_content = None;
        let mut rtf_content = None;
        let mut image_path = None;
        let mut content_type = ClipboardContentType::Unknown;

        // 确定内容类型
        if self.ctx.has(ContentFormat::Image) {
            content_type = ClipboardContentType::Image;
            // 处理图片
            match self.ctx.get_image() {
                Ok(image) => {
                    // 生成唯一文件名
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let file_name = format!("clipboard_image_{}.png", timestamp);
                    let image_path_full = images_dir.join(&file_name);

                    // 保存图片
                    match image.save_to_path(image_path_full.to_str().unwrap()) {
                        Ok(_) => {
                            image_path = Some(file_name);
                            text = "[图片内容]".to_string();
                            debug!("图片已保存到: {:?}", image_path);
                        }
                        Err(e) => {
                            error!("保存图片失败: {}", e);
                            text = "[图片内容-保存失败]".to_string();
                        }
                    }
                }
                Err(e) => {
                    error!("读取图片失败: {}", e);
                    text = "[图片内容-读取失败]".to_string();
                }
            }
        } else if self.ctx.has(ContentFormat::Rtf) {
            content_type = ClipboardContentType::RichText;
            // 获取富文本
            rtf_content = self.ctx.get_rich_text().ok();
            // 尝试获取文本内容作为显示文本
            text = match self.ctx.get_text() {
                Ok(t) => t,
                Err(_) => "[富文本内容]".to_string(),
            };
        } else if self.ctx.has(ContentFormat::Html) {
            content_type = ClipboardContentType::Html;
            // 获取HTML
            html_content = self.ctx.get_html().ok();
            // 尝试获取文本内容作为显示文本
            text = match self.ctx.get_text() {
                Ok(t) => t,
                Err(_) => "[HTML内容]".to_string(),
            };
        } else {
            // 获取纯文本
            match self.ctx.get_text() {
                Ok(t) => {
                    text = t;
                    content_type = ClipboardContentType::Text;
                }
                Err(e) => {
                    error!("剪贴板读取失败: {}", e);
                    text = "[读取失败]".to_string();
                }
            }
        }

        // 创建新条目
        let mut state = self.state.lock().await;
        let new_item = state.create_item(text, content_type, html_content, rtf_content, image_path);

        // 计算内容哈希值检查是否变化
        let hash = state.calculate_hash(&new_item);
        if state.last_content_hash == Some(hash) {
            return Ok(false); // 内容未变化
        }

        // 添加到历史并更新哈希
        let new_item = state.add_item(new_item);
        drop(state); // 释放锁，避免在发送事件时持有锁

        // 发送事件通知前端
        self.app
            .emit("clipboard-changed", new_item)
            .map_err(|e| ClipboardError::EventError(e.to_string()))?;

        Ok(true)
    }
}

// 实现ClipboardHandler trait用于监听剪贴板变化
impl ClipboardHandler for ClipboardStateManager {
    fn on_clipboard_change(&mut self) {
        // 创建一个新的Tokio运行时处理剪贴板变化
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        match rt.block_on(self.handle_clipboard_update()) {
            Ok(true) => debug!("剪贴板内容已更新"),
            Ok(false) => debug!("剪贴板变化但内容重复，已忽略"),
            Err(e) => error!("处理剪贴板更新失败: {}", e),
        }
    }
}

// 获取剪贴板历史记录
#[tauri::command]
fn get_clipboard_history(
    state: State<Arc<Mutex<ClipboardState>>>,
) -> Result<Vec<ClipboardItem>, String> {
    // 创建一个新的Tokio运行时
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return Err(format!("创建运行时失败: {}", e)),
    };

    // 在运行时中执行异步操作
    match rt.block_on(async {
        let state = state.lock().await;
        Ok::<_, ClipboardError>(state.history.clone())
    }) {
        Ok(history) => Ok(history),
        Err(e) => Err(e.to_string()),
    }
}

// 获取当前剪贴板内容
#[tauri::command]
fn get_clipboard_content() -> Result<ClipboardItem, String> {
    let ctx = match ClipboardContext::new() {
        Ok(ctx) => ctx,
        Err(e) => return Err(format!("初始化剪贴板失败: {}", e)),
    };

    // 变量准备
    let mut text = String::new();
    let mut html_content = None;
    let mut rtf_content = None;
    let mut image_path = None;
    let mut content_type = ClipboardContentType::Unknown;

    // 确定内容类型并获取相应内容
    if ctx.has(ContentFormat::Image) {
        content_type = ClipboardContentType::Image;
        text = "[图片内容]".to_string();
        // 注意: 这里我们不保存图片，因为这只是读取当前内容
    } else if ctx.has(ContentFormat::Rtf) {
        content_type = ClipboardContentType::RichText;
        rtf_content = ctx.get_rich_text().ok();
        text = ctx
            .get_text()
            .unwrap_or_else(|_| "[富文本内容]".to_string());
    } else if ctx.has(ContentFormat::Html) {
        content_type = ClipboardContentType::Html;
        html_content = ctx.get_html().ok();
        text = ctx.get_text().unwrap_or_else(|_| "[HTML内容]".to_string());
    } else {
        // 获取纯文本
        match ctx.get_text() {
            Ok(t) => {
                text = t;
                content_type = ClipboardContentType::Text;
            }
            Err(e) => return Err(format!("获取剪贴板文本失败: {}", e)),
        }
    }

    // 创建条目
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(ClipboardItem {
        id: 0, // 临时ID
        content: text,
        content_type,
        html_content,
        rtf_content,
        image_path,
        timestamp: now,
    })
}

// 获取图片数据（用于前端显示）
#[tauri::command]
fn get_clipboard_image(app: tauri::AppHandle, image_name: String) -> Result<Vec<u8>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;
    let image_path = app_data_dir.join("clipboard_images").join(image_name);

    if !image_path.exists() {
        return Err(format!("图片不存在: {:?}", image_path));
    }

    fs::read(image_path).map_err(|e| format!("读取图片文件失败: {}", e))
}

// 设置剪贴板内容
#[tauri::command]
fn set_clipboard_content(
    content: String,
    html_content: Option<String>,
    rtf_content: Option<String>,
) -> Result<(), String> {
    let ctx = match ClipboardContext::new() {
        Ok(ctx) => ctx,
        Err(e) => return Err(format!("初始化剪贴板失败: {}", e)),
    };

    // 设置普通文本
    if let Err(e) = ctx.set_text(content) {
        return Err(format!("设置剪贴板文本失败: {}", e));
    }

    // 如果有HTML内容，也设置
    if let Some(html) = html_content {
        if let Err(e) = ctx.set_html(html) {
            debug!("设置HTML内容失败: {}", e);
            // 继续执行，不返回错误
        }
    }

    // 如果有RTF内容，也设置
    if let Some(rtf) = rtf_content {
        if let Err(e) = ctx.set_rich_text(rtf) {
            debug!("设置富文本内容失败: {}", e);
            // 继续执行，不返回错误
        }
    }

    Ok(())
}

// 清空剪贴板历史
#[tauri::command]
fn clear_clipboard_history(state: State<Arc<Mutex<ClipboardState>>>) -> Result<(), String> {
    // 创建一个新的Tokio运行时
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return Err(format!("创建运行时失败: {}", e)),
    };

    // 在运行时中执行异步操作
    match rt.block_on(async {
        let mut state = state.lock().await;
        state.history.clear();
        state.next_id = 0;
        state.last_content_hash = None;
        Ok::<_, ClipboardError>(())
    }) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// 设置历史记录大小限制
#[tauri::command]
fn set_max_history_size(
    size: usize,
    state: State<Arc<Mutex<ClipboardState>>>,
) -> Result<(), String> {
    // 创建一个新的Tokio运行时
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return Err(format!("创建运行时失败: {}", e)),
    };

    // 在运行时中执行异步操作
    match rt.block_on(async {
        let mut state = state.lock().await;
        state.max_history_size = size;

        // 如果当前历史记录大小超过新的限制，则截断
        let current_len = state.history.len();
        if current_len > size {
            // 安全地截断历史记录
            let new_start = current_len - size;
            let new_history = state.history.split_off(new_start);
            state.history = new_history;
        }

        Ok::<_, ClipboardError>(())
    }) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// 主入口点
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志w
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::DEBUG)
            .finish(),
    )
    .context("设置日志订阅者失败")
    .expect("初始化日志系统失败");

    // 构建应用
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 初始化状态
            let clipboard_state = Arc::new(Mutex::new(ClipboardState::default()));
            app.manage(clipboard_state.clone());

            // 创建剪贴板监听器
            let app_handle = app.app_handle();
            let clipboard_manager =
                match ClipboardStateManager::new(clipboard_state, app_handle.clone()) {
                    Ok(manager) => manager,
                    Err(e) => {
                        error!("创建剪贴板管理器失败: {}", e);
                        return Ok(());
                    }
                };

            // 创建剪贴板监听器
            let mut watcher = match ClipboardWatcherContext::new() {
                Ok(watcher) => watcher,
                Err(e) => {
                    error!("创建剪贴板监听器失败: {}", e);
                    return Ok(());
                }
            };

            // 添加处理器
            let _shutdown = watcher.add_handler(clipboard_manager);

            // 启动监听（在新线程中）
            std::thread::spawn(move || {
                info!("启动剪贴板监听...");
                watcher.start_watch();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_clipboard_content,
            set_clipboard_content,
            get_clipboard_history,
            clear_clipboard_history,
            set_max_history_size,
            get_clipboard_image,
        ])
        .run(tauri::generate_context!())
        .context("运行Tauri应用失败")
        .expect("应用运行失败");
}

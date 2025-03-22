import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

// 定义剪贴板条目类型
interface ClipboardItem {
  id: number;
  content: string;
  contentType: string;
  htmlContent?: string;
  rtfContent?: string;
  imageBase64?: string;
  imagePath?: string;
  timestamp: number;
}

function App() {
  const [clipboardItems, setClipboardItems] = useState<ClipboardItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedItem, setSelectedItem] = useState<ClipboardItem | null>(null);

  // 格式化时间戳
  const formatTimestamp = (timestamp: number): string => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleString();
  };

  // 格式化内容类型
  const formatContentType = (type: string): string => {
    switch (type) {
      case "text":
        return "纯文本";
      case "richText":
        return "富文本";
      case "html":
        return "HTML";
      case "image":
        return "图片";
      case "file":
        return "文件";
      default:
        return "未知格式";
    }
  };

  // 获取剪贴板历史记录
  const fetchClipboardHistory = async () => {
    try {
      const history = await invoke<ClipboardItem[]>("get_clipboard_history");
      setClipboardItems(history);
      setLoading(false);
    } catch (error) {
      console.error("获取剪贴板历史记录失败:", error);
      setLoading(false);
    }
  };

  // 清空剪贴板历史记录
  const clearClipboardHistory = async () => {
    try {
      await invoke("clear_clipboard_history");
      setClipboardItems([]);
      setSelectedItem(null);
    } catch (error) {
      console.error("清空剪贴板历史记录失败:", error);
    }
  };

  // 复制内容到剪贴板
  const copyToClipboard = async (item: ClipboardItem) => {
    try {
      await invoke("set_clipboard_content", {
        content: item.content,
        htmlContent: item.htmlContent,
        rtfContent: item.rtfContent,
      });
      alert("已复制到剪贴板");
    } catch (error) {
      console.error("复制到剪贴板失败:", error);
      alert("复制失败: " + error);
    }
  };

  // 渲染内容预览
  const renderContentPreview = (item: ClipboardItem) => {
    switch (item.contentType) {
      case "image":
        if (item.imageBase64) {
          return (
            <div className="image-preview">
              <img
                src={`data:image/png;base64,${item.imageBase64}`}
                alt="剪贴板图片"
              />
            </div>
          );
        } else if (item.imagePath) {
          // 如果没有base64但有图片路径，可以提供一个获取图片的选项
          return (
            <div className="image-preview">
              <p>图片已保存，但无法直接预览</p>
              <button onClick={() => getImageBase64(item)}>加载图片</button>
            </div>
          );
        }
        return <p>图片内容不可用</p>;

      case "html":
        if (item.htmlContent) {
          return (
            <div className="html-preview">
              <iframe
                srcDoc={item.htmlContent}
                title="HTML内容预览"
                width="100%"
                height="300px"
                sandbox="allow-same-origin"
              />
            </div>
          );
        }
        return <p>{item.content}</p>;

      case "richText":
        if (item.rtfContent) {
          return (
            <div className="rtf-preview">
              <p>富文本内容 (RTF)</p>
              <pre>{item.rtfContent.substring(0, 100)}...</pre>
            </div>
          );
        }
        return <p>{item.content}</p>;

      default:
        return <p>{item.content}</p>;
    }
  };

  // 从服务器获取图片的base64编码
  const getImageBase64 = async (item: ClipboardItem) => {
    if (!item.imagePath) return;

    try {
      const base64Data = await invoke<string>("get_clipboard_image_base64", {
        imageName: item.imagePath,
      });

      // 更新当前项的base64数据
      const updatedItem = { ...item, imageBase64: base64Data };
      setSelectedItem(updatedItem);

      // 同时更新列表中的项
      setClipboardItems((prevItems) =>
        prevItems.map((i) => (i.id === item.id ? updatedItem : i))
      );
    } catch (error) {
      console.error("获取图片失败:", error);
    }
  };

  // 处理点击条目
  const handleItemClick = (item: ClipboardItem) => {
    setSelectedItem(item);
  };

  // 组件挂载时设置监听器
  useEffect(() => {
    // 初始加载历史记录
    fetchClipboardHistory();

    // 监听剪贴板变化事件
    const unlisten = listen<ClipboardItem>("clipboard-changed", (event) => {
      setClipboardItems((prevItems) => [event.payload, ...prevItems]);
    });

    // 组件卸载时移除监听器
    return () => {
      unlisten.then((unlistenFn) => unlistenFn());
    };
  }, []);

  return (
    <main className="container">
      <h1>增强型剪贴板管理工具</h1>

      <div className="clipboard-controls">
        <button onClick={clearClipboardHistory}>清空历史记录</button>
      </div>

      <div className="app-layout">
        <div className="clipboard-history">
          <h2>剪贴板历史记录</h2>

          {loading ? (
            <p>加载中...</p>
          ) : clipboardItems.length === 0 ? (
            <p>暂无剪贴板历史记录</p>
          ) : (
            <ul className="clipboard-items">
              {clipboardItems.map((item) => (
                <li
                  key={item.id}
                  className={`clipboard-item ${
                    selectedItem?.id === item.id ? "selected" : ""
                  }`}
                  onClick={() => handleItemClick(item)}
                >
                  <div className="clipboard-content-type">
                    {formatContentType(item.contentType)}
                  </div>
                  <div className="clipboard-summary">
                    {item.contentType === "image"
                      ? "[图片内容]"
                      : item.content.substring(0, 50) +
                        (item.content.length > 50 ? "..." : "")}
                  </div>
                  <div className="clipboard-timestamp">
                    {formatTimestamp(item.timestamp)}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>

        {selectedItem && (
          <div className="content-preview">
            <h2>内容预览</h2>
            <div className="preview-actions">
              <button onClick={() => copyToClipboard(selectedItem)}>
                复制到剪贴板
              </button>
            </div>
            <div className="preview-content">
              {renderContentPreview(selectedItem)}
            </div>
            <div className="content-details">
              <p>创建时间: {formatTimestamp(selectedItem.timestamp)}</p>
              <p>类型: {formatContentType(selectedItem.contentType)}</p>
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

export default App;

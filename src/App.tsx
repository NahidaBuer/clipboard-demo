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
  timestamp: number;
}

function App() {
  const [clipboardItems, setClipboardItems] = useState<ClipboardItem[]>([]);
  const [loading, setLoading] = useState(true);

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
                className="clipboard-item"
                onClick={() => copyToClipboard(item)}
              >
                <div className="clipboard-content-type">
                  {formatContentType(item.contentType)}
                </div>
                <div className="clipboard-content">{item.content}</div>
                <div className="clipboard-timestamp">
                  {formatTimestamp(item.timestamp)}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </main>
  );
}

export default App;

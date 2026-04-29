import { useState, useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { cropScreenRegion, recognizeText } from "@/lib/ocr";
import { useOcrSettings } from "@/stores/ocr-settings";
import { logError } from "@/lib/logger";

export function OcrScreenshot() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [drawing, setDrawing] = useState(false);
  const [startPos, setStartPos] = useState({ x: 0, y: 0 });
  const [imagePath, setImagePath] = useState("");
  const [loadKey, setLoadKey] = useState(0);

  const params = new URLSearchParams(window.location.search);
  const pathParam = params.get("path") || "";

  // 首次加载 + 监听复用事件
  useEffect(() => {
    useOcrSettings.getState().loadSettings();
    if (pathParam) setImagePath(pathParam);

    // 窗口复用时，Rust 会发此事件传入新截图路径
    const unlisten = listen<string>("ocr-screenshot-update", (event) => {
      setDrawing(false);
      // 立即清除 canvas，避免复用窗口时闪烁上一次的选区
      const canvas = canvasRef.current;
      const ctx = canvas?.getContext("2d");
      if (canvas && ctx) {
        ctx.clearRect(0, 0, canvas.width, canvas.height);
      }
      setImagePath(event.payload);
      setLoadKey((k) => k + 1); // 强制触发重新加载（路径可能相同）
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // 加载截图到 canvas
  const loadImage = useCallback((path: string) => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;

    const img = new Image();
    img.onload = async () => {
      imgRef.current = img;
      canvas.width = img.width;
      canvas.height = img.height;
      ctx.drawImage(img, 0, 0);
      // 截图绘制完成后通知 Rust 设置窗口位置并显示
      await invoke("ocr_screenshot_ready");
    };
    // 加 timestamp 避免浏览器缓存（复用窗口时文件路径相同但内容已变）
    img.src = convertFileSrc(path) + "?t=" + Date.now();
  }, []);

  useEffect(() => {
    if (imagePath) loadImage(imagePath);
  }, [imagePath, loadKey, loadImage]);

  // 重绘 canvas（背景 + 选区）
  const redraw = useCallback(
    (sx: number, sy: number, ex: number, ey: number) => {
      const canvas = canvasRef.current;
      const ctx = canvas?.getContext("2d");
      const img = imgRef.current;
      if (!canvas || !ctx || !img) return;

      // 画背景
      ctx.drawImage(img, 0, 0);
      // 半透明遮罩
      ctx.fillStyle = "rgba(0, 0, 0, 0.4)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      // 选区
      const rx = Math.min(sx, ex);
      const ry = Math.min(sy, ey);
      const rw = Math.abs(ex - sx);
      const rh = Math.abs(ey - sy);
      if (rw > 0 && rh > 0) {
        // 清除选区遮罩
        ctx.clearRect(rx, ry, rw, rh);
        ctx.drawImage(img, rx, ry, rw, rh, rx, ry, rw, rh);
        // 选区边框
        ctx.strokeStyle = "#3b82f6";
        ctx.lineWidth = 2;
        ctx.strokeRect(rx, ry, rw, rh);
        // 尺寸标签
        ctx.fillStyle = "#3b82f6";
        ctx.font = "12px sans-serif";
        ctx.fillText(`${rw} × ${rh}`, rx + 4, ry > 18 ? ry - 6 : ry + rh + 14);
      }
    },
    [],
  );

  // 将 CSS 鼠标坐标转换为 canvas 像素坐标（适配 DPI 缩放）
  const toCanvasCoords = (clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: clientX, y: clientY };
    const sx = canvas.width / canvas.clientWidth;
    const sy = canvas.height / canvas.clientHeight;
    return { x: Math.round(clientX * sx), y: Math.round(clientY * sy) };
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) {
      // 右键 / 中键直接退出
      closeWindow();
      return;
    }
    setDrawing(true);
    setStartPos(toCanvasCoords(e.clientX, e.clientY));
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!drawing) return;
    const { x, y } = toCanvasCoords(e.clientX, e.clientY);
    redraw(startPos.x, startPos.y, x, y);
  };

  const handleMouseUp = async (e: React.MouseEvent) => {
    if (!drawing) return;
    setDrawing(false);

    const { x: ex, y: ey } = toCanvasCoords(e.clientX, e.clientY);
    const rx = Math.min(startPos.x, ex);
    const ry = Math.min(startPos.y, ey);
    const rw = Math.abs(ex - startPos.x);
    const rh = Math.abs(ey - startPos.y);

    if (rw < 10 || rh < 10) {
      // 太小，关闭
      closeWindow();
      return;
    }

    // 立即退出截图状态，打开结果窗口显示"识别中..."
    closeWindow();

    // 先打开结果窗口（空文本 = 加载态）
    invoke("open_ocr_result_window", { text: "" }).catch(() => {});

    // 后台异步裁剪 + 识别
    (async () => {
      try {
        const base64 = await cropScreenRegion(imagePath, rx, ry, rw, rh);
        const text = await recognizeText(base64);
        await invoke("open_ocr_result_window", { text });
      } catch (error) {
        logError("OCR 识别失败:", error);
        await invoke("open_ocr_result_window", { text: `识别失败: ${error}` });
      }
    })();
  };

  const closeWindow = () => {
    // 隐藏而非关闭，下次热键可复用窗口
    getCurrentWindow().hide();
  };

  // ESC 关闭
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeWindow();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        cursor: "crosshair",
        userSelect: "none",
        overflow: "hidden",
        background: "transparent",
      }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onContextMenu={(e) => e.preventDefault()}
    >
      <canvas
        ref={canvasRef}
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          width: "100%",
          height: "100%",
        }}
      />
      {!drawing && (
        <div
          style={{
            position: "absolute",
            top: 12,
            left: "50%",
            transform: "translateX(-50%)",
            background: "rgba(0,0,0,0.7)",
            color: "#fff",
            padding: "6px 16px",
            borderRadius: 6,
            fontSize: 13,
            pointerEvents: "none",
          }}
        >
          拖拽选择识别区域 · ESC 取消
        </div>
      )}
    </div>
  );
}

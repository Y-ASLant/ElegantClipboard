import { invoke } from "@tauri-apps/api/core";
import { logError } from "@/lib/logger";
import { useOcrSettings } from "@/stores/ocr-settings";

/** 调用 OCR 识别（根据当前选择的提供者路由） */
export async function recognizeText(imageBase64: string): Promise<string> {
  const ocrSettings = useOcrSettings.getState();
  if (!ocrSettings.enabled) throw new Error("OCR 功能未启用");

  try {
    if (ocrSettings.provider === "custom") {
      if (!ocrSettings.customApiUrl) throw new Error("请先配置自定义 OCR API 地址");
      return await invoke<string>("ocr_recognize_custom", {
        imageBase64,
        apiUrl: ocrSettings.customApiUrl,
        proxyMode: ocrSettings.proxyMode || "none",
        proxyUrl: ocrSettings.proxyUrl || "",
      });
    }

    // 默认: 百度 OCR
    return await invoke<string>("ocr_recognize_baidu", {
      imageBase64,
      apiKey: ocrSettings.baiduApiKey,
      secretKey: ocrSettings.baiduSecretKey,
      accuracy: ocrSettings.accuracy || "high",
      proxyMode: ocrSettings.proxyMode || "none",
      proxyUrl: ocrSettings.proxyUrl || "",
    });
  } catch (error) {
    logError("OCR识别失败:", error);
    throw error;
  }
}

/** 触发 OCR 截图流程 */
export async function triggerOcrCapture(): Promise<void> {
  try {
    const screenshotPath = await invoke<string>("ocr_capture_screen");
    await invoke("open_ocr_screenshot_window", { screenshotPath });
  } catch (error) {
    logError("OCR 截图失败:", error);
    throw error;
  }
}

/** 裁剪截图区域并返回 base64 */
export async function cropScreenRegion(
  imagePath: string,
  x: number,
  y: number,
  width: number,
  height: number,
): Promise<string> {
  return invoke<string>("ocr_crop_region", {
    imagePath,
    x: Math.round(x),
    y: Math.round(y),
    width: Math.round(width),
    height: Math.round(height),
  });
}

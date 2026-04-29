import { invoke } from "@tauri-apps/api/core";
import { useOcrSettings } from "@/stores/ocr-settings";
import { logError } from "@/lib/logger";

/** 调用百度 OCR 识别 */
export async function recognizeText(imageBase64: string): Promise<string> {
  const ocrSettings = useOcrSettings.getState();
  if (!ocrSettings.enabled) throw new Error("OCR 功能未启用");

  try {
    const result = await invoke<string>("ocr_recognize_baidu", {
      imageBase64,
      apiKey: ocrSettings.baiduApiKey,
      secretKey: ocrSettings.baiduSecretKey,
      proxyMode: ocrSettings.proxyMode || "none",
      proxyUrl: ocrSettings.proxyUrl || "",
    });
    return result;
  } catch (error) {
    logError("OCR 识别失败:", error);
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

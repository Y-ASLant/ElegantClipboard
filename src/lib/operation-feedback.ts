import { invoke } from "@tauri-apps/api/core";
import { showToast } from "@/components/ui/toast";
import { t } from "@/i18n";
import { logError } from "@/lib/logger";

const operationCommands = {
  copy: "copy_to_clipboard",
  paste: "paste_content",
  pastePlain: "paste_content_as_plain",
  pastePath: "paste_as_path",
  pasteText: "paste_text_direct",
  mergePaste: "merge_paste_content",
  save: "save_file_as",
  showInExplorer: "show_in_explorer",
  details: "get_item_file_status",
  reorder: "bump_item_to_top",
  copyText: "write_text_to_clipboard",
  copyTranslation: "write_text_to_clipboard",
  pin: "toggle_pin",
  favorite: "toggle_favorite",
  move: "move_clipboard_item",
  moveFavorite: "move_favorite_clipboard_item",
  delete: "delete_clipboard_item",
  batchDelete: "batch_delete_clipboard_items",
  clear: "clear_history",
  editText: "update_text_content",
} as const satisfies Record<string, string>;

export type ClipboardOperation = keyof typeof operationCommands;

export type OperationResult<T = void> =
  | { status: "success"; value: T }
  | { status: "cancelled" }
  | { status: "failed" };

const reasonKeys = {
  item_not_found: "operationFeedback.reasons.item_not_found",
  resource_missing: "operationFeedback.reasons.resource_missing",
  resource_unreadable: "operationFeedback.reasons.resource_unreadable",
  permission_denied: "operationFeedback.reasons.permission_denied",
  invalid_content: "operationFeedback.reasons.invalid_content",
  unsupported_content: "operationFeedback.reasons.unsupported_content",
  image_decode_failed: "operationFeedback.reasons.image_decode_failed",
  clipboard_unavailable: "operationFeedback.reasons.clipboard_unavailable",
  clipboard_write_failed: "operationFeedback.reasons.clipboard_write_failed",
  paste_failed: "operationFeedback.reasons.paste_failed",
  save_failed: "operationFeedback.reasons.save_failed",
  invalid_destination: "operationFeedback.reasons.invalid_destination",
  explorer_failed: "operationFeedback.reasons.explorer_failed",
  internal: "operationFeedback.reasons.internal",
} satisfies Record<string, string>;

export type OperationErrorCode = keyof typeof reasonKeys;
export interface OperationError {
  code: OperationErrorCode;
  detail: string;
}

export function isOperationError(error: unknown): error is OperationError {
  if (typeof error !== "object" || error === null) return false;
  return "code" in error && "detail" in error
    && typeof error.code === "string"
    && Object.prototype.hasOwnProperty.call(reasonKeys, error.code)
    && typeof error.detail === "string";
}

/** Only controlled localized reasons are shown; native detail stays in developer logs. */
export function getOperationErrorMessage(error: unknown): string {
  return isOperationError(error)
    ? t(reasonKeys[error.code])
    : t("operationFeedback.unknownReason");
}

export function reportUserError(localizedTitle: string, error: unknown, diagnosticContext?: string): void {
  logError(diagnosticContext ?? localizedTitle, error);
  showToast(`${localizedTitle}: ${getOperationErrorMessage(error)}`, "error");
}

export function reportOperationError(operation: ClipboardOperation, error: unknown): void {
  reportUserError(t(`operationFeedback.operations.${operation}`), error, `Clipboard operation '${operation}' failed`);
}

export async function runClipboardOperation<T = void>(
  operation: ClipboardOperation,
  args?: Record<string, unknown>,
): Promise<OperationResult<T>> {
  try {
    const value = await invoke<T>(operationCommands[operation], args);
    if (operation === "save" && value === false) return { status: "cancelled" };
    return { status: "success", value };
  } catch (error) {
    reportOperationError(operation, error);
    return { status: "failed" };
  }
}

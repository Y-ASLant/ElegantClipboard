# Windows 窗口置顶验收（2026-09-19）

- 主工具栏增加会话级“置顶 / 已置顶”按钮，沿用现有 small outline 尺寸与选中态。Windows 适配集中在 `tray.rs`，通过已有原生窗口句柄调用 `SetWindowPos(HWND_TOPMOST/HWND_NOTOPMOST)`，使用 `SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE`，不会在切换时移动、缩放或额外激活窗口。
- 置顶时受控粘贴不再隐藏历史窗口，仍尝试把键盘焦点交回原目标应用；取消置顶后恢复现有隐藏、失败时重新显示的流程。置顶不写入设置，应用重启后默认关闭；关闭到托盘再唤回时保留本次会话状态。
- 使用 `target/qa/window-pin-ui-d79f786e6ac04812b759f208bb03eb02` 隔离目录和真实 GPUI 窗口验收。初始 `WS_EX_TOPMOST=false`，点击后为 `true`，再次点击后为 `false`；退出并用同一目录重启后仍为 `false`。420×520 最小外框中“已置顶”“导出备份”“采集未启用”均位于窗口边界内且互不重叠，截图为忽略目录中的 `pinned-min-window.png`。
- 该验收未写入系统剪贴板，也未向外部应用注入粘贴快捷键；置顶状态下的可见性分支由代码路径检查覆盖，跨应用最终粘贴仍需独立交互验收。

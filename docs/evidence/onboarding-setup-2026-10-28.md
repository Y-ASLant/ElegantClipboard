# 首次使用与配置引导（2026-10-28）

将首次引导从循环功能示意改为可以照着操作的入门流程。beta.9 的历史验证保留在 [首次引导布局与动效](onboarding-motion-2026-10-27.md)。

## 内容与交互

1. 唤出与隐藏：展示真实已保存快捷键，教用户按一次隐藏、再按一次回来，并以文字提示“可在设置中修改”。新数据目录默认使用 Alt+C；已有组合、关闭状态、旧 `ctrl_shift_v` 预设继续保持原意。
2. 复制记录：解释正常复制进入历史、打开后聚焦搜索，以及本地历史和普通文件仅存路径。监听未启用或已暂停时显示真实状态。
3. 选择与粘贴：从目标应用唤出，方向键选择或点击记录，Enter 使用记录，Shift+Enter 使用纯文本。区分目标应用唤出与托盘打开：前者在回贴能力可用且目标有效时尝试自动粘贴，后者只复制。平台能力受限时显示现有能力提示。
4. 初始偏好：直接配置系统/浅色/深色主题、开机启动和唤出后搜索聚焦。复用已有命令与 pending/ACK 状态，保存中禁止步骤导航与重复提交；成功后更新显示，完成引导仍需最后一步后台确认。

移除旧 `onboarding_preview.rs` 和 `onboarding_scenes.rs`，教程和偏好分别由 `onboarding_lessons.rs`、`onboarding_preferences.rs` 承担。保留无跳过、Esc 不完成、外点不自动隐藏，以及真实热键切换显隐不丢失步骤的语义。剪贴板主窗口与引导不显示应用内 toast；已打开设置窗口时沿用设置通知，其他反馈交给系统原生通知，不在以后打开设置时重放。

## 布局与参考

- 参考 [Chakra Steps](https://chakra-ui.com/docs/components/steps)、[Card](https://chakra-ui.com/docs/components/card)、[Kbd](https://chakra-ui.com/docs/components/kbd) 和 [Presence](https://chakra-ui.com/docs/components/presence) 的步骤层级、轻边框、按键块和淡入滑入方式。
- 仍为 Rust/GPUI 原生界面，无 React、WebView 或新的组件库。交互控件使用官方 GPUI Kit Stepper、Kbd、Button、TabBar、Switch 和 Scrollable；应用教程内容用 GPUI 布局。
- 顶部进度、上方标题与说明、正文卡片、底部固定导航。窗口较小时正文独立滚动，不将按钮挤出客户区；每步使用独立滚动容器标识，避免共享上一页滚动位置。
- 步骤内容 240 ms 淡入并上移 8 px，首步按键 360 ms 入场。没有无限循环示意，GPUI reduce_motion 下不挂载入场动画。

## Windows 原生验证

使用独立数据目录与 `--no-monitor`，不写用户系统剪贴板，不读取用户历史。保留用户运行实例，测试只操作指定进程的窗口。中文注册热键测试使用 Alt+F12，避免与用户当前实例争用 Alt+C；英文禁用热键分支显示真实状态。

已观察并验证：

- 中文浅色 560×760 四步，首屏实际显示 Alt+F12；真实按键使窗口隐藏再显示。
- 中文 420×520 正文滚动、英文深色 420×520 长文案换行、固定导航、禁用热键提示。
- 引导内主题保存为 dark，搜索聚焦保存为 true，均从隔离数据库只读确认。
- 开机启动在英文隔离目录开启后，仅创建该测试目录的自有 Run 条目；从同一开关关闭后条目数量恢复为零。
- 重构初版曾验证修改快捷键入口能打开对应设置页；后续按简约要求移除该按钮，改为“可在设置中修改”文字提示。
- 最终 Release 的新目录不保存主快捷键键值，首屏显示 Alt+C；当前用户实例保留运行，未向其注入 Alt+C。
- 英文小窗口滚动某一步后，下一步从标题开始显示，未继承上一页偏移。
- 连续 70 ms 截图可见步骤内容由淡到实、上移到位；最终 Release 验证搜索聚焦保存、Esc 不写完成偏好、最后一步成功完成。重启同一目录直接进入历史页，保留已保存偏好。

截图保存在忽略目录 `target/qa/onboarding-setup-*`。本段原生操作的剪贴板序列为 3193。临时驱动在验收后删除，所有测试实例已退出。

## 设置窗口专属通知复验（系统通知接入前）

使用另一个独立目录与 `--no-monitor` 启动 Windows 原生窗口：引导内切换搜索聚焦，数据库确认保存但主窗口无 toast；随后打开设置，无旧通知重放。在设置页修改粘贴按键，设置窗口显示保存成功通知，主窗口无通知；激活主窗口后再修改引导偏好，通知仍只出现在已打开的设置窗口。关闭设置后完成引导，历史页没有欢迎 toast。截图为 `target/qa/settings-toast-*.png`，临时驱动和测试进程已清理。

驱动只读取剪贴板序列，没有剪贴板写入操作；启动观察为 3250，稍后为 3255，不将跨时间段的变化归因于测试进程。

通知路由改动后的源码大小、格式、all-targets check、Clippy 和 404 项测试均通过。默认 Release 构建在链接阶段缺失 `libtray_icon` 产物（`os error 2`）；随后检查发现默认 `target/release/deps` 已不存在，改用独立 `target/notification-verified` 目录构建成功（退出码 0），并在独立临时数据目录以 `--smoke-test` 验证原生启动与自动退出，退出码为 0。

## 系统通知与精简文案

主窗口反馈改为平台系统通知，设置窗口存在时继续使用设置内 Notification。Windows 按 [Microsoft AppUserModelID 注册说明](https://learn.microsoft.com/en-us/windows/win32/shell/enable-desktop-toast-with-appusermodelid) 使用应用自身 AUMID 与 WinRT Toast；首次实际发送时在开始菜单独立的 `ElegantClipboard` 子目录注册带身份的快捷方式，不覆盖旧版同名快捷方式。Linux 使用 `org.freedesktop.Notifications` 的 session D-Bus，macOS 使用自身 `.app` 的 UNUserNotificationCenter，并仅在首次实际发送时请求系统授权。应用前台由长期持有的 delegate 允许原生横幅；系统权限关闭或投递失败时只记录错误，不循环通知或恢复应用内 toast。

首屏改为“唤出剪贴板”，仅保留快捷键、显隐说明与“可在设置中修改”；移除修改按钮、重复标签与练习段落。其余步骤标题和说明同步精简，末页只保留设置入口提示。中文浅色与英文深色 420×520 原生窗口均已截图查看，未显示修改按钮。

Windows 隔离原生验收已确认主窗口保存搜索聚焦后没有应用内 toast，使用该 AUMID 的 WinRT 通知历史读到“ElegantClipboard搜索焦点设置已保存”。系统通知 smoke 和最终 Release 的真实操作发送均返回成功，Release 标准错误为空；桌面横幅截图未捕捉到可读通知，因此不宣称已观察到横幅，实际展示服从系统设置。标准错误为空，剪贴板序列保持 3535。macOS ARM64 与 Linux x64 通知模块通过目标平台编译和 Clippy `-D warnings`，本机未验证这两个桌面系统的实际投递。

通知消息使用 XML 文本节点，回归验证特殊字符不会注入节点；通知身份快捷方式回归在临时目录验证重复注册与无关文件保护，不写真实开始菜单。原生发送 smoke 为临时程序，验收后移除。

## 自动检查

引导重构阶段 `just check`、`just test` 和 `just build` 全部通过，258 个 Rust 文件均不超过 500 行，404 项测试通过。系统通知接入后的源码大小、格式、all-targets check、Clippy、Release 构建与测试通过：262 个 Rust 文件均不超过 500 行，406 项测试通过（core 218、platform 131、GPUI 57），2 项原有忽略。保留既有热键兼容与冲突覆盖，另增加 XML 消息安全和临时目录通知快捷方式所有权回归；未添加绑定文案或控件接线的测试。

## 验证边界

本次原生视觉与交互验收在 Windows 执行。macOS/Linux 共用界面与偏好协议，文案按平台展示 Cmd 或 Ctrl；没有在这两个桌面系统观察界面。复制粘贴教程没有注入实际复制或粘贴按键，不能替代跨应用回贴验收。GPUI reduce_motion 分支沿用框架状态，没有增加操作系统偏好同步。

# Windows 搜索框中文输入法验收（2026-09-19）

在活动的 Windows 11 桌面会话中，使用独立目录 `target/qa/ime-input-20260919` 启动 release 版 GPUI 窗口，参数为 `--no-monitor --data-dir`。该目录仅放入一条合成文本“你好世界，中文输入法搜索验证”，未读取旧版或用户数据，也没有操作系统剪贴板。

通过 Windows UI Automation 定位“搜索剪贴板历史…”输入框，实际点击后发送 `n i h a o` 键，再按空格确认候选。确认前输入框的 ValuePattern 为 `ni'hao`，确认后为“你好”；等待搜索防抖后，列表中恰有一条“查看”入口，命中合成记录。测试脚本在结束时恢复原前台窗口和光标位置，并结束自己启动的进程，退出码为 0。

在独立目录 `target/qa/ime-group-20260919` 中，用真实鼠标点击“＋ 新建”和“分组名称”，发送 `gongzuo` 后 UI Automation 的 ValuePattern 为 `gong'zuo`；按空格确认后为“工作”，点击“创建”后界面出现同名分组。SQLite 名称码点为 `U+5DE5 U+4F5C`，`PRAGMA quick_check=ok`；同目录重新启动窗口冒烟退出码为 0。整个过程禁用剪贴板采集。

在另一个独立目录 `target/qa/ime-edit-20260919` 中，仅放入一条合成“原文”记录。从卡片“查看”进入“编辑”，真实点击“编辑文本”并清空原文，发送 `zhongwen` 后 ValuePattern 为 `zhong'wen`；按空格确认后为“中文”。点击“保存文本”后 SQLite `text_content` 和 `preview` 均为码点 `U+4E2D U+6587`，`content_type=text`、`char_count=2`，`PRAGMA quick_check=ok`；同目录重新启动窗口冒烟退出码为 0。测试结束时恢复原前台窗口和光标位置，并结束各自启动的进程。

这证明当前桌面会话的拼音组合输入、候选确认，以及搜索、分组创建和正文编辑三个主要输入入口能够连通；仍未覆盖其他输入法、分组重命名等其他输入场景、真实跨应用粘贴或长时间运行。自动化最初仅调用 UI Automation `SetFocus` 时按键未进入搜索框；改用真实鼠标点击后验收通过，因此不把前者当作输入法失败。

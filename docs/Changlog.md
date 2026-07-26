# ElegantClipboard 更新日志

## v1.2.6
**发布日期：** 2026年7月26日

- fix: 修复 WebDAV 跨设备同步图片/文件时 metadata 与媒体不一致——导出仅包含实际上传成功的条目，导入按大小限制过滤并 reconcile 本地路径 https://github.com/Y-ASLant/ElegantClipboard/issues/132
- fix: WebDAV 媒体后台下载/自动同步完成后推送 `webdav-media-ready`，主窗口自动刷新列表以显示预览与路径
- fix: RDP 等无真实路径的文件复制场景，从 `FileGroupDescriptorW` / `FileGroupDescriptor` 解析虚拟文件名用于展示与去重哈希
- fix: 多文件同步时要求全部文件通过大小校验且哈希成功才纳入导出；空 `file_paths` 不再误计入媒体清单
- fix: 导出端裁剪未被条目引用的孤儿 media_map；导入端校验 media_map 完整性，拒绝无 blob backing 的 ghost 条目
- fix: 同内容多路径文件各自保留 media 映射；部分文件哈希失败时不写入任何 media
- fix: 自动/手动同步 reconcile 后若无待下载媒体也会通知前端刷新

## v1.2.5
**发布日期：** 2026年7月26日

- fix: 修复复制 NAS/UNC 超大图片文件（`files` 类型）后，每次启动重读整文件生成预览导致网络占用暴涨与闪退 https://github.com/Y-ASLant/ElegantClipboard/issues/143
- feat: `files` 类型单张图片预览统一应用大小限制——本地路径遵循设置「单张图片最大大小」（默认 50MB），UNC/网络路径固定 10MB
- perf: 超大文件已确认超限时跳过 batch 文件状态 IPC，避免对 NAS 发起多余存在性检查
- fix: 修复 images 目录长期累积 `.dib` 伴侣文件、清空历史后仍占用磁盘的问题 https://github.com/Y-ASLant/ElegantClipboard/issues/142
- fix(ui): 子菜单改用 Portal 渲染并增加碰撞内边距；视口变窄时自动关闭右键菜单，缓解菜单位置异常
- feat: 文件图片预览加载失败时会话内缓存，避免同会话重复尝试
- feat: RTF 哈希规范化，去除易变字段，提升富文本去重稳定性
- refactor: 移除 CF_DIB 伴侣文件遗留支持，启动时清理旧版 `.dib`
- refactor: 剪贴板内容类型过滤逻辑移至 `ClipChangeSettings`，热路径单次批量读取设置
- refactor: 清理冗余代码并统一部分错误消息为中文
- chore: 依赖安全更新（`crossbeam-epoch`）

## v1.2.4
**发布日期：** 2026年7月18日

- feat: 粘贴按键选择——设置 → 快捷键新增粘贴按键选项，支持 Ctrl+V（默认）与 Shift+Insert 两种模式 https://github.com/Y-ASLant/ElegantClipboard/issues/134
- feat: 清空历史支持免确认模式——确认对话框新增"不再询问"复选框，开启后点击清空按钮直接执行并显示 toast https://github.com/Y-ASLant/ElegantClipboard/issues/133
- feat: 系统托盘新增「检查更新」菜单项
- feat: WebDAV 同步支持记录最近同步时间，前端同步页实时展示
- feat: WebDAV 手动同步完成后自动清理状态提示
- fix: 修复新手引导在设置未加载完成时错误显示
- fix: 修复设置页主题预览色块颜色异常
- fix: WebDAV 跨设备同步媒体文件落地路径修复，不再依赖两端目录一致
- fix: WebDAV 同步并发保护，避免重复同步请求
- refactor: 优化 WebDAV 媒体同步——按类型分别限制大小，图片/文件同步独立线程


## v1.2.3
**发布日期：** 2026年7月5日

- feat: 列表布局新增瀑布流模式，支持虚拟滚动与高度缓存 https://github.com/Y-ASLant/ElegantClipboard/issues/126
- feat: 文本/图片预览窗口实时同步主窗口主题、圆角与字体设置
- feat: 设置窗口独立构建入口，缩短打开设置页时的加载时间
- perf: 优化应用启动——路由懒加载、并行初始化、延后非关键配置加载
- fix: 修复文本预览面板背景与主窗口卡片颜色不一致
- refactor: 重构 Design Token 体系（`surface-tokens.css`），统一圆角、阴影、语义色与 `.transition-surface` / `.elevation-*` 工具类
- style: 全应用组件样式迁移至 Token；优化卡片悬停过渡
- chore: 调整悬停预览延迟与粘贴后置顶默认值；更新依赖并完善 `cargo audit` 忽略配置

## v1.2.0
**发布日期：** 2026年7月4日

- feat: 条目级自定义分组——`group_id` 字段、组内排序、分组视图过滤；导入/导出支持分组数据
- feat: 文件剪贴板高保真还原——捕获 CF_HDROP 伴生格式，新增 `file_payload` 存储，支持 staging 与合并粘贴
- feat: 升级 clipboard-rs 至 v0.3.5 本地分叉，增加 CF_DIB 图片格式读写
- feat: 工具栏新增 WebDAV 同步快捷按钮
- feat: 粘贴音效改由后端统一触发，覆盖卡片粘贴、合并粘贴、快捷键粘贴、粘贴为路径等场景；设置页支持音效试听
- feat: 新增 `batch_get_item_file_status` 批量检查文件有效性，减少 IPC 调用
- feat: 插件可用性检查——WebDAV / 翻译插件启用状态统一管理，UI 动态响应
- fix: 移除 `content_type` 数据库 CHECK 约束，迁移时不再因历史脏数据导致整表重建失败 https://github.com/Y-ASLant/ElegantClipboard/issues/125
- fix: RTF 剪贴板备份/恢复改用原始 buffer 读写，避免格式丢失
- fix: URL 类型检测——未启用 URL 监听时不误匹配 text 类型
- fix: 图片哈希长度由 16 位扩展至 32 位；更新文本内容时清空图片/文件路径；upsert 不再覆盖 `created_at`
- fix: 翻译失败通知与翻译窗口初始白屏
- refactor: 设置页卡片统一为 `SettingsCard` 组件；优化 Tab 切换动画与间距
- refactor: 重定义圆角 CSS 变量，新增 `interactive-surface` 交互样式类
- style: 同步图标改为云箭头；工具栏按钮圆角统一为 `rounded-md`
- chore: 更新 Rust 依赖至最新版本；添加 cargo 审计配置

## v1.1.5
**发布日期：** 2026年7月1日

- feat: 剪贴板库统一，arboard + clipboard-master 替换为 clipboard-rs，统一读写与监听
- feat: RTF 存储重构为 base64，优化多格式写入流程
- feat: 剪贴板内容处理移至独立 Worker 线程，避免阻塞主线程
- feat: 剪贴板处理引入增量更新，减少不必要的全量刷新
- feat: 图片预览根据元数据设置 aspect-ratio，加载前预留准确高度，减少布局抖动
- feat: 优化失效文件检测逻辑与样式展示
- feat: 日志文件路径改为可点击直接打开文件
- feat: 翻译模块错误消息和选项标签支持多语言国际化
- fix: 简化剪贴板写入验证逻辑并修正密度键名
- fix: 用原子计数器确保测试临时数据库文件名唯一，修复 DatabaseBusy 竞态
- fix: 管理员启动降权时改为同步启动降权实例，避免与单实例插件冲突
- refactor: 增强应用健壮性与数据安全性
- refactor: 增强日志系统并改进错误处理
- refactor: WebDAV 设置引入快照脏检查，避免无变更时写入
- refactor: 数据库添加语义哈希回填迁移的版本守卫
- refactor: WebDAV 自动同步线程 spawn 失败改为日志记录而非 panic
- refactor: 剪贴板热路径批量查询设置，减少数据库访问
- perf: 加速回到顶部平滑滚动动画
- perf: WebDAV 设置 Hook 性能优化，减少无效更新
- perf: 图片自动高度默认值改为关闭，减少不必要的布局计算

---


## v1.1.0
**发布日期：** 2026年6月30日

- feat: 剪贴板支持富文本格式（RTF）读取与写回
- feat: 富文本写回支持 HTML + RTF + Unicode 多格式同时写入
- fix: CI portable 包下载 pattern 错误，导致下一次发布缺便携版
- fix: CI release draft 竞态条件，拆分 portable 上传为独立 job
- fix: 管理员模式下托盘重启和数据导入后重启可能产生双实例
- fix: 设置页加载失败后 auto-save 永久关闭
- fix: 划词翻译选中内容与剪贴板相同时无法翻译的回归
- fix: 划词翻译剪贴板备份/恢复支持 HTML 和图片，不再丢失富文本内容
- fix: 剪贴板读取增加 TOCTOU 检测，序列号变化时自动重试
- refactor: 剪贴板操作统一迁移至 arboard，移除 clipboard-rs 依赖
- refactor: 移除非 Windows 平台占位代码
- refactor: 重构剪贴板 RTF 处理逻辑，提取独立模块
- refactor: 重构翻译结果页底部操作栏布局
- perf: 窗口显示触发链路优化：新增 PositionCache 缓存定位设置
- perf: 移除窗口显示时冗余的 IPC 调用
- style: 调整复制成功动画的时长和关键帧
- style: 调整工具栏按钮尺寸和信息提示颜色细节
- build: 添加 Clippy 代码检查到 Makefile

---


## v1.0.3
**发布日期：** 2026年6月30日

- fix: 修复设置页面初始化时自动保存的误触发 #119

---

## v1.0.2
**发布日期：** 2026年6月30日

- fix: 修复复制时造成异常闪退问题
- feat: 文字翻译结果窗口支持固定及关闭逻辑（失去焦点关闭）


---

## v1.0.1
**发布日期：** 2026年6月29日

- fix: 修复鼠标点击条目后再次呼出时键盘导航失效的问题
- fix: 增加 0xc0000409 异常闪退日志捕捉
- docs: 更新文档说明


---

## v1.0.0
**发布日期：** 2026年6月28日

- feat: WebDAV 同步插件，支持多设备剪贴板同步（坚果云、Nextcloud 等）
- feat: 文本翻译插件，支持条目翻译与划词翻译（Microsoft / Google / 百度 / DeepLX / OpenAI 兼容等）
- feat: 插件总开关，未启用时零后台占用
- feat: URL 链接内容类型，自动识别 http(s) / ftp / www 链接并迁移存量记录
- feat: 快速粘贴扩展为 10 个槽位（含数字 0）
- feat: 复制成功反馈与列表入场动画
- feat: 新手引导
- feat: i18n 支持，多语言（简体中文 / English / 繁體中文）
- feat: 设置 → 关于可查看版本更新日志
- feat: 系统托盘图标显示/隐藏
- fix: 从 0.9.x 升级后可能无法启动的问题
- fix: 安装/更新失败及开机自启失效
- fix: 收藏顺序乱跳
- fix: 多屏不同 DPI 下窗口位置偏移、尺寸异常、双屏唤醒首次不居中
- fix: 超大图片导致卡死，新增单张图片大小限制
- fix: 预览窗口偶现无法消失、预览相关设置初始状态不正确
- fix: Explorer 重启后已隐藏的托盘图标重新出现
- fix: 资源管理器重命名时 Win+V 打断输入
- fix: 显示区域提示设置无法持久保存
- fix: 小键盘数字键不支持快捷键
- fix: Win+V 模式下可清空快捷粘贴快捷键
- fix: 拖拽指针样式改为系统 move 光标
- fix: 卡片间距设置无效
- fix: 列表顺序异常跳动问题
- fix: 设置页 Tab 滚动位置未重置、导航指示器不跟随切换
- fix: WebDAV / 翻译插件、管理员模式与同步相关边界问题
- perf: 列表滚动与搜索性能优化，修复搜索时偶发白屏
- perf: 搜索关键词上下文与图片加载体验
- perf: 外观主题 Tab 字体列表加载速度
- refactor: 移除设置内搜索，调整设置页布局与预览默认值
- chore: 自 v0.9.30 以来首个正式版；插件默认关闭，需手动启用


---

## v0.9.30
**发布日期：** 2026年3月21日

- fix: 修复键盘导航从剪贴板进入后点击关闭逻辑触发错误的问题
- fix: 修复预览窗口偶现的无法消失问题
- fix: 唤醒位置记忆改为重启后依旧有效
- style: 界面优化更新

---

## v0.9.27
**发布日期：** 2026年3月10日

- fix: 修复小键盘数字快捷键不生效问题
- fix: 修复键盘导航失效问题
- fix: 更新去重算法
- feat: 增加收藏页粘贴快捷键
- feat: 增加字体自定义
- style: 改进拖拽区域提示样式
- fix: 改进有歧义的设置选项描述

---

## v0.9.19
**发布日期：** 2026年3月8日

- feat: 增加应用过滤
- perf: 检查更新下载优化
- feat: 增加便携版发布
- feat: 增加剪贴板唤出位置自定义
- fix: 更新时默认跟随系统代理
- feat: 增加复制监听文件类型选项
- fix: 修复长时间运行后预览窗口层级问题
- fix: 修复偶现的预览窗口无法消失问题
- chore: 补全日志记录
- fix: 移除剪贴板唤出时 Y 轴翻转逻辑，改为向上位移
- refactor: 移除无用依赖、Rust 2024 语法、设置界面优化、开放更多自定义

---

## v0.9.3
**发布日期：** 2026年3月7日

- feat: 增加批量删除功能
- feat: 增加入场动画（默认关闭）
- feat: 托盘增加快速暂停监控和恢复监控
- feat: 托盘增加临时禁用快捷键
- feat: 返回置顶按钮增加可拖拽位置
- feat: 增加程序启动时检查更新功能（默认开启，可手动关闭）
- style: 卡片拖拽体验优化
- style: 设置界面调整及其他界面细节优化

---

## v0.8.5
**发布日期：** 2026年3月4日

- fix: 增强清理逻辑
- fix: 修复窗口焦点问题
- feat: 增加预览缩放无界模式，允许图片预览超出屏幕区域，最高缩放 500%
- feat: 增加文本预览，适配直角模式、深色模式
- feat: 增加区域提示（默认开启，可关闭）
- perf: 设置界面组件样式优化、线程占用优化
- refactor: 移除无用依赖和代码
- fix: 默认关闭自动重置状态、预览触发延迟默认改为 500ms
- fix: 重写拖拽区域和显示区域，修复偶现的点击无法粘贴问题

---

## v0.7.77
**发布日期：** 2026年3月3日

- fix: 修复更新后开机自启配置失效问题
- fix: 修复图片预览相关问题
- style: 优化预览窗口效果

---

## v0.7.65
**发布日期：** 2026年3月2日

- feat: 增加分组功能
- feat: 增加粘贴后置顶功能
- feat: 右键菜单增加移动至分组
- refactor: 将底部分类改为文本和其它（非文本包含文件、截图等）
- fix: 修复文本编辑窗口高级效果丢失问题

---

## v0.7.39
**发布日期：** 2026年2月28日

- refactor: 重构主题系统优化深色模式显示效果
- fix: 修复深色模式下 Mica 等开启窗口效果后显示不完整问题

---

## v0.7.36
**发布日期：** 2026年2月27日

- feat: 增加更加详细的日志记录
- fix: 截图增加重试，避免锁竞争问题造成的无法记录截图问题
- ci: 增加自动更新 winget 仓库

---

## v0.7.35
**发布日期：** 2026年2月27日

- feat: 增加剪贴板窗口特效 Mica、Acrylic、Tabbed
- feat: 增加剪贴板窗口显示自定义及排序功能
- feat: 增加数据清理功能
- feat: 搜索栏增加一键清空按钮
- fix: 修复一些异常
- refactor: 清理一些无用依赖
- fix: 默认关闭搜索栏聚焦
- feat: 增加 scoop、Winget 安装方式

---

## v0.7.22
**发布日期：** 2026年2月26日

- fix: 修复多屏不同缩放下无法获取窗口焦点问题
- fix: 修复快捷键粘贴时连键问题
- fix: 修复固定窗口粘贴时，焦点错误造成的无法粘贴问题
- style: 设置界面调整
- feat: 增加更多自定义
- chore: 增加更详细的日志记录

---

## v0.7.16
**发布日期：** 2026年2月22日

> 该版本更新后，数据存储路径需要手动选择才能加载旧版本剪贴板数据（适配免安装便携版）

- fix: 修复底部组件在高 DPI 显示错位问题
- fix: 修复锁定面板时，无法粘贴问题
- fix: 修复更新过程无法打断问题
- fix: 取消收藏时卡片不会消失问题
- fix: ESC、Delete 按键失效问题
- fix: 托盘点击触发逻辑问题
- feat: 增加数据导入导出功能
- feat: 增加剪贴板窗口大小自定义
- feat: 增加更多自定义设置
- feat: 增加深色、浅色模式切换
- style: 增加操作反馈
- style: 设置界面图标更改

---

## v0.7.1
**发布日期：** 2026年2月21日

- fix: 优化日志显示，显示当前版本至最新版本之间的日志
- feat: 搜索栏增加更多自定义
- fix: 修复更新版本时架构错误问题

---

## v0.7.0
**发布日期：** 2026年2月21日

- fix: 修复火狐浏览器内核，输入时光标出现漏斗问题
- fix: 修复管理员模式开机自启问题
- fix: 修复复制时软件来源问题
- feat: 增加左右方向键切换分类

---

## v0.6.15
**发布日期：** 2026年2月21日

- feat: 增加键盘导航

---

## v0.6.10
**发布日期：** 2026年2月20日

- feat: 底部增加分类切换

---

## v0.6.9
**发布日期：** 2026年2月20日

- fix: 修复剪贴板被底部任务栏遮挡问题
- feat: 增加日志记录保存
- fix: 修复上版本造成的剪贴板序号只显示 1~9 的问题

---

## v0.6.8
**发布日期：** 2026年2月19日

- feat: 增加快捷键粘贴

---

## v0.6.7
**发布日期：** 2026年2月18日

- ci: 增加 arm64 Github Action 支持
- feat: 增加自动清理历史记录默认 30 天
- fix: 修复可能存在的卡键问题
- fix: 修复更新时，下载版本错误问题
- fix: 托盘重启时，权限问题
- docs: 更新相关文档
- chore: 依赖更新

---

## v0.6.6
**发布日期：** 2026年2月13日

- fix: 修复自定义快捷键无效问题
- feat: 文本类增加编辑窗口（待优化、目前只针对不带格式的文本）

---

## v0.6.5
**发布日期：** 2026年2月12日

- feat: 增加在线更新功能（当前仅支持手动检查更新、非检查更新时不会有任何网络请求）
- feat: 增加直角主题（Windows 10）
- fix: 修复多实例问题，仅允许单实例，多次启动会有弹窗通知

---

## v0.6.4
**发布日期：** 2026年2月11日

- feat: 显示复制来源增强、支持更多的自定义
- fix: 修复管理员模式下开机自启失效问题
- style: 界面动效统一
- perf: 性能优化

---

## v0.6.3
**发布日期：** 2026年2月11日

- feat: 增加内容复制来源
- refactor: 注释简化
- fix: 修复 v0.6.2 索引 Bug

---

## v0.6.1
**发布日期：** 2026年2月10日

- style: 统一主题界面样式
- chore: 精简依赖
- perf: 滚动阈值优化
- fix: 修复多屏幕、混合 DPI 下显示异常问题
- feat: 增加 ESC 关闭设置窗口
- style: 预览界面增加阴影

---

## v0.6.0
**发布日期：** 2026年2月10日

- fix: 关闭时重置剪贴板状态
- feat: 右键菜单增加另存为
- fix: 搜索 bug 修复
- fix: 修复界面总记录数只显示 100 条的问题
- feat: 搜索高亮，搜索后显示效果优化
- feat: 增加数据信息大小统计（手动刷新，其余时间无额外开销）

---

## v0.5.6
**发布日期：** 2026年2月10日

- fix: 修复 HLS 转 RGB 精度问题（动态颜色更接近系统颜色）
- fix: 修复主题界面预览小方块颜色问题

---

## v0.5.5
**发布日期：** 2026年2月10日

- feat: 增加动态主题
- feat: Windows 原生的 WM_SETTINGCHANGE 消息机制
- perf: 零轮询、零 CPU 开销、零 React 开销，模块级方案，无限接近原生性能
- style: 设置界面样式调整

---

## v0.5.1
**发布日期：** 2026年2月9日

- feat: 右下角增加一键返回顶部悬浮按钮
- fix: 修复卸载旧版本时无法停止旧版软件造成的无法卸载问题
- fix: 默认关闭图片预览
- fix: 默认开启图片最大高度 512px
- feat: 增加 ESC 关闭弹窗、隐藏剪贴板

---

## v0.5.0
**发布日期：** 2026年2月9日

- refactor: update_shortcut 改为 register() → on_shortcut() 附带 toggle handler
- refactor: process_image 从异步 thread::spawn 写入改为同步 fs::write
- fix: resume() 中 fetch_sub + store 改为 fetch_update 原子 CAS 防下溢
- fix: 修复更新时无法终止正在运行的程序
- fix: 修复 Bounce 动画误触发——增加 hasDraggedRef，仅在实际拖拽结束后播放
- fix: 修复跨区域拖拽位置丢失——跨区域时先 togglePin 再 moveItem
- fix: Settings 自动保存条件优化——用 settingsLoadedRef 替代检查默认值
- fix: 补充 memo 比较中遗漏的 text_content、preview、file_paths 字段
- fix: 搜索/刷新竞态修复——引入 _fetchId 计数器，丢弃过期响应
- refactor: 移除无用的 pinnedItems IPC
- refactor: 将 itemsRef 赋值移入 useEffect，避免渲染期间写入 ref

---

## v0.4.9
**发布日期：** 2026年2月8日

- feat: 增加图片非预览模式下的大小，实现关闭预览时仍有良好预览效果
- feat: 设置界面可手动关闭该模式，开启悬浮预览

---

## v0.4.8
**发布日期：** 2026年2月8日

- fix: 修复预览界面在某些情况下的显示异常
- fix: 修复预览时点击滚动条导致预览界面背景显示异常
- fix: 置顶与非置顶卡片序号连续
- refactor: 剪贴板滚动组件更换，滚动更丝滑、样式更符合界面

---

## v0.4.5
**发布日期：** 2026年2月6日

- feat: 图片悬浮预览——鼠标悬停缩略图 300ms 弹出独立预览窗口
- feat: Ctrl+滚轮缩放——预览窗口支持平滑缩放
- feat: 缩放百分比显示——缩放时右下角显示百分比，1.2秒自动淡出
- feat: 预览位置偏好——设置中可选自动/左侧/右侧
- feat: 图片预览开关——设置中可启用/禁用悬浮预览
- feat: 缩放步进可调——设置中可调整 5%-50%
- feat: 单图片文件预览——复制图片文件自动显示预览
- ci: GitHub Actions CI——push/PR 自动执行 ESLint + TypeScript + Cargo 检查
- ci: GitHub Actions Release——推送 tag 自动构建
- perf: 图片加载改为 Asset Protocol（替代 base64，零编码开销，浏览器自动缓存）
- refactor: 代码模块化重构——ClipboardItemCard 拆分为 3 个文件
- refactor: Rust 命令模块化——commands/mod.rs 拆分为 clipboard/settings/file_ops
- refactor: 快捷键解析独立——提取 shortcut.rs 模块
- refactor: 消除快捷键读取重复——4 处重复代码统一复用 get_current_shortcut()
- chore: 移除 lazy_static，迁移到 std::sync::LazyLock（Rust 1.80+）
- chore: 移除 base64，图片改用 Asset Protocol 加载
- fix: 预览窗口隐藏时清空图片，防止下次显示时闪烁旧图

---

## v0.4.0
**发布日期：** 2026年2月4日

- feat: 预览弹窗信息支持自定义，优化预览交互反馈
- feat: 增加图片预览，Ctrl+滚轮可放大缩小预览图片
- refactor: 减少外部依赖，重构图片缩略图实现方式
- feat: 添加以管理员身份启动功能；历史记录达数万条时查询性能及占用优化
- perf: 优化列表滚动；增加列表菜单；增加文件信息统计、文件复制、文件/文本类型区分
- feat: 增加卡片拖动效果和卡片置顶效果
- feat: 增加鼠标跟随

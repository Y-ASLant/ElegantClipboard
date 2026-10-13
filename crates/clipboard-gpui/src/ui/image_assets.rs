use super::*;
use gpui_kit::ImageSource;

/// 窗口隐藏多久之后释放解码后的图片。留出余量，避免「隐藏 → 马上再唤出」时
/// 反复解码同一批缩略图。
const IMAGE_RELEASE_DELAY: Duration = Duration::from_secs(30);

impl ClipboardView {
    /// 当前列表里会用到的图片路径：`image` 记录的受管图片，以及文件卡片解码出的原图。
    fn rendered_image_paths(&self) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();
        for item in &self.history.items {
            if item.content_type == "image"
                && let Some(path) = item.image_path.as_deref()
            {
                paths.insert(PathBuf::from(path));
            }
        }
        for info in self.file_card_info.values() {
            if let Some(path) = &info.image_path {
                paths.insert(path.clone());
            }
        }
        paths
    }

    /// 释放已经不在列表里的图片解码缓存。
    ///
    /// GPUI 的 asset cache 会把加载过的资源一直留着——`App::remove_asset` 的文档写得很明白：
    /// 「Pending loads and completed results are cached until `remove_asset`」。应用不主动清，
    /// 这些位图就永远不释放。而 `img()` 是**按原始分辨率**解码的，一张 1920×1080 的截图
    /// 就是约 8 MB，攒十几张就是上百 MB。
    ///
    /// 这里只在图片离开当前列表时回收，屏幕上仍在显示的图片不受影响，所以没有额外开销。
    pub(super) fn sync_image_assets(&mut self, cx: &mut App) {
        let needed = self.rendered_image_paths();
        for path in std::mem::take(&mut self.rendered_images) {
            if !needed.contains(&path) {
                ImageSource::from(path).remove_asset(cx);
            }
        }
        self.rendered_images = needed;
    }

    /// 窗口隐藏后延迟释放全部缩略图。窗口看不见时没必要占着解码位图；
    /// 在延迟到期前再次唤出会取消该任务，因此频繁显隐不会反复解码。
    pub(super) fn schedule_image_release(&mut self, cx: &mut Context<Self>) {
        self.image_release_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(IMAGE_RELEASE_DELAY).await;
            let _ = view.update(cx, |this, cx| {
                for path in std::mem::take(&mut this.rendered_images) {
                    ImageSource::from(path).remove_asset(cx);
                }
            });
        }));
    }
}

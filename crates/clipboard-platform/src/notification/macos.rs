use anyhow::{Context, Result, anyhow, bail};
use block2::{DynBlock, RcBlock};
use objc2::{
    AnyThread, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{Bool, ProtocolObject},
};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString, NSThread};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSettings,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::{
    path::Path,
    ptr::NonNull,
    sync::{Mutex, OnceLock, mpsc},
    time::Duration,
};

const BUNDLE_ID: &str = "com.aslant.elegant-clipboard-gpui";
static SEND_LOCK: Mutex<()> = Mutex::new(());
static DELEGATE: OnceLock<Retained<NotificationDelegate>> = OnceLock::new();
static AUTHORIZATION: Mutex<Option<mpsc::Receiver<Result<(), String>>>> = Mutex::new(None);

define_class!(
    // 无界面状态，可在系统通知中心的回调队列上使用。
    #[unsafe(super(NSObject))]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

fn native_error(error: *mut NSError) -> Option<String> {
    // NSError 只在回调内借用，跨线程仅传拥有所有权的错误文本。
    let error = unsafe { error.as_ref() }?;
    Some(format!(
        "{} ({}:{})",
        error.localizedDescription(),
        error.domain(),
        error.code()
    ))
}

fn authorize(center: &UNUserNotificationCenter) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    let settings = RcBlock::new(move |value: NonNull<UNNotificationSettings>| {
        let _ = sender.send(unsafe { value.as_ref() }.authorizationStatus());
    });
    center.getNotificationSettingsWithCompletionHandler(&settings);
    let status = receiver
        .recv_timeout(Duration::from_secs(15))
        .context("系统通知权限查询超时")?;
    if status == UNAuthorizationStatus::Denied {
        bail!("系统已关闭 ElegantClipboard 通知");
    }
    if status != UNAuthorizationStatus::NotDetermined {
        return Ok(());
    }
    let mut pending = AUTHORIZATION
        .lock()
        .map_err(|_| anyhow!("系统通知授权锁不可用"))?;
    if pending.is_none() {
        let (sender, receiver) = mpsc::channel();
        let permission = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            let result = match native_error(error) {
                Some(error) => Err(error),
                None if granted.as_bool() => Ok(()),
                None => Err("系统未授予通知权限".to_owned()),
            };
            let _ = sender.send(result);
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert,
            &permission,
        );
        *pending = Some(receiver);
    }
    // 超时不重复请求授权，后续发送继续等待同一次系统请求。
    let result = pending
        .as_ref()
        .expect("authorization receiver")
        .recv_timeout(Duration::from_secs(120));
    match result {
        Ok(result) => {
            *pending = None;
            result.map_err(|error| anyhow!(error))
        }
        Err(error) => Err(anyhow!("等待系统通知授权失败：{error}")),
    }
}

pub(super) fn show(title: &str, message: &str) -> Result<()> {
    if NSThread::isMainThread_class() {
        bail!("系统通知必须从后台线程发送");
    }
    let _guard = SEND_LOCK
        .lock()
        .map_err(|_| anyhow!("系统通知发送锁不可用"))?;
    autoreleasepool(|_| {
        let bundle = NSBundle::mainBundle();
        let identifier = bundle
            .bundleIdentifier()
            .context("系统通知需要从 ElegantClipboard.app 启动")?;
        if identifier.to_string() != BUNDLE_ID {
            bail!("系统通知应用身份与 ElegantClipboard 不一致");
        }
        let path = bundle.bundlePath().to_string();
        let path = Path::new(&path);
        if path.extension().and_then(|part| part.to_str()) != Some("app")
            || !path.join("Contents/Info.plist").is_file()
        {
            bail!("系统通知需要完整的 ElegantClipboard.app 应用包");
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        // 通知中心弱持有 delegate，应用全程保持强引用以支持前台横幅。
        let delegate = DELEGATE.get_or_init(NotificationDelegate::new);
        center.setDelegate(Some(ProtocolObject::from_ref(&**delegate)));
        authorize(&center)?;
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(message));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str("clipboard-feedback"),
            &content,
            None,
        );
        let (sender, receiver) = mpsc::channel();
        let submitted = RcBlock::new(move |error: *mut NSError| {
            let _ = sender.send(native_error(error).map_or(Ok(()), Err));
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&submitted));
        receiver
            .recv_timeout(Duration::from_secs(15))
            .context("发送系统通知超时")?
            .map_err(|error| anyhow!(error))
    })
}

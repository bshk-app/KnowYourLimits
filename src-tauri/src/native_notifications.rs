//! Native permission and delivery, including banners while the panel is open.
use crate::alerts::Alert;
use block2::{DynBlock, RcBlock};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{Bool, ProtocolObject},
    ClassType,
};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSound,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::sync::{Arc, Mutex};

define_class!(
    // SAFETY: NSObject imposes no subclass invariants; the delegate has no ivars.
    #[unsafe(super = NSObject)]
    #[name = "KYLNotificationDelegate"]
    struct NotificationDelegate;
    unsafe impl NSObjectProtocol for NotificationDelegate {}
    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);
// SAFETY: The delegate has no mutable state; its sole method invokes the
// system-provided completion block. It may be called on any notification queue.
unsafe impl Send for NotificationDelegate {}
unsafe impl Sync for NotificationDelegate {}

pub struct NotificationService {
    _delegate: Retained<NotificationDelegate>,
}
impl NotificationService {
    pub fn new() -> Self {
        // SAFETY: Inherited NSObject new initializes our stateless subclass.
        let delegate: Retained<NotificationDelegate> =
            unsafe { msg_send![NotificationDelegate::class(), new] };
        Self {
            _delegate: delegate,
        }
    }
    fn center(&self) -> Result<Retained<UNUserNotificationCenter>, String> {
        // The UserNotifications framework requires an actual application bundle.
        // Delay initialization so unbundled tauri dev/smoke runs remain usable.
        if !NSBundle::mainBundle()
            .bundlePath()
            .to_string()
            .ends_with(".app")
        {
            return Err("Уведомления доступны в установленной версии приложения.".into());
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*self._delegate)));
        Ok(center)
    }
    pub async fn request_permission(&self) -> Result<bool, String> {
        let (send, receive) = tokio::sync::oneshot::channel();
        let send = Arc::new(Mutex::new(Some(send)));
        {
            let completion = RcBlock::new(move |granted: Bool, error: *mut NSError| {
                if let Some(send) = send.lock().expect("permission callback").take() {
                    let _ = send.send(granted.as_bool() && error.is_null());
                }
            });
            self.center()?
                .requestAuthorizationWithOptions_completionHandler(
                    UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                    &completion,
                );
        }
        tokio::time::timeout(std::time::Duration::from_secs(60), receive)
            .await
            .map_err(|_| "Разрешение на уведомления ещё не получено.".to_owned())?
            .map_err(|_| "Не удалось проверить разрешение на уведомления.".to_owned())
    }
    fn request(alert: &Alert) -> Retained<UNNotificationRequest> {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&alert.title));
        content.setBody(&NSString::from_str(&alert.body));
        if alert.sound {
            content.setSound(Some(&UNNotificationSound::defaultSound()));
        }
        // Replacing the provider's previous delivered alert keeps Notification
        // Center compact; threshold deduplication belongs to the coordinator.
        UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!("quota-{}", alert.provider.label().to_lowercase())),
            &content,
            None,
        )
    }
    pub fn send(&self, alert: Alert) {
        if let Ok(center) = self.center() {
            center.addNotificationRequest_withCompletionHandler(&Self::request(&alert), None);
        }
    }
    pub async fn send_confirmed(&self, alert: Alert) -> Result<(), String> {
        let (send, receive) = tokio::sync::oneshot::channel();
        let send = Arc::new(Mutex::new(Some(send)));
        {
            let completion = RcBlock::new(move |error: *mut NSError| {
                if let Some(send) = send.lock().expect("delivery callback").take() {
                    let _ = send.send(error.is_null());
                }
            });
            self.center()?.addNotificationRequest_withCompletionHandler(
                &Self::request(&alert),
                Some(&completion),
            );
        }
        if tokio::time::timeout(std::time::Duration::from_secs(10), receive)
            .await
            .ok()
            .and_then(Result::ok)
            == Some(true)
        {
            Ok(())
        } else {
            Err("Не удалось отправить уведомление. Проверьте разрешение в настройках macOS.".into())
        }
    }
}

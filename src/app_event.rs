use gpui::SharedString;

#[derive(Clone)]
pub struct NotificationEvent(pub SharedString);

impl NotificationEvent {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self(message.into())
    }
}

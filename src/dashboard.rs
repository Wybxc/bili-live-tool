use gpui::*;
use gpui_component::{ActiveTheme, v_flex};

use crate::{
    broadcast_panel::BroadcastPanel, login_page::UserSession, profile_header::ProfileHeader,
};

pub enum DashboardEvent {
    Logout,
}

pub struct Dashboard {
    name: SharedString,
    avatar: ImageSource,
    broadcast: Entity<BroadcastPanel>,
}

impl EventEmitter<DashboardEvent> for Dashboard {}

impl Dashboard {
    pub fn new(session: UserSession, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let broadcast = cx.new(|cx| BroadcastPanel::new(session.user_id, window, cx));
        Self {
            name: session.name,
            avatar: ImageSource::from(session.face_url),
            broadcast,
        }
    }
}

impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_8()
            .gap_6()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(ProfileHeader {
                name: self.name.clone(),
                avatar: self.avatar.clone(),
                on_logout: cx.listener(|_, _, _, cx| cx.emit(DashboardEvent::Logout)),
            })
            .child(self.broadcast.clone())
    }
}

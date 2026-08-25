use gpui::*;
use gpui_component::{ActiveTheme, v_flex};

use crate::{
    broadcast_panel::BroadcastPanel,
    login_page::UserSession,
    profile_header::{ProfileHeader, ProfileHeaderEvent},
};

pub enum DashboardEvent {
    Logout,
}

pub struct Dashboard {
    profile: Entity<ProfileHeader>,
    broadcast: Entity<BroadcastPanel>,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<DashboardEvent> for Dashboard {}

impl Dashboard {
    pub fn new(session: UserSession, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let profile = cx.new(|cx| ProfileHeader::new(&session, window, cx));
        let broadcast = cx.new(|cx| BroadcastPanel::new(session.user_id, window, cx));
        let mut this = Self {
            profile: profile.clone(),
            broadcast: broadcast.clone(),
            subscriptions: Vec::new(),
        };
        this.subscriptions.push(cx.subscribe_in(
            &profile,
            window,
            |_, _, event: &ProfileHeaderEvent, _, cx| {
                let ProfileHeaderEvent::Logout = event;
                cx.emit(DashboardEvent::Logout);
            },
        ));
        this
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
            .child(self.profile.clone())
            .child(self.broadcast.clone())
    }
}

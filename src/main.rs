#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bili_api;
mod broadcast_panel;
mod dashboard;
mod login_page;
mod profile_header;
mod room_editor;
mod utils;

use gpui::*;
use gpui_component::{ActiveTheme, Root};

use crate::{
    dashboard::{Dashboard, DashboardEvent},
    login_page::{LoginEvent, LoginPage, UserSession},
};

enum AppPage {
    Login { view: Entity<LoginPage> },
    Dashboard { view: Entity<Dashboard> },
}

struct AppView {
    page: AppPage,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = cx.new(|cx| LoginPage::new(window, cx));
        Self::from_login(window, cx, view)
    }

    fn from_login(window: &mut Window, cx: &mut Context<Self>, view: Entity<LoginPage>) -> Self {
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, LoginEvent::LoggedIn(session), window, cx| {
                this.show_dashboard(session.clone(), window, cx)
            },
        );
        Self {
            page: AppPage::Login { view },
            _subscriptions: vec![subscription],
        }
    }

    fn from_dashboard(
        window: &mut Window,
        cx: &mut Context<Self>,
        view: Entity<Dashboard>,
    ) -> Self {
        let logout_subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, DashboardEvent::Logout, window, cx| this.show_login(window, cx),
        );
        Self {
            page: AppPage::Dashboard { view },
            _subscriptions: vec![logout_subscription],
        }
    }

    fn show_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| LoginPage::after_logout(window, cx));
        *self = Self::from_login(window, cx, view);
        cx.notify();
    }

    fn show_dashboard(
        &mut self,
        session: UserSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|cx| Dashboard::new(session, window, cx));
        *self = Self::from_dashboard(window, cx, view);
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let notification_layer = Root::render_notification_layer(window, cx);
        div()
            .size_full()
            .child(match &self.page {
                AppPage::Login { view, .. } => view.clone().into_any_element(),
                AppPage::Dashboard { view, .. } => view.clone().into_any_element(),
            })
            .children(notification_layer)
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    app.run(|cx| {
        gpui_component::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |w, cx| {
                w.set_window_title("Bili Live Tool");
                let view = cx.new(|cx| AppView::new(w, cx));
                cx.new(|cx| Root::new(view, w, cx).bg(cx.theme().background))
            })
            .expect("open window")
        })
        .detach()
    })
}

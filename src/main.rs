#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_event;
mod bili_api;
mod broadcast_panel;
mod dashboard;
mod login_page;
mod profile_header;
mod room_editor;
mod utils;

use gpui::*;
use gpui_component::{ActiveTheme, Root, WindowExt};

use crate::{
    app_event::NotificationEvent,
    dashboard::{Dashboard, DashboardEvent},
    login_page::{LoginEvent, LoginPage, UserSession},
};

enum AppPage {
    Login { view: Entity<LoginPage> },
    Dashboard { view: Entity<Dashboard> },
}

struct AppView {
    page: AppPage,
    subscriptions: Vec<Subscription>,
}

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = cx.new(|cx| LoginPage::new(window, cx));
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, LoginEvent::LoggedIn(session), window, cx| {
                this.show_dashboard(session.clone(), window, cx)
            },
        );
        Self {
            page: AppPage::Login { view },
            subscriptions: vec![subscription],
        }
    }

    fn show_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| LoginPage::after_logout(window, cx));
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, LoginEvent::LoggedIn(session), window, cx| {
                this.show_dashboard(session.clone(), window, cx)
            },
        );
        self.page = AppPage::Login { view };
        self.subscriptions = vec![subscription];
        cx.notify();
    }

    fn show_dashboard(
        &mut self,
        session: UserSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|cx| Dashboard::new(session, window, cx));
        let logout_subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &DashboardEvent, window, cx| {
                let DashboardEvent::Logout = event;
                this.show_login(window, cx);
            },
        );
        let notification_subscription = cx.subscribe_in(
            &view,
            window,
            |_, _, event: &NotificationEvent, window, cx| {
                window.push_notification(event.0.clone(), cx)
            },
        );
        self.page = AppPage::Dashboard { view };
        self.subscriptions = vec![logout_subscription, notification_subscription];
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(match &self.page {
            AppPage::Login { view, .. } => view.clone().into_any_element(),
            AppPage::Dashboard { view, .. } => view.clone().into_any_element(),
        })
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

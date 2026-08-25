#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_event;
mod bili_api;
mod broadcast_panel;
mod dashboard;
mod login_page;
mod profile_header;
mod room_editor;

use gpui::*;
use gpui_component::{ActiveTheme, Root, WindowExt};

use crate::{
    app_event::NotificationEvent,
    dashboard::{Dashboard, DashboardEvent},
    login_page::{LoginEvent, LoginPage, UserSession},
};

enum AppPage {
    Login {
        view: Entity<LoginPage>,
        _subscription: Subscription,
    },
    Dashboard {
        view: Entity<Dashboard>,
        _logout_subscription: Subscription,
        _notification_subscription: Subscription,
    },
}

struct AppView {
    page: AppPage,
}

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = cx.new(|cx| LoginPage::new(window, cx));
        let subscription = cx.subscribe_in(&view, window, |this, _, event, window, cx| {
            let LoginEvent::LoggedIn(session) = event;
            this.show_dashboard(session.clone(), window, cx);
        });
        Self {
            page: AppPage::Login {
                view,
                _subscription: subscription,
            },
        }
    }

    fn show_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| LoginPage::after_logout(window, cx));
        let subscription = cx.subscribe_in(&view, window, |this, _, event, window, cx| {
            let LoginEvent::LoggedIn(session) = event;
            this.show_dashboard(session.clone(), window, cx);
        });
        self.page = AppPage::Login {
            view,
            _subscription: subscription,
        };
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
        self.page = AppPage::Dashboard {
            view,
            _logout_subscription: logout_subscription,
            _notification_subscription: notification_subscription,
        };
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

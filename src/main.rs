#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

mod bili_api;
mod broadcast_panel;
mod dashboard;
mod login_page;
mod profile_header;
mod room_editor;
mod ureq_http_client;
mod utils;

use gpui::*;
use gpui_component::{ActiveTheme, Root, Theme, ThemeRegistry, ThemeSet};

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

struct AppViewStore(Entity<AppView>);

impl Global for AppViewStore {}

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
        let dialog_layer = Root::render_dialog_layer(window, cx);
        div()
            .size_full()
            .child(match &self.page {
                AppPage::Login { view, .. } => view.clone().into_any_element(),
                AppPage::Dashboard { view, .. } => view.clone().into_any_element(),
            })
            .children(notification_layer)
            .children(dialog_layer)
    }
}

const THEME_SET: &str = include_str!("../themes/ayu.json");

fn load_theme_set(cx: &mut App) -> anyhow::Result<()> {
    let theme_set: ThemeSet = serde_json::from_str(THEME_SET)?;
    ThemeRegistry::global_mut(cx).load_themes_from_str(THEME_SET)?;

    let registry = ThemeRegistry::global(cx);
    let light = theme_set
        .themes
        .iter()
        .find(|theme| !theme.mode.is_dark())
        .and_then(|theme| registry.themes().get(&theme.name))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("主题集合缺少浅色主题"))?;
    let dark = theme_set
        .themes
        .iter()
        .find(|theme| theme.mode.is_dark())
        .and_then(|theme| registry.themes().get(&theme.name))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("主题集合缺少深色主题"))?;

    let theme = Theme::global_mut(cx);
    theme.light_theme = light;
    theme.dark_theme = dark;
    Ok(())
}

fn open_main_window(cx: &mut App) -> anyhow::Result<()> {
    let window_options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(720.), px(560.)), cx)),
        ..Default::default()
    };
    cx.open_window(window_options, |window, cx| {
        window.set_window_title("Bili Live Tool");
        window.activate_window();

        Theme::sync_system_appearance(Some(window), cx);
        window
            .observe_window_appearance(|window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
            })
            .detach();
        let view = if let Some(store) = cx.try_global::<AppViewStore>() {
            store.0.clone()
        } else {
            let view = cx.new(|cx| AppView::new(window, cx));
            cx.set_global(AppViewStore(view.clone()));
            view
        };
        cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
    })?;
    Ok(())
}

fn main() {
    tracing_subscriber::fmt::init();
    let app = gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .with_http_client(Arc::new(ureq_http_client::UreqHttpClient::new()));
    app.on_reopen(|cx| {
        if cx.windows().is_empty()
            && let Err(error) = open_main_window(cx)
        {
            tracing::error!("Failed to reopen main window: {error:#}");
        }
    });
    app.run(|cx| {
        gpui_component::init(cx);
        load_theme_set(cx).expect("load bundled theme set");
        open_main_window(cx).expect("open main window");
    })
}

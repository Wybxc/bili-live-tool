use std::{io::Cursor, sync::Arc, time::Duration};

use gpui::*;
use gpui_component::{ActiveTheme, IconName, Sizable, StyledExt, button::Button, spinner::Spinner};

use crate::{
    bili_api,
    utils::{weak_emit, weak_read, weak_update},
};

#[derive(Clone)]
pub struct UserSession {
    pub(crate) name: SharedString,
    pub(crate) user_id: u64,
    pub(crate) face_url: String,
}

impl TryFrom<bili_api::NavUserInfo> for UserSession {
    type Error = anyhow::Error;

    fn try_from(info: bili_api::NavUserInfo) -> Result<Self, Self::Error> {
        if !info.is_login || info.mid == 0 || info.uname.is_empty() {
            return Err(anyhow::anyhow!("登录会话无效"));
        }
        Ok(Self {
            name: info.uname.as_str().into(),
            user_id: info.mid,
            face_url: info.face.to_string(),
        })
    }
}

pub enum LoginEvent {
    LoggedIn(UserSession),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QrStatus {
    Waiting,
    Confirming,
    Expired,
}

enum LoginState {
    RestoringSession,
    LoadingQr,
    Polling { qr: Arc<Image>, status: QrStatus },
    Failed { message: SharedString },
}

impl LoginState {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            LoginState::Polling {
                status: QrStatus::Waiting | QrStatus::Confirming,
                ..
            }
        )
    }
}

pub struct LoginPage {
    state: LoginState,
    login_task: Task<()>,
}

impl EventEmitter<LoginEvent> for LoginPage {}

impl LoginPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let login_task = cx.spawn_in(window, async move |weak, cx| {
            if let Ok(info) = cx
                .background_spawn(async { bili_api::get_nav_user_info() })
                .await
                && info.is_login
                && let Ok(session) = UserSession::try_from(info)
            {
                weak_emit(cx, &weak, LoginEvent::LoggedIn(session));
            } else {
                Self::run_qr_login(weak, cx).await;
            }
        });
        Self {
            state: LoginState::RestoringSession,
            login_task,
        }
    }

    pub fn after_logout(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = bili_api::clear_cookies();
        Self {
            state: LoginState::LoadingQr,
            login_task: cx.spawn_in(window, Self::run_qr_login),
        }
    }

    fn refresh_qr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.login_task = cx.spawn_in(window, Self::run_qr_login);
    }

    async fn run_qr_login(weak: WeakEntity<Self>, cx: &mut AsyncWindowContext) {
        weak_update(cx, &weak, |this, _| this.state = LoginState::LoadingQr);

        let response = match cx
            .background_spawn(async { bili_api::generate_passport_qrcode() })
            .await
        {
            Ok(response) => response,
            Err(err) => {
                weak_update(cx, &weak, |this, _| {
                    this.set_failed(format!("二维码加载失败：{err}"))
                });
                return;
            }
        };
        let qr = match encode_qr(response.url.as_str()) {
            Ok(qr) => qr,
            Err(err) => {
                weak_update(cx, &weak, |this, _| {
                    this.set_failed(format!("二维码生成失败：{err}"))
                });
                return;
            }
        };

        let key = response.qrcode_key.clone();
        weak_update(cx, &weak, |this, _| {
            this.state = LoginState::Polling {
                qr,
                status: QrStatus::Waiting,
            }
        });
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;

            if weak_read(cx, &weak, |this| this.state.is_active()).ok() != Some(true) {
                return;
            }

            let poll_key = key.clone();
            let Ok(result) = cx
                .background_spawn(async move { bili_api::poll_passport_qrcode_status(&poll_key) })
                .await
            else {
                continue;
            };

            if result.code == bili_api::PollPassportQrcodeStatusCode::Success {
                let info = match cx
                    .background_spawn(async { bili_api::get_nav_user_info() })
                    .await
                {
                    Ok(info) => info,
                    Err(err) => {
                        weak_update(cx, &weak, |this, _| {
                            this.set_failed(format!("登录会话获取失败，请刷新二维码：{err}"))
                        });
                        return;
                    }
                };

                let Ok(session) = UserSession::try_from(info) else {
                    weak_update(cx, &weak, |this, _| {
                        this.set_failed("登录会话无效，请刷新二维码")
                    });
                    return;
                };

                let _ = bili_api::save_cookies();
                weak_emit(cx, &weak, LoginEvent::LoggedIn(session));
                return;
            }

            let status = match result.code {
                bili_api::PollPassportQrcodeStatusCode::Expired => QrStatus::Expired,
                bili_api::PollPassportQrcodeStatusCode::Confirming => QrStatus::Confirming,
                _ => QrStatus::Waiting,
            };
            weak_update(cx, &weak, |this, _| {
                if let LoginState::Polling {
                    status: current_status,
                    ..
                } = &mut this.state
                {
                    *current_status = status;
                }
            });
        }
    }

    fn set_failed(&mut self, message: impl Into<SharedString>) {
        self.state = LoginState::Failed {
            message: message.into(),
        };
    }
}

impl Render for LoginPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, qr) = match &self.state {
            LoginState::RestoringSession => (
                "正在恢复登录状态",
                Spinner::new().large().into_any_element(),
            ),
            LoginState::LoadingQr => (
                "正在加载登录二维码",
                Spinner::new().large().into_any_element(),
            ),
            LoginState::Polling { qr, status, .. } => {
                let text = match status {
                    QrStatus::Waiting => "请使用 B 站 APP 扫码登录",
                    QrStatus::Confirming => "请在手机上确认登录",
                    QrStatus::Expired => "二维码已过期，请刷新",
                };
                (text, img(qr.clone()).size_72().into_any_element())
            }
            LoginState::Failed { message } => (
                "二维码加载失败",
                div()
                    .text_center()
                    .child(text!(message.clone()))
                    .into_any_element(),
            ),
        };
        div()
            .v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_5()
            .child(div().text_lg().font_semibold().child(text))
            .child(
                div()
                    .size_72()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .child(qr),
            )
            .child(
                Button::new("refresh")
                    .icon(IconName::LoaderCircle)
                    .label("刷新二维码")
                    .on_click(cx.listener(|this, _, window, cx| this.refresh_qr(window, cx))),
            )
    }
}

fn encode_qr(text: &str) -> anyhow::Result<Arc<Image>> {
    let image = qrcode::QrCode::new(text)?
        .render::<image::Luma<u8>>()
        .build();
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
}

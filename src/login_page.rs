use std::{io::Cursor, sync::Arc, time::Duration};

use async_compat::Compat;
use gpui::*;
use gpui_component::{ActiveTheme, IconName, Sizable, StyledExt, button::Button, spinner::Spinner};

use crate::bili_api;

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
    RestoringSession {
        generation: u64,
    },
    LoadingQr {
        generation: u64,
    },
    Polling {
        generation: u64,
        qr: Arc<Image>,
        status: QrStatus,
    },
    Failed {
        generation: u64,
        message: SharedString,
    },
}

impl LoginState {
    fn generation(&self) -> u64 {
        match self {
            LoginState::RestoringSession { generation }
            | LoginState::LoadingQr { generation }
            | LoginState::Polling { generation, .. }
            | LoginState::Failed { generation, .. } => *generation,
        }
    }
}

pub struct LoginPage {
    state: LoginState,
}

impl EventEmitter<LoginEvent> for LoginPage {}

impl LoginPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            state: LoginState::RestoringSession { generation: 0 },
        };
        this.restore_or_start(window, cx);
        this
    }

    fn restore_or_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.state.generation();
        cx.spawn_in(window, async move |weak, cx| {
            match Compat::new(bili_api::get_nav_user_info()).await {
                Ok(info) if info.is_login => {
                    let Ok(session) = UserSession::try_from(info) else {
                        let _ = cx.update(|window, cx| {
                            weak.update(cx, |this, cx| {
                                if this.is_current(generation) {
                                    this.start(window, cx);
                                }
                            })
                        });
                        return;
                    };
                    let _ = cx.update(|_, cx| {
                        weak.update(cx, |this, cx| {
                            let LoginState::RestoringSession {
                                generation: current,
                            } = &this.state
                            else {
                                return;
                            };
                            if *current != generation {
                                return;
                            }
                            cx.emit(LoginEvent::LoggedIn(session));
                        })
                    });
                }
                _ => {
                    let _ = cx.update(|window, cx| {
                        weak.update(cx, |this, cx| {
                            if matches!(
                                this.state,
                                LoginState::RestoringSession {
                                    generation: current
                                } if current == generation
                            ) {
                                this.start(window, cx);
                            }
                        })
                    });
                }
            }
        })
        .detach();
    }

    pub fn after_logout(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            state: LoginState::RestoringSession { generation: 0 },
        };
        let _ = bili_api::clear_cookies();
        this.start(window, cx);
        this
    }

    fn is_current(&self, generation: u64) -> bool {
        self.state.generation() == generation
    }

    fn fail(&mut self, generation: u64, message: impl Into<SharedString>) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.state = LoginState::Failed {
            generation,
            message: message.into(),
        };
        true
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.state.generation().wrapping_add(1);
        self.state = LoginState::LoadingQr { generation };
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let response = Compat::new(bili_api::generate_passport_qrcode()).await;
            let Ok(response) = response else {
                let message = format!("二维码加载失败：{}", response.unwrap_err());
                let _ = cx.update(|_, cx| {
                    weak.update(cx, |this, cx| {
                        if this.fail(generation, message) {
                            cx.notify();
                        }
                    })
                });
                return;
            };
            let qr = encode_qr(response.url.as_str());
            let Ok(qr) = qr else {
                let message = format!("二维码生成失败：{}", qr.unwrap_err());
                let _ = cx.update(|_, cx| {
                    weak.update(cx, |this, cx| {
                        if this.fail(generation, message) {
                            cx.notify();
                        }
                    })
                });
                return;
            };
            let key = response.qrcode_key.to_string();
            let _ = cx.update(|_, cx| {
                weak.update(cx, |this, cx| {
                    if this.is_current(generation) {
                        this.state = LoginState::Polling {
                            generation,
                            qr,
                            status: QrStatus::Waiting,
                        };
                        cx.notify();
                    }
                })
            });
            loop {
                Compat::new(tokio::time::sleep(Duration::from_millis(1500))).await;
                let active = cx
                    .update(|_, cx| {
                        weak.upgrade().is_some_and(|entity| {
                            let this = entity.read(cx);
                            matches!(
                                this.state,
                                LoginState::Polling {
                                    generation: current,
                                    status: QrStatus::Waiting | QrStatus::Confirming,
                                    ..
                                } if current == generation
                            )
                        })
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
                let Ok(result) = Compat::new(bili_api::poll_passport_qrcode_status(&key)).await
                else {
                    continue;
                };
                if result.code == bili_api::PollPassportQrcodeStatusCode::Success {
                    let Ok(info) = Compat::new(bili_api::get_nav_user_info()).await else {
                        break;
                    };
                    let Ok(session) = UserSession::try_from(info) else {
                        let _ = cx.update(|_, cx| {
                            weak.update(cx, |this, cx| {
                                if this.fail(generation, "登录会话无效，请刷新二维码")
                                {
                                    cx.notify();
                                }
                            })
                        });
                        break;
                    };
                    let _ = cx.update(|_, cx| {
                        weak.update(cx, |this, cx| {
                            if !this.is_current(generation) {
                                return;
                            }
                            let _ = bili_api::save_cookies();
                            cx.emit(LoginEvent::LoggedIn(session));
                        })
                    });
                    break;
                }
                let status = match result.code {
                    bili_api::PollPassportQrcodeStatusCode::Expired => QrStatus::Expired,
                    bili_api::PollPassportQrcodeStatusCode::Confirming => QrStatus::Confirming,
                    _ => QrStatus::Waiting,
                };
                let _ = cx.update(|_, cx| {
                    weak.update(cx, |this, cx| {
                        let LoginState::Polling {
                            generation: current,
                            status: current_status,
                            ..
                        } = &mut this.state
                        else {
                            return;
                        };
                        if *current != generation {
                            return;
                        }
                        *current_status = status;
                        cx.notify();
                    })
                });
            }
        })
        .detach();
    }
}

impl Render for LoginPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, qr) = match &self.state {
            LoginState::RestoringSession { .. } => (
                "正在恢复登录状态",
                Spinner::new().large().into_any_element(),
            ),
            LoginState::LoadingQr { .. } => (
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
            LoginState::Failed { message, .. } => (
                "二维码加载失败",
                div()
                    .text_center()
                    .child(message.clone())
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
                    .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
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

#[cfg(test)]
mod tests {
    use super::{LoginPage, LoginState, UserSession};
    use crate::bili_api;

    #[test]
    fn rejects_invalid_user_session() {
        assert!(
            UserSession::try_from(bili_api::NavUserInfo {
                is_login: true,
                mid: 0,
                uname: "user".into(),
                face: "face".into(),
            })
            .is_err()
        );
    }

    #[test]
    fn stale_generation_cannot_change_login_state() {
        let mut page = LoginPage {
            state: LoginState::LoadingQr { generation: 2 },
        };
        assert!(!page.fail(1, "stale"));
        assert!(matches!(
            page.state,
            LoginState::LoadingQr { generation: 2 }
        ));
    }
}

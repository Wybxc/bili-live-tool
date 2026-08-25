use std::sync::Arc;

use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, StyledExt, WindowExt,
    button::{Button, ButtonVariants},
    dialog::{DialogContent, DialogFooter},
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};

use crate::{
    bili_api::{self, StreamProtocol},
    room_editor::{RoomEditor, RoomEditorEvent},
    utils::{clipboard_copy, encode_qr, weak_update_in},
};

#[derive(Clone)]
struct ProtocolSet {
    items: Box<[StreamProtocol]>,
    active: usize,
}

impl ProtocolSet {
    fn new(protocols: Vec<StreamProtocol>) -> Option<Self> {
        (!protocols.is_empty()).then(|| Self {
            items: protocols.into_boxed_slice(),
            active: 0,
        })
    }

    fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.active = index;
        }
    }

    fn active(&self) -> &StreamProtocol {
        &self.items[self.active]
    }
}

#[derive(Default)]
enum BroadcastState {
    #[default]
    Offline,
    Starting,
    AwaitingFaceVerification,
    LiveWithoutCredentials,
    Living(ProtocolSet),
    Stopping {
        protocols: Option<ProtocolSet>,
    },
}

pub struct BroadcastPanel {
    user_id: u64,
    editor: Entity<RoomEditor>,
    state: BroadcastState,
    subscriptions: Vec<Subscription>,
}

#[derive(IntoElement)]
struct FaceVerificationContent {
    qr: Arc<Image>,
}

impl RenderOnce for FaceVerificationContent {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        DialogContent::new()
            .v_flex()
            .items_center()
            .gap_4()
            .child("请使用 B 站 APP 扫码完成人脸验证")
            .child(img(self.qr).size_64())
    }
}

#[derive(IntoElement)]
struct FaceVerificationActions {
    panel: WeakEntity<BroadcastPanel>,
}

impl RenderOnce for FaceVerificationActions {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        DialogFooter::new().child(
            Button::new("verified")
                .primary()
                .label("我已完成验证")
                .on_click(move |_, window, cx| {
                    window.close_dialog(cx);
                    let _ = self.panel.update(cx, |this, cx| this.start(window, cx));
                }),
        )
    }
}

#[derive(IntoElement)]
struct BroadcastSetup {
    editor: Entity<RoomEditor>,
    panel: WeakEntity<BroadcastPanel>,
    starting: bool,
    awaiting_verification: bool,
}

impl RenderOnce for BroadcastSetup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        v_flex().gap_5().child(self.editor).child(
            Button::new("start")
                .primary()
                .icon(IconName::Play)
                .label(if self.awaiting_verification {
                    "重新验证"
                } else {
                    "开始直播"
                })
                .loading(self.starting)
                .on_click(move |_, window, cx| {
                    let _ = self.panel.update(cx, |this, cx| this.start(window, cx));
                }),
        )
    }
}

#[derive(IntoElement)]
struct ProtocolLine {
    id: &'static str,
    label: &'static str,
    value: SharedString,
}

impl RenderOnce for ProtocolLine {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = self.value;
        h_flex()
            .gap_3()
            .child(
                div()
                    .w_16()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(value.clone()),
            )
            .child(
                Button::new(self.id)
                    .ghost()
                    .icon(IconName::Copy)
                    .tooltip("复制")
                    .on_click(move |_, window, cx| {
                        window.push_notification(clipboard_copy(&*value), cx);
                    }),
            )
    }
}

#[derive(IntoElement)]
struct ProtocolPanel {
    protocols: Option<ProtocolSet>,
    panel: WeakEntity<BroadcastPanel>,
    stopping: bool,
}

impl RenderOnce for ProtocolPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let details = if let Some(protocols) = self.protocols {
            let panel = self.panel.clone();

            let protocol = protocols.active();
            v_flex()
                .child(
                    TabBar::new("protocols")
                        .underline()
                        .selected_index(protocols.active)
                        .on_click(move |index: &usize, _, cx| {
                            let _ = panel.update(cx, |this, cx| {
                                this.select_protocol(*index);
                                cx.notify();
                            });
                        })
                        .children(
                            protocols
                                .items
                                .iter()
                                .map(|protocol| Tab::new().label(protocol.name.to_string())),
                        ),
                )
                .child(
                    v_flex()
                        .gap_4()
                        .p_4()
                        .child(ProtocolLine {
                            id: "copy-address",
                            label: "服务器",
                            value: protocol.addr.as_str().into(),
                        })
                        .child(ProtocolLine {
                            id: "copy-code",
                            label: "推流码",
                            value: protocol.code.as_str().into(),
                        }),
                )
                .into_any_element()
        } else {
            div()
                .p_4()
                .text_color(cx.theme().muted_foreground)
                .child("当前直播不是由本次会话启动，无法恢复推流码")
                .into_any_element()
        };
        let panel = self.panel;
        v_flex().gap_5().child(details).child(
            Button::new("stop")
                .danger()
                .icon(IconName::Close)
                .label("结束直播")
                .loading(self.stopping)
                .on_click(move |_, window, cx| {
                    let _ = panel.update(cx, |this, cx| this.stop(window, cx));
                }),
        )
    }
}

impl BroadcastPanel {
    pub fn new(user_id: u64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| RoomEditor::new(user_id, window, cx));
        let mut this = Self {
            user_id,
            editor: editor.clone(),
            state: BroadcastState::Offline,
            subscriptions: Vec::new(),
        };
        this.subscriptions.push(cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &RoomEditorEvent, _, cx| {
                let RoomEditorEvent::LiveRestored = event;
                if matches!(this.state, BroadcastState::Offline) {
                    this.state = BroadcastState::LiveWithoutCredentials;
                    cx.notify();
                }
            },
        ));
        this
    }

    fn set_editor_locked(&self, locked: bool, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_broadcast_locked(locked, cx));
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(
            self.state,
            BroadcastState::Offline | BroadcastState::AwaitingFaceVerification
        ) {
            return;
        }
        let request = match self.editor.read(cx).start_request(cx) {
            Ok(request) => request,
            Err(error) => {
                window.push_notification(error.to_string(), cx);
                return;
            }
        };
        let user_id = self.user_id;
        self.state = BroadcastState::Starting;
        self.set_editor_locked(true, cx);
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_spawn(async move {
                    bili_api::start_live_session(
                        request.room_id,
                        user_id,
                        &request.title,
                        request.area_id,
                    )
                })
                .await;
            weak_update_in(cx, &weak, |this, window, cx| match result {
                Ok(bili_api::StartLiveOutcome::Started(protocols)) => {
                    let Some(protocols) = ProtocolSet::new(protocols) else {
                        this.state = BroadcastState::Offline;
                        this.set_editor_locked(false, cx);
                        window.push_notification("开播失败：服务端未返回有效推流协议", cx);
                        return;
                    };
                    this.state = BroadcastState::Living(protocols);
                    this.set_editor_locked(false, cx);
                    window.push_notification("开播成功", cx);
                }
                Ok(bili_api::StartLiveOutcome::FaceVerification(url)) => match encode_qr(&url) {
                    Ok(qr) => {
                        this.state = BroadcastState::AwaitingFaceVerification;
                        this.set_editor_locked(false, cx);
                        let panel = cx.entity().downgrade();
                        window.open_dialog(cx, move |dialog, _, _| {
                            dialog
                                .title("人脸验证")
                                .child(FaceVerificationContent { qr: qr.clone() })
                                .footer(FaceVerificationActions {
                                    panel: panel.clone(),
                                })
                        });
                    }
                    Err(error) => {
                        this.state = BroadcastState::Offline;
                        this.set_editor_locked(false, cx);
                        window.push_notification(format!("二维码生成失败：{error}"), cx);
                    }
                },
                Err(error) => {
                    this.state = BroadcastState::Offline;
                    this.set_editor_locked(false, cx);
                    window.push_notification(format!("开播失败：{error}"), cx);
                }
            });
        })
        .detach();
    }

    fn stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let room_id = match self.editor.read(cx).room_id() {
            Ok(room_id) => room_id,
            Err(error) => {
                window.push_notification(error.to_string(), cx);
                return;
            }
        };
        let current = std::mem::take(&mut self.state);
        let protocols = match current {
            BroadcastState::Living(protocols) => Some(protocols),
            BroadcastState::LiveWithoutCredentials => None,
            other => {
                self.state = other;
                return;
            }
        };
        self.state = BroadcastState::Stopping { protocols };
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_spawn(async move { bili_api::stop_live_session(room_id) })
                .await;
            weak_update_in(cx, &weak, |this, window, cx| {
                if result.is_ok() {
                    this.state = BroadcastState::Offline;
                } else {
                    let state = std::mem::take(&mut this.state);
                    this.state = match state {
                        BroadcastState::Stopping {
                            protocols: Some(protocols),
                        } => BroadcastState::Living(protocols),
                        BroadcastState::Stopping { protocols: None } => {
                            BroadcastState::LiveWithoutCredentials
                        }
                        state => state,
                    };
                }
                let message: SharedString = match result {
                    Ok(_) => "下播成功".into(),
                    Err(error) => format!("下播失败：{error}").into(),
                };
                window.push_notification(message, cx);
            });
        })
        .detach();
    }

    fn select_protocol(&mut self, index: usize) {
        match &mut self.state {
            BroadcastState::Living(protocols)
            | BroadcastState::Stopping {
                protocols: Some(protocols),
            } => protocols.select(index),
            _ => {}
        }
    }
}

impl Render for BroadcastPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let room = self.editor.read(cx).room_label();
        let panel = cx.entity().downgrade();
        let content = match &self.state {
            BroadcastState::Offline
            | BroadcastState::Starting
            | BroadcastState::AwaitingFaceVerification => BroadcastSetup {
                editor: self.editor.clone(),
                panel,
                starting: matches!(self.state, BroadcastState::Starting),
                awaiting_verification: matches!(
                    self.state,
                    BroadcastState::AwaitingFaceVerification
                ),
            }
            .into_any_element(),
            BroadcastState::LiveWithoutCredentials => ProtocolPanel {
                protocols: None,
                panel,
                stopping: false,
            }
            .into_any_element(),
            BroadcastState::Living(protocols) => ProtocolPanel {
                protocols: Some(protocols.clone()),
                panel,
                stopping: false,
            }
            .into_any_element(),
            BroadcastState::Stopping { protocols } => ProtocolPanel {
                protocols: protocols.clone(),
                panel,
                stopping: true,
            }
            .into_any_element(),
        };
        v_flex()
            .gap_5()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("房间号：{room}")),
            )
            .child(content)
    }
}

use std::sync::Arc;

use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IndexPath, WindowExt,
    button::Button,
    h_flex,
    input::{Input, InputState},
    select::{Select, SelectEvent, SelectState},
    v_flex,
};

use crate::{
    bili_api::{self, Area},
    utils::{weak_update, weak_update_in},
};

#[derive(Clone)]
pub struct StartLiveRequest {
    pub room_id: u64,
    pub title: String,
    pub area_id: u64,
}

pub enum RoomEditorEvent {
    LiveRestored,
}

#[derive(Default)]
enum RoomState {
    #[default]
    Loading,
    LoadingDetails,
    Ready(u64),
    Failed,
}

#[derive(Clone, Copy)]
struct AreaSelection {
    parent_id: u64,
    area_id: u64,
}

#[derive(Default)]
enum AreaState {
    #[default]
    Loading,
    Ready {
        areas: Arc<Vec<Area>>,
        selection: Option<AreaSelection>,
    },
    Failed,
}

#[derive(Default, PartialEq, Eq)]
enum UpdateState {
    #[default]
    Idle,
    Updating,
}

pub struct RoomEditor {
    room: RoomState,
    areas: AreaState,
    title: Entity<InputState>,
    area: Entity<SelectState<Vec<SharedString>>>,
    sub: Entity<SelectState<Vec<SharedString>>>,
    update: UpdateState,
    broadcast_locked: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RoomEditorEvent> for RoomEditor {}

impl RoomEditor {
    pub fn new(user_id: u64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("请输入直播标题"));
        let area = cx.new(|cx| SelectState::new(vec![], None, window, cx));
        let sub = cx.new(|cx| SelectState::new(vec![], None, window, cx));
        let mut this = Self {
            room: RoomState::Loading,
            areas: AreaState::Loading,
            title,
            area: area.clone(),
            sub: sub.clone(),
            update: UpdateState::Idle,
            broadcast_locked: false,
            _subscriptions: Vec::new(),
        };
        this._subscriptions.push(
            cx.subscribe_in(&area, window, |this, _, event, window, cx| {
                let SelectEvent::Confirm(value) = event;
                this.select_parent(value.as_ref(), window, cx);
            }),
        );
        this._subscriptions
            .push(cx.subscribe_in(&sub, window, |this, _, event, _, cx| {
                let SelectEvent::Confirm(value) = event;
                this.select_sub(value.as_ref());
                cx.notify();
            }));
        this.initialize(user_id, window, cx);
        this
    }

    pub fn start_request(&self, cx: &App) -> anyhow::Result<StartLiveRequest> {
        if self.update == UpdateState::Updating || self.broadcast_locked {
            return Err(anyhow::anyhow!("房间信息正在更新"));
        }
        Ok(StartLiveRequest {
            room_id: self.room_id()?,
            title: self.title.read(cx).value().to_string(),
            area_id: self.area_id()?,
        })
    }

    pub fn set_broadcast_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.broadcast_locked = locked;
        cx.notify();
    }

    pub fn room_label(&self) -> SharedString {
        match self.room {
            RoomState::Loading | RoomState::LoadingDetails => "获取中...".into(),
            RoomState::Ready(room_id) => room_id.to_string().into(),
            RoomState::Failed => "获取失败".into(),
        }
    }

    fn initialize(&mut self, user_id: u64, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |weak, cx| {
            match cx
                .background_spawn(async { bili_api::get_live_area_list() })
                .await
                .map(Arc::new)
            {
                Ok(areas) => {
                    weak_update_in(cx, &weak, |this, window, cx| {
                        this.install_areas(areas, window, cx)
                    });
                }
                Err(error) => {
                    weak_update_in(cx, &weak, |this, window, cx| {
                        this.areas = AreaState::Failed;
                        this.notify(format!("分区列表加载失败：{error}"), window, cx);
                    });
                }
            }
            Self::load_room(weak, user_id, cx).await;
        })
        .detach();
    }

    async fn load_room(weak: WeakEntity<Self>, user_id: u64, cx: &mut AsyncWindowContext) {
        let room = cx
            .background_spawn(async move { bili_api::get_room_id(user_id) })
            .await;
        let Ok(room) = room else {
            weak_update_in(cx, &weak, |this, window, cx| {
                this.room = RoomState::Failed;
                this.notify(format!("房间号获取失败：{}", room.unwrap_err()), window, cx);
            });
            return;
        };
        let room_id = room.room_id;
        weak_update(cx, &weak, |this, _| this.room = RoomState::LoadingDetails);
        let info = cx
            .background_spawn(async move { bili_api::get_room_info(room_id) })
            .await;
        let Ok(info) = info else {
            weak_update_in(cx, &weak, |this, window, cx| {
                this.room = RoomState::Failed;
                this.notify(
                    format!("房间信息获取失败：{}", info.unwrap_err()),
                    window,
                    cx,
                );
            });
            return;
        };
        let title = info.title.to_string();
        weak_update_in(cx, &weak, |this, window, cx| {
            this.room = RoomState::Ready(room_id);
            this.title
                .update(cx, |state, cx| state.set_value(title, window, cx));
            this.select_area(info.parent_area_id, info.area_id, window, cx);
            if info.live_status == bili_api::LiveStatus::Living {
                cx.emit(RoomEditorEvent::LiveRestored);
            }
        });
    }

    fn notify(
        &self,
        message: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message: SharedString = message.into();
        window.push_notification(message, cx);
    }

    pub fn room_id(&self) -> anyhow::Result<u64> {
        match self.room {
            RoomState::Ready(room_id) => Ok(room_id),
            RoomState::Loading | RoomState::LoadingDetails | RoomState::Failed => {
                Err(anyhow::anyhow!("房间信息尚未就绪"))
            }
        }
    }

    fn area_id(&self) -> anyhow::Result<u64> {
        match &self.areas {
            AreaState::Ready {
                selection: Some(selection),
                ..
            } => Ok(selection.area_id),
            AreaState::Loading => Err(anyhow::anyhow!("分区列表尚未就绪")),
            AreaState::Ready {
                selection: None, ..
            }
            | AreaState::Failed => Err(anyhow::anyhow!("请选择分区")),
        }
    }

    fn install_areas(
        &mut self,
        areas: Arc<Vec<Area>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = areas.first().and_then(|parent| {
            parent.list.first().map(|area| AreaSelection {
                parent_id: parent.id,
                area_id: area.id,
            })
        });
        let parent_names = areas.iter().map(|area| area.name.as_str().into()).collect();
        let sub_names = areas
            .first()
            .map(|parent| {
                parent
                    .list
                    .iter()
                    .map(|area| area.name.as_str().into())
                    .collect()
            })
            .unwrap_or_default();
        self.areas = AreaState::Ready { areas, selection };
        self.area.update(cx, |state, cx| {
            state.set_items(parent_names, window, cx);
            state.set_selected_index(selection.map(|_| IndexPath::default().row(0)), window, cx);
        });
        self.sub.update(cx, |state, cx| {
            state.set_items(sub_names, window, cx);
            state.set_selected_index(selection.map(|_| IndexPath::default().row(0)), window, cx);
        });
    }

    fn select_parent(
        &mut self,
        name: Option<&SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AreaState::Ready { areas, selection } = &mut self.areas else {
            return;
        };
        let parent = name.and_then(|name| {
            areas
                .iter()
                .find(|area| area.name.as_str() == name.as_ref())
        });
        *selection = parent.and_then(|parent| {
            parent.list.first().map(|area| AreaSelection {
                parent_id: parent.id,
                area_id: area.id,
            })
        });
        let names = parent
            .map(|parent| {
                parent
                    .list
                    .iter()
                    .map(|area| area.name.as_str().into())
                    .collect()
            })
            .unwrap_or_default();
        self.sub.update(cx, |state, cx| {
            state.set_items(names, window, cx);
            state.set_selected_index(selection.map(|_| IndexPath::default().row(0)), window, cx);
        });
        cx.notify();
    }

    fn select_sub(&mut self, name: Option<&SharedString>) {
        let AreaState::Ready { areas, selection } = &mut self.areas else {
            return;
        };
        let Some(parent_id) = selection.map(|selection| selection.parent_id) else {
            return;
        };
        let area_id = areas
            .iter()
            .find(|area| area.id == parent_id)
            .and_then(|parent| {
                name.and_then(|name| {
                    parent
                        .list
                        .iter()
                        .find(|area| area.name.as_str() == name.as_ref())
                })
            })
            .map(|area| area.id);
        *selection = area_id.map(|area_id| AreaSelection { parent_id, area_id });
    }

    fn select_area(
        &mut self,
        parent_id: u64,
        area_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AreaState::Ready { areas, selection } = &mut self.areas else {
            return;
        };
        let Some((parent_index, parent)) = areas
            .iter()
            .enumerate()
            .find(|(_, area)| area.id == parent_id)
        else {
            return;
        };
        let Some(area_index) = parent.list.iter().position(|area| area.id == area_id) else {
            return;
        };
        *selection = Some(AreaSelection { parent_id, area_id });
        let names = parent
            .list
            .iter()
            .map(|area| area.name.as_str().into())
            .collect();
        self.area.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::default().row(parent_index)), window, cx)
        });
        self.sub.update(cx, |state, cx| {
            state.set_items(names, window, cx);
            state.set_selected_index(Some(IndexPath::default().row(area_index)), window, cx);
        });
    }

    fn update_room(
        &mut self,
        title: bool,
        area: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.update == UpdateState::Updating || self.broadcast_locked {
            return;
        }
        let Ok(room_id) = self.room_id() else {
            self.notify("房间信息尚未就绪", window, cx);
            return;
        };
        let title_value = self.title.read(cx).value().to_string();
        let area_id = if area {
            let Ok(area_id) = self.area_id() else {
                self.notify("分区列表尚未就绪", window, cx);
                return;
            };
            Some(area_id)
        } else {
            None
        };
        self.update = UpdateState::Updating;
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx
                .background_spawn(async move {
                    bili_api::update_live_room(
                        room_id,
                        title.then_some(title_value.as_str()),
                        area_id,
                    )
                })
                .await;
            weak_update_in(cx, &weak, |this, window, cx| {
                this.update = UpdateState::Idle;
                let message: SharedString = match result {
                    Ok(_) => "更新成功".into(),
                    Err(error) => format!("更新失败：{error}").into(),
                };
                this.notify(message, window, cx);
            });
        })
        .detach();
    }
}

impl Render for RoomEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let room_ready = matches!(self.room, RoomState::Ready(_));
        let area_ready = matches!(
            self.areas,
            AreaState::Ready {
                selection: Some(_),
                ..
            }
        );
        let updating = self.update == UpdateState::Updating;
        v_flex()
            .gap_5()
            .child(
                h_flex()
                    .gap_3()
                    .child(div().w_12().child("标题"))
                    .child(Input::new(&self.title).flex_1())
                    .child(
                        Button::new("update-title")
                            .label("更新")
                            .disabled(updating || self.broadcast_locked || !room_ready)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.update_room(true, false, window, cx)
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(div().w_12().child("分区"))
                    .child(Select::new(&self.area).flex_1().disabled(!area_ready))
                    .child(Select::new(&self.sub).flex_1().disabled(!area_ready))
                    .child(
                        Button::new("update-area")
                            .label("更新")
                            .disabled(
                                updating || self.broadcast_locked || !room_ready || !area_ready,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.update_room(false, true, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("开始直播时自动更新标题和分区"),
            )
    }
}

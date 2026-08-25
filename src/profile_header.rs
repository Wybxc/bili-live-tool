use std::sync::Arc;

use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, ThemeMode,
    avatar::Avatar,
    button::{Button, ButtonVariants},
    h_flex,
};

use crate::{login_page::UserSession, utils::weak_update};

pub enum ProfileHeaderEvent {
    Logout,
}

enum AvatarState {
    Loading,
    Ready(Arc<Image>),
    Failed,
}

pub struct ProfileHeader {
    name: SharedString,
    avatar: AvatarState,
}

impl EventEmitter<ProfileHeaderEvent> for ProfileHeader {}

impl ProfileHeader {
    pub fn new(session: &UserSession, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let this = Self {
            name: session.name.clone(),
            avatar: AvatarState::Loading,
        };
        let face_url = session.face_url.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let avatar = match cx
                .background_spawn(async move { load_avatar(&face_url) })
                .await
            {
                Ok(image) => AvatarState::Ready(image),
                Err(_) => AvatarState::Failed,
            };
            weak_update(cx, &weak, |this, _| this.avatar = avatar);
        })
        .detach();
        this
    }
}

impl Render for ProfileHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let avatar = match &self.avatar {
            AvatarState::Ready(image) => Avatar::new().src(image.clone()).large(),
            AvatarState::Loading | AvatarState::Failed => {
                Avatar::new().name(self.name.clone()).large()
            }
        };
        h_flex()
            .gap_3()
            .child(avatar)
            .child(div().flex_1().child(self.name.clone()))
            .child(
                Button::new("logout")
                    .ghost()
                    .icon(IconName::Close)
                    .label("退出登录")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ProfileHeaderEvent::Logout))),
            )
            .child(
                Button::new("theme")
                    .ghost()
                    .icon(IconName::Sun)
                    .tooltip("切换主题")
                    .on_click(|_, _, cx| {
                        let next = if cx.theme().mode.is_dark() {
                            ThemeMode::Light
                        } else {
                            ThemeMode::Dark
                        };
                        gpui_component::Theme::change(next, None, cx)
                    }),
            )
    }
}

fn load_avatar(url: &str) -> anyhow::Result<Arc<Image>> {
    let mut response = ureq::get(url).call()?;
    let bytes = response.body_mut().read_to_vec()?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
}

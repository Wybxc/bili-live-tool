use gpui::*;
use gpui_component::{
    IconName, Sizable,
    avatar::Avatar,
    button::{Button, ButtonVariants},
    h_flex,
};

type LogoutHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ProfileHeader {
    name: SharedString,
    avatar: ImageSource,
    on_logout: LogoutHandler,
}

impl ProfileHeader {
    pub fn new(
        name: SharedString,
        avatar: ImageSource,
        on_logout: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            name,
            avatar,
            on_logout: Box::new(on_logout),
        }
    }
}

impl RenderOnce for ProfileHeader {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let avatar = Avatar::new()
            .src(self.avatar.clone())
            .name(self.name.clone())
            .large();
        let on_logout = self.on_logout;
        h_flex()
            .gap_3()
            .child(avatar)
            .child(div().flex_1().child(self.name.clone()))
            .child(
                Button::new("logout")
                    .ghost()
                    .icon(IconName::Close)
                    .label("退出登录")
                    .on_click(on_logout),
            )
    }
}

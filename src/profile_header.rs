use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, ThemeMode,
    avatar::Avatar,
    button::{Button, ButtonVariants},
    h_flex,
};

#[derive(IntoElement)]
pub struct ProfileHeader {
    name: SharedString,
    avatar: ImageSource,
    on_logout: Box<dyn Fn(&mut Window, &mut App)>,
}

impl ProfileHeader {
    pub fn new(
        name: SharedString,
        avatar: ImageSource,
        on_logout: impl Fn(&mut Window, &mut App) + 'static,
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
                    .on_click(move |_, window, cx| on_logout(window, cx)),
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

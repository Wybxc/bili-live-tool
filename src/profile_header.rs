use gpui::*;
use gpui_component::{
    IconName, Sizable,
    avatar::Avatar,
    button::{Button, ButtonVariants},
    h_flex,
};

#[derive(IntoElement)]
pub struct ProfileHeader<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub name: SharedString,
    pub avatar: ImageSource,
    pub on_logout: F,
}

impl<F> RenderOnce for ProfileHeader<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
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

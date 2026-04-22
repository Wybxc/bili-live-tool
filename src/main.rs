// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{rc::Rc, sync::Arc, time::Duration};

use image::Rgb;
use qrcode::QrCode;
use slint::{Image, ModelRc, Rgb8Pixel, SharedPixelBuffer, VecModel, Weak};

use crate::bili_api::Area;

mod bili_api;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            *s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic message"
        };
        rfd::MessageDialog::new()
            .set_title("Application Error")
            .set_description(message)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }));
    tracing_subscriber::fmt::init();

    let ui = MainWindow::new()?;
    init(&ui);
    ui.run()?;
    Ok(())
}

fn init(window: &MainWindow) {
    let login = window.global::<LoginLogic>();
    let user = window.global::<UserLogic>();
    let live = window.global::<LiveLogic>();

    login.on_refresh_qr_code({
        let login = login.as_weak();
        move || spawn(refresh_qr_code(login.clone()))
    });

    login.on_logout({
        let login = login.as_weak();
        let user = user.as_weak();
        move || {
            spawn(logout(login.clone()));
            spawn(poll_login_status(login.clone(), user.clone()));
        }
    });

    live.on_update_sub_area_list({
        let live = live.as_weak();
        move || spawn(update_sub_area_list(live.clone()))
    });

    spawn({
        let login = login.as_weak();
        let user = user.as_weak();
        async move {
            // If already logged in, skip the QR code login process.
            if init_user(user.clone()).await {
                login.unwrap().set_login_status(LoginStatus::Success);
                return;
            }

            // Not logged in, initialize the QR code login process.
            refresh_qr_code(login.clone()).await.unwrap();
            poll_login_status(login, user).await.unwrap();
        }
    });

    spawn({
        let live = live.as_weak();
        async move {
            init_live_area_list(live.clone()).await;
            update_sub_area_list(live.clone()).await;
        }
    })
}

async fn init_user(user: Weak<UserLogic<'static>>) -> bool {
    if let Ok(info) = bili_api::get_nav_user_info().await {
        if info.is_login {
            let user = user.unwrap();
            user.set_uname(info.uname.as_str().into());
            spawn(async move {
                if let Ok(face) = download_image(info.face.as_str()).await {
                    user.set_face(face);
                }
            });
            return true;
        }
    }
    false
}

/// Refresh the QR code and update the login state to Polling. If already logged in, do nothing.
async fn refresh_qr_code(login: Weak<LoginLogic<'static>>) -> anyhow::Result<()> {
    let response = bili_api::generate_passport_qrcode().await?;
    let qrcode = QrCode::new(response.url.as_str())?;
    let qrcode = qrcode.render::<Rgb<u8>>().build();
    let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
        qrcode.as_raw(),
        qrcode.width(),
        qrcode.height(),
    );

    let login = login.unwrap();
    login.set_qr_code(Image::from_rgb8(buffer));
    login.set_login_status(LoginStatus::Waiting);
    login.set_oauth_key(response.qrcode_key.as_str().into());
    login.set_qr_code_ready(true);

    Ok(())
}

async fn poll_login_status(
    login: Weak<LoginLogic<'static>>,
    user: Weak<UserLogic<'static>>,
) -> anyhow::Result<()> {
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let key = match login.unwrap().get_login_status() {
            LoginStatus::Waiting | LoginStatus::Confirming | LoginStatus::Expired => {
                login.unwrap().get_oauth_key()
            }
            LoginStatus::Success => break,
        };
        let Ok(status) = bili_api::poll_passport_qrcode_status(&key).await else {
            continue;
        };
        match status.code {
            bili_api::PollPassportQrcodeStatusCode::Success => {
                init_user(user).await;
                login.unwrap().set_login_status(LoginStatus::Success);
                bili_api::save_cookies();
                tracing::info!("Login success");
                break;
            }
            bili_api::PollPassportQrcodeStatusCode::Expired => {
                login.unwrap().set_login_status(LoginStatus::Expired);
            }
            bili_api::PollPassportQrcodeStatusCode::Confirming => {
                login.unwrap().set_login_status(LoginStatus::Confirming);
            }
            bili_api::PollPassportQrcodeStatusCode::Waiting => {
                login.unwrap().set_login_status(LoginStatus::Waiting);
            }
            bili_api::PollPassportQrcodeStatusCode::Unknown => {}
        }
    }
    Ok(())
}

async fn logout(login: Weak<LoginLogic<'static>>) -> anyhow::Result<()> {
    bili_api::clear_cookies();
    login.unwrap().set_qr_code_ready(false);
    login.unwrap().set_login_status(LoginStatus::Waiting);
    refresh_qr_code(login).await?;
    Ok(())
}

async fn live_area_list() -> Option<Arc<Vec<Area>>> {
    static AREA_LIST: async_once_cell::OnceCell<Arc<Vec<Area>>> = async_once_cell::OnceCell::new();
    AREA_LIST
        .get_or_try_init(async { bili_api::get_live_area_list().await.map(Arc::new) })
        .await
        .ok()
        .cloned()
}

async fn init_live_area_list(live: Weak<LiveLogic<'static>>) {
    let Some(area_list) = live_area_list().await else {
        return;
    };

    live.unwrap().set_area_list(collect(
        area_list.iter().map(|area| area.name.as_str().into()),
    ));
    live.unwrap().set_selected_area(
        area_list
            .first()
            .map(|area| area.name.as_str().into())
            .unwrap_or_default(),
    ); // TODO: persist the selected area
}

async fn update_sub_area_list(live: Weak<LiveLogic<'static>>) {
    let Some(area_list) = live_area_list().await else {
        return;
    };
    let selected_area_name = live.unwrap().get_selected_area();
    let Some(area) = area_list
        .iter()
        .find(|area| area.name.as_str() == selected_area_name.as_str())
        .or(area_list.first())
    else {
        tracing::warn!("Area list is empty");
        return;
    };

    live.unwrap().set_sub_area_list(collect(
        area.list
            .iter()
            .map(|sub_area| sub_area.name.as_str().into()),
    ));
}

fn spawn<F: std::future::Future + 'static>(f: F) {
    slint::spawn_local(async_compat::Compat::new(f)).unwrap();
}

fn collect<T: Clone + 'static>(iter: impl Iterator<Item = T>) -> ModelRc<T> {
    let model = VecModel::from_iter(iter);
    Rc::new(model).into()
}

async fn download_image(url: &str) -> anyhow::Result<Image> {
    let response = reqwest::get(url).await?;
    let bytes = response.bytes().await?;
    let img = image::load_from_memory(&bytes)?;
    if img.color().has_alpha() {
        let rgba8image = img.to_rgba8();
        Ok(Image::from_rgba8_premultiplied(
            SharedPixelBuffer::clone_from_slice(
                rgba8image.as_raw(),
                rgba8image.width(),
                rgba8image.height(),
            ),
        ))
    } else {
        let rgb8image = img.to_rgb8();
        Ok(Image::from_rgb8(SharedPixelBuffer::clone_from_slice(
            rgb8image.as_raw(),
            rgb8image.width(),
            rgb8image.height(),
        )))
    }
}

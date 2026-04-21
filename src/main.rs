// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Duration;

use image::Rgb;
use qrcode::QrCode;
use slint::{Image, Rgb8Pixel, SharedPixelBuffer, Weak};

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
    init_login_logic(ui.global::<LoginLogic>(), ui.global::<UserInfo>());
    ui.run()?;
    Ok(())
}

fn init_login_logic(login_logic: LoginLogic, user_info: UserInfo) {
    login_logic.on_refresh_qr_code({
        let logic = login_logic.as_weak();
        move || spawn(refresh_qr_code(logic.clone()))
    });

    spawn({
        let login_logic = login_logic.as_weak();
        let user_info = user_info.as_weak();
        async move {
            // If already logged in, skip the QR code login process.
            if init_user_info(user_info.clone()).await {
                login_logic.unwrap().set_login_status(LoginStatus::Success);
                return;
            }

            // Not logged in, initialize the QR code login process.
            refresh_qr_code(login_logic.clone()).await.unwrap();
            poll_login_status(login_logic, user_info).await.unwrap();
        }
    });
}

async fn init_user_info(user_info: Weak<UserInfo<'static>>) -> bool {
    if let Ok(info) = bili_api::get_nav_user_info().await {
        if info.is_login {
            let user_info = user_info.unwrap();
            user_info.set_uname(info.uname.as_str().into());
            spawn(async move {
                if let Ok(face) = download_image(info.face.as_str()).await {
                    user_info.set_face(face);
                }
            });
            return true;
        }
    }
    false
}

/// Refresh the QR code and update the login state to Polling. If already logged in, do nothing.
///
/// # Cancel safety
///
/// Not cancel safe.
async fn refresh_qr_code(login_logic: Weak<LoginLogic<'static>>) -> anyhow::Result<()> {
    let response = bili_api::generate_passport_qrcode().await?;
    let qrcode = QrCode::new(response.url.as_str())?;
    let qrcode = qrcode.render::<Rgb<u8>>().build();
    let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
        qrcode.as_raw(),
        qrcode.width(),
        qrcode.height(),
    );

    let login_logic = login_logic.unwrap();
    login_logic.set_qr_code(Image::from_rgb8(buffer));
    login_logic.set_login_status(LoginStatus::Waiting);
    login_logic.set_oauth_key(response.qrcode_key.as_str().into());
    login_logic.set_qr_code_ready(true);

    Ok(())
}

async fn poll_login_status(
    login_logic: Weak<LoginLogic<'static>>,
    user_info: Weak<UserInfo<'static>>,
) -> anyhow::Result<()> {
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let key = match login_logic.unwrap().get_login_status() {
            LoginStatus::Waiting | LoginStatus::Confirming | LoginStatus::Expired => {
                login_logic.unwrap().get_oauth_key()
            }
            LoginStatus::Success => break,
        };
        let Ok(status) = bili_api::poll_passport_qrcode_status(&key).await else {
            continue;
        };
        match status.code {
            bili_api::PollPassportQrcodeStatusCode::Success => {
                init_user_info(user_info).await;
                login_logic.unwrap().set_login_status(LoginStatus::Success);
                tracing::info!("Login success");
                break;
            }
            bili_api::PollPassportQrcodeStatusCode::Expired => {
                login_logic.unwrap().set_login_status(LoginStatus::Expired);
            }
            bili_api::PollPassportQrcodeStatusCode::Confirming => {
                login_logic
                    .unwrap()
                    .set_login_status(LoginStatus::Confirming);
            }
            bili_api::PollPassportQrcodeStatusCode::Waiting => {
                login_logic.unwrap().set_login_status(LoginStatus::Waiting);
            }
            bili_api::PollPassportQrcodeStatusCode::Unknown => {}
        }
    }
    Ok(())
}

fn spawn<F: std::future::Future + 'static>(f: F) {
    slint::spawn_local(async_compat::Compat::new(f)).unwrap();
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

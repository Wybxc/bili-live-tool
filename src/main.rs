// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use image::Rgb;
use qrcode::QrCode;
use slint::{
    Image, JoinHandle, Model, ModelRc, Rgb8Pixel, SharedPixelBuffer, SharedString, VecModel, Weak,
};

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
            .set_level(rfd::MessageLevel::Error)
            .show();
    }));
    tracing_subscriber::fmt::init();

    let ui = MainWindow::new()?;
    init(&ui);
    ui.run()?;
    Ok(())
}

fn toast(logic: Weak<Logic<'static>>, message: &str) {
    let notifications = logic.unwrap().get_notifications();
    let notifications = notifications
        .as_any()
        .downcast_ref::<VecModel<SharedString>>()
        .unwrap();
    notifications.push(message.into());
    spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let notifications = logic.unwrap().get_notifications();
        let notifications = notifications
            .as_any()
            .downcast_ref::<VecModel<SharedString>>()
            .unwrap();
        notifications.remove(0);
    });
}

fn init(window: &MainWindow) {
    let logic = window.global::<Logic>();

    logic.on_copy_to_clipboard({
        let logic = logic.as_weak();
        move |text| {
            if let Ok(()) = arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.set_text(&*text))
                .inspect_err(|e| {
                    tracing::error!("Failed to copy to clipboard: {}", e);
                    toast(logic.clone(), "复制失败");
                })
            {
                toast(logic.clone(), "复制成功");
            }
        }
    });

    logic.on_refresh_qr_code({
        let logic = logic.as_weak();
        move || {
            spawn(refresh_qr_code(logic.clone()));
        }
    });

    logic.on_logout({
        let logic = logic.as_weak();
        move || {
            spawn(logout(logic.clone()));
            start_login_session(logic.clone());
        }
    });

    logic.on_update_sub_area_list({
        let logic = logic.as_weak();
        move || {
            spawn(update_sub_area_list(logic.clone()));
        }
    });

    logic.on_update_live_area({
        let logic = logic.as_weak();
        move || {
            spawn(update_live_area(logic.clone()));
        }
    });

    logic.on_update_live_title({
        let logic = logic.as_weak();
        move || {
            spawn(update_live_title(logic.clone()));
        }
    });

    logic.on_start_live({
        let logic = logic.as_weak();
        move || {
            spawn(start_live(logic.clone()));
        }
    });

    logic.on_stop_live({
        let logic = logic.as_weak();
        move || {
            spawn(stop_live(logic.clone()));
        }
    });

    spawn({
        let logic = logic.as_weak();
        async move {
            // If already logged in, skip the QR code logic process.
            if init_user(logic.clone()).await {
                logic.unwrap().set_login_status(LoginStatus::Success);
                return;
            }

            // Not logged in, initialize the QR code logic process.
            start_login_session(logic);
        }
    });

    spawn({
        let logic = logic.as_weak();
        async move {
            init_live_area_list(logic.clone()).await;
            update_sub_area_list(logic.clone()).await;
        }
    });
}

async fn init_user(logic: Weak<Logic<'static>>) -> bool {
    if let Ok(info) = bili_api::get_nav_user_info().await {
        if info.is_login {
            logic.unwrap().set_uname(info.uname.as_str().into());

            logic.unwrap().set_room_id_status(RoomIdStatus::Fetching);
            spawn({
                let logic = logic.clone();
                async move {
                    if let Ok(room_id) = bili_api::get_room_id(info.mid).await {
                        let logic = logic.unwrap();
                        logic.set_room_id(room_id.room_id.to_string().into());
                        logic.set_room_id_status(RoomIdStatus::Ok);
                    } else {
                        logic.unwrap().set_room_id_status(RoomIdStatus::Failed);
                    }

                    init_room_info(logic.clone()).await.unwrap_or_else(|e| {
                        tracing::error!("Failed to initialize room info: {}", e);
                    });
                }
            });

            spawn(async move {
                let logic = logic.unwrap();
                if let Ok(face) = download_image(info.face.as_str()).await {
                    logic.set_face(face);
                }
            });

            return true;
        }
    }
    false
}

async fn init_room_info(logic: Weak<Logic<'static>>) -> anyhow::Result<()> {
    let room_id = logic.unwrap().get_room_id().parse::<u64>()?;
    let room_info = bili_api::get_room_info(room_id).await?;
    logic.unwrap().set_title(room_info.title.as_str().into());
    logic
        .unwrap()
        .set_live_status(if room_info.live_status == bili_api::LiveStatus::Living {
            LiveStatus::Living
        } else {
            LiveStatus::Off
        });

    if let Some(area_list) = live_area_list().await {
        let area = area_list
            .iter()
            .find(|area| area.id == room_info.parent_area_id)
            .ok_or_else(|| anyhow::anyhow!("Area not found"))?;
        let sub_area = area
            .list
            .iter()
            .find(|sub_area| sub_area.id == room_info.area_id)
            .ok_or_else(|| anyhow::anyhow!("Sub area not found"))?;
        tracing::info!(
            "Room area: {} > {}",
            area.name.as_str(),
            sub_area.name.as_str()
        );

        let logic = logic.unwrap();
        logic.set_selected_area(area.name.as_str().into());
        set_array(
            &logic.get_sub_area_list(),
            area.list
                .iter()
                .map(|sub_area| sub_area.name.as_str().into()),
        );
        logic.set_selected_sub_area(sub_area.name.as_str().into());
    }
    Ok(())
}

/// Refresh the QR code and update the logic state to Polling. If already logged in, do nothing.
async fn refresh_qr_code(logic: Weak<Logic<'static>>) -> anyhow::Result<()> {
    logic.unwrap().set_qr_code_ready(false);

    let response = bili_api::generate_passport_qrcode().await?;
    let qrcode = QrCode::new(response.url.as_str())?;
    let qrcode = qrcode.render::<Rgb<u8>>().build();
    let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
        qrcode.as_raw(),
        qrcode.width(),
        qrcode.height(),
    );

    let logic = logic.unwrap();
    logic.set_qr_code(Image::from_rgb8(buffer));
    logic.set_login_status(LoginStatus::Waiting);
    logic.set_oauth_key(response.qrcode_key.as_str().into());
    logic.set_qr_code_ready(true);

    Ok(())
}

fn start_login_session(logic: Weak<Logic<'static>>) {
    static LOGIN_SESSION_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
    let mut handle_lock = LOGIN_SESSION_HANDLE.lock().unwrap();

    // Abort the previous logic session if it exists.
    if let Some(handle) = handle_lock.take() {
        handle.abort();
    }

    // Start a new logic session.
    let handle = spawn(async move {
        let _ = refresh_qr_code(logic.clone()).await;
        let _ = login_session(logic).await;
    });
    *handle_lock = Some(handle);
}

async fn login_session(logic: Weak<Logic<'static>>) -> anyhow::Result<()> {
    loop {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let key = match logic.unwrap().get_login_status() {
            LoginStatus::Waiting | LoginStatus::Confirming | LoginStatus::Expired => {
                logic.unwrap().get_oauth_key()
            }
            LoginStatus::Success => break,
        };
        let Ok(status) = bili_api::poll_passport_qrcode_status(&key).await else {
            continue;
        };
        match status.code {
            bili_api::PollPassportQrcodeStatusCode::Success => {
                init_user(logic.clone()).await;
                logic.unwrap().set_login_status(LoginStatus::Success);
                bili_api::save_cookies();
                tracing::info!("Login success");
                break;
            }
            bili_api::PollPassportQrcodeStatusCode::Expired => {
                logic.unwrap().set_login_status(LoginStatus::Expired);
            }
            bili_api::PollPassportQrcodeStatusCode::Confirming => {
                logic.unwrap().set_login_status(LoginStatus::Confirming);
            }
            bili_api::PollPassportQrcodeStatusCode::Waiting => {
                logic.unwrap().set_login_status(LoginStatus::Waiting);
            }
            bili_api::PollPassportQrcodeStatusCode::Unknown => {}
        }
    }
    Ok(())
}

async fn logout(logic: Weak<Logic<'static>>) -> anyhow::Result<()> {
    bili_api::clear_cookies();
    logic.unwrap().set_login_status(LoginStatus::Waiting);
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

async fn init_live_area_list(logic: Weak<Logic<'static>>) {
    let Some(area_list) = live_area_list().await else {
        return;
    };

    set_array(
        &logic.unwrap().get_area_list(),
        area_list.iter().map(|area| area.name.as_str().into()),
    );
}

async fn update_sub_area_list(logic: Weak<Logic<'static>>) {
    let Some(area_list) = live_area_list().await else {
        return;
    };
    let selected_area_name = logic.unwrap().get_selected_area();
    let Some(area) = area_list
        .iter()
        .find(|area| area.name.as_str() == selected_area_name.as_str())
        .or(area_list.first())
    else {
        tracing::warn!("Area list is empty");
        return;
    };

    set_array(
        &logic.unwrap().get_sub_area_list(),
        area.list
            .iter()
            .map(|sub_area| sub_area.name.as_str().into()),
    );
}

async fn update_live_area(logic: Weak<Logic<'static>>) {
    let selected_area_name = logic.unwrap().get_selected_area();
    let selected_sub_area_name = logic.unwrap().get_selected_sub_area();
    let Some(area_list) = live_area_list().await else {
        return;
    };
    let Some(area) = area_list
        .iter()
        .find(|area| area.name.as_str() == selected_area_name.as_str())
    else {
        tracing::warn!("Area not found: {}", selected_area_name.as_str());
        return;
    };
    let Some(sub_area) = area
        .list
        .iter()
        .find(|sub_area| sub_area.name.as_str() == selected_sub_area_name.as_str())
    else {
        tracing::warn!(
            "Sub area not found: {} > {}",
            selected_area_name.as_str(),
            selected_sub_area_name.as_str()
        );
        return;
    };
    let room_id = logic.unwrap().get_room_id();
    let Ok(room_id) = room_id.parse::<u64>() else {
        tracing::error!("Invalid room id: {room_id}");
        return;
    };
    let Some(csrf) = bili_api::get_csrf_token() else {
        tracing::error!("Failed to get CSRF token");
        return;
    };
    if let Err(e) = bili_api::update_area(room_id, sub_area.id, &csrf).await {
        tracing::error!("Failed to update live area: {}", e);
        toast(logic, &format!("更新直播分区失败：{}", e));
        return;
    };
    toast(logic, "更新直播分区成功");
}

async fn update_live_title(logic: Weak<Logic<'static>>) {
    let title = logic.unwrap().get_title();
    let room_id = logic.unwrap().get_room_id();
    let Ok(room_id) = room_id.parse::<u64>() else {
        tracing::error!("Invalid room id: {room_id}");
        return;
    };
    let Some(csrf) = bili_api::get_csrf_token() else {
        tracing::error!("Failed to get CSRF token");
        return;
    };
    if let Err(e) = bili_api::update_title(room_id, &title, &csrf).await {
        tracing::error!("Failed to update live title: {}", e);
        toast(logic, &format!("更新标题失败：{}", e));
        return;
    }
    toast(logic, "更新标题成功");
}

async fn start_live(logic: Weak<Logic<'static>>) {
    let room_id = logic.unwrap().get_room_id();
    let Ok(room_id) = room_id.parse::<u64>() else {
        tracing::error!("Invalid room id: {room_id}");
        return;
    };

    let selected_area = logic.unwrap().get_selected_area();
    let selected_sub_area = logic.unwrap().get_selected_sub_area();
    let Some(area_list) = live_area_list().await else {
        tracing::error!("Failed to get live area list");
        return;
    };
    let Some(area) = area_list
        .iter()
        .find(|area| area.name.as_str() == selected_area.as_str())
    else {
        tracing::warn!("Area not found: {}", selected_area.as_str());
        return;
    };
    let Some(sub_area) = area
        .list
        .iter()
        .find(|sub_area| sub_area.name.as_str() == selected_sub_area.as_str())
    else {
        tracing::warn!(
            "Sub area not found: {} > {}",
            selected_area.as_str(),
            selected_sub_area.as_str()
        );
        return;
    };

    let Some(csrf) = bili_api::get_csrf_token() else {
        tracing::error!("Failed to get CSRF token");
        return;
    };

    let timestamp = match bili_api::get_timestamp().await {
        Ok(ts) => ts,
        Err(e) => {
            tracing::error!("Failed to get timestamp: {}", e);
            toast(logic, &format!("获取时间戳失败：{}", e));
            return;
        }
    };

    let version = match bili_api::get_live_version(timestamp).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to get live version: {}", e);
            toast(logic, &format!("获取直播版本失败：{}", e));
            return;
        }
    };

    let response = match bili_api::start_live(room_id, sub_area.id, &csrf, version, timestamp).await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to start live: {}", e);
            toast(logic, &format!("开播失败：{}", e));
            return;
        }
    };

    let mut protocols = vec![Protocol {
        name: "RTMP".into(),
        addr: response.rtmp.addr.as_str().into(),
        code: response.rtmp.code.as_str().into(),
    }];
    for protocol in &response.protocols {
        protocols.push(Protocol {
            name: protocol.protocol.to_uppercase().into(),
            addr: protocol.addr.as_str().into(),
            code: protocol.code.as_str().into(),
        });
    }

    // Add suffix to duplicate protocol names to distinguish them in the UI, e.g., "RTMP 1", "RTMP 2".
    let mut freq = HashMap::new();
    for protocol in &protocols {
        *freq.entry(protocol.name.clone()).or_insert(0) += 1;
    }
    let mut suffix = HashMap::new();
    let protocols = protocols
        .into_iter()
        .map(|protocol| {
            if freq[protocol.name.as_str()] > 1 {
                let count = suffix.entry(protocol.name.clone()).or_insert(0);
                *count += 1;
                Protocol {
                    name: format!("{} {}", protocol.name.as_str(), *count).into(),
                    ..protocol
                }
            } else {
                protocol
            }
        })
        .collect::<Vec<_>>();

    set_array(&logic.unwrap().get_protocols(), protocols.into_iter());

    logic.unwrap().set_live_status(LiveStatus::Living);

    toast(logic, "开播成功");
}

async fn stop_live(logic: Weak<Logic<'static>>) {
    let room_id = logic.unwrap().get_room_id();
    let Ok(room_id) = room_id.parse::<u64>() else {
        tracing::error!("Invalid room id: {room_id}");
        return;
    };
    let Some(csrf) = bili_api::get_csrf_token() else {
        tracing::error!("Failed to get CSRF token");
        return;
    };
    if let Err(e) = bili_api::stop_live(room_id, &csrf).await {
        tracing::error!("Failed to stop live: {}", e);
        toast(logic, &format!("下播失败：{}", e));
        return;
    }
    logic.unwrap().set_live_status(LiveStatus::Off);
    toast(logic, "下播成功");
}

fn spawn<F: std::future::Future + 'static>(f: F) -> JoinHandle<F::Output> {
    slint::spawn_local(async_compat::Compat::new(f)).unwrap()
}

fn set_array<T: Clone + 'static>(model: &ModelRc<T>, iter: impl Iterator<Item = T>) {
    let model = model.as_any().downcast_ref::<VecModel<T>>().unwrap();
    model.set_vec(iter.collect::<Vec<_>>());
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

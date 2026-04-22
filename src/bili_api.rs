#![allow(dead_code)]

use std::{
    io::BufReader,
    sync::{Arc, LazyLock},
};

use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde_aux::field_attributes::deserialize_number_from_string;
use smol_str::SmolStr;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Middleware error: {0}")]
    Middleware(anyhow::Error),
    #[error("API error: code {code}, message: {message}")]
    Api { code: i32, message: SmolStr },
    #[error("Empty payload")]
    EmptyPayload,
}

impl From<reqwest_middleware::Error> for Error {
    fn from(err: reqwest_middleware::Error) -> Self {
        use reqwest_middleware::Error::*;
        match err {
            Middleware(error) => Error::Middleware(error),
            Reqwest(error) => Error::Http(error),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(serde::Deserialize, Default, Debug)]
#[serde(default)]
pub struct Response<T> {
    pub code: i32,
    pub message: SmolStr,
    pub data: Option<T>,
}

impl<T> Response<T> {
    pub fn into_data(self) -> Result<T> {
        if self.code == 0 {
            if let Some(data) = self.data {
                Ok(data)
            } else {
                Err(Error::EmptyPayload)
            }
        } else {
            Err(Error::Api {
                code: self.code,
                message: self.message,
            })
        }
    }
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct NoneData {}

#[derive(serde::Deserialize, Default, Debug)]
#[serde(default)]
pub struct GeneratePassportQrcode {
    pub url: SmolStr,
    pub qrcode_key: SmolStr,
}

pub async fn generate_passport_qrcode() -> Result<GeneratePassportQrcode> {
    const URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
    let client = client();
    let response = client.get(URL).send().await?;
    let json: Response<GeneratePassportQrcode> = response.json().await?;
    tracing::info!(
        "Generated Passport QR code with URL: {}",
        json.data.as_ref().map(|d| d.url.as_str()).unwrap_or("N/A")
    );
    json.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_generate_passport_qrcode() {
    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "url": "https://passport.bilibili.com/h5-app/passport/login/scan?navhide=1\u0026qrcode_key=8587cf8106a0b863c46d6bab913537f6\u0026from=",
            "qrcode_key": "8587cf8106a0b863c46d6bab913537f6"
        }
    }"#;
    let response: Response<GeneratePassportQrcode> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(
        data.url,
        r"https://passport.bilibili.com/h5-app/passport/login/scan?navhide=1&qrcode_key=8587cf8106a0b863c46d6bab913537f6&from="
    );
    assert_eq!(data.qrcode_key, "8587cf8106a0b863c46d6bab913537f6");
}

#[derive(serde::Deserialize, Default, Debug)]
#[serde(default)]
pub struct PollPassportQrcodeStatus {
    pub url: SmolStr,
    pub refresh_token: SmolStr,
    pub timestamp: u64,
    pub code: PollPassportQrcodeStatusCode,
    pub message: SmolStr,
}

#[derive(serde_repr::Deserialize_repr, Default, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(i32)]
pub enum PollPassportQrcodeStatusCode {
    Success = 0,
    Expired = 86038,
    Confirming = 86090,
    Waiting = 86101,
    #[default]
    Unknown,
}

pub async fn poll_passport_qrcode_status(key: &str) -> Result<PollPassportQrcodeStatus> {
    const URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";
    let client = client();
    let response = client.get(URL).query(&[("qrcode_key", key)]).send().await?;

    let json: Response<PollPassportQrcodeStatus> = response.json().await?;
    tracing::info!(
        "Polled Passport QR code status: code {:?}, message {}",
        json.data.as_ref().map(|d| d.code).unwrap_or_default(),
        json.data
            .as_ref()
            .map(|d| d.message.as_str())
            .unwrap_or("N/A")
    );
    let data = json.into_data()?;
    Ok(data)
}

#[cfg(test)]
#[test]
fn deserialize_poll_passport_qrcode_status() {
    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "url": "",
            "refresh_token": "",
            "timestamp": 0,
            "code": 86101,
            "message": "未扫码"
        }
    }"#;
    let response: Response<PollPassportQrcodeStatus> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(data.url, "");
    assert_eq!(data.refresh_token, "");
    assert_eq!(data.timestamp, 0);
    assert_eq!(data.code, PollPassportQrcodeStatusCode::Waiting);
    assert_eq!(data.message, "未扫码");

    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "url": "",
            "refresh_token": "",
            "timestamp": 0,
            "code": 86090,
            "message": "二维码已扫码未确认"
        }
    }"#;
    let response: Response<PollPassportQrcodeStatus> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(data.url, "");
    assert_eq!(data.refresh_token, "");
    assert_eq!(data.timestamp, 0);
    assert_eq!(data.code, PollPassportQrcodeStatusCode::Confirming);
    assert_eq!(data.message, "二维码已扫码未确认");

    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "url": "https://passport.biligame.com/crossDomain?DedeUserID=***\u0026DedeUserID__ckMd5=***\u0026Expires=***\u0026SESSDATA=***\u0026bili_jct=***\u0026gourl=https%3A%2F%2Fpassport.bilibili.com",
            "refresh_token": "***",
            "timestamp": 1662363009601,
            "code": 0,
            "message": ""
        }
    }"#;
    let response: Response<PollPassportQrcodeStatus> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(data.url, "https://passport.biligame.com/crossDomain?DedeUserID=***&DedeUserID__ckMd5=***&Expires=***&SESSDATA=***&bili_jct=***&gourl=https%3A%2F%2Fpassport.bilibili.com");
    assert_eq!(data.refresh_token, "***");
    assert_eq!(data.timestamp, 1662363009601);
    assert_eq!(data.code, PollPassportQrcodeStatusCode::Success);
    assert_eq!(data.message, "");

    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "url": "",
            "refresh_token": "",
            "timestamp": 0,
            "code": 86038,
            "message": "二维码已失效"
        }
    }"#;
    let response: Response<PollPassportQrcodeStatus> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(data.url, "");
    assert_eq!(data.refresh_token, "");
    assert_eq!(data.timestamp, 0);
    assert_eq!(data.code, PollPassportQrcodeStatusCode::Expired);
    assert_eq!(data.message, "二维码已失效");
}

#[derive(serde::Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct NavUserInfo {
    pub is_login: bool,
    pub face: SmolStr,
    pub uname: SmolStr,
    pub mid: u64,
}

pub async fn get_nav_user_info() -> Result<NavUserInfo> {
    const URL: &str = "https://api.bilibili.com/x/web-interface/nav";
    let client = client();
    let response = client.get(URL).send().await?;
    let json: Response<NavUserInfo> = response.json().await?;
    tracing::info!(
        "Fetched nav user info: is_login={}, uname={}",
        json.data.as_ref().map(|d| d.is_login).unwrap_or(false),
        json.data
            .as_ref()
            .map(|d| d.uname.as_str())
            .unwrap_or("N/A")
    );
    json.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_nav_user_info() {
    let json_str = r##"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "isLogin": true,
            "email_verified": 1,
            "face": "https://i0.hdslb.com/bfs/face/aebb2639a0d47f2ce1fec0631f412eaf53d4a0be.jpg",
            "face_nft": 0,
            "face_nft_type": 0,
            "level_info": {
                "current_level": 6,
                "current_min": 28800,
                "current_exp": 52689,
                "next_exp": "--"
            },
            "mid": 293793435,
            "mobile_verified": 1,
            "money": 172.4,
            "moral": 70,
            "official": {
                "role": 0,
                "title": "",
                "desc": "",
                "type": -1
            },
            "officialVerify": {
                "type": -1,
                "desc": ""
            },
            "pendant": {
                "pid": 2511,
                "name": "初音未来13周年",
                "image": "https://i0.hdslb.com/bfs/garb/item/4f8f3f1f2d47f0dad84f66aa57acd4409ea46361.png",
                "expire": 0,
                "image_enhance": "https://i0.hdslb.com/bfs/garb/item/fe0b83b53e2342b16646f6e7a9370d8a867decdb.webp",
                "image_enhance_frame": "https://i0.hdslb.com/bfs/garb/item/127c507ec8448be30cf5f79500ecc6ef2fd32f2c.png"
            },
            "scores": 0,
            "uname": "社会易姐QwQ",
            "vipDueDate": 1707494400000,
            "vipStatus": 1,
            "vipType": 2,
            "vip_pay_type": 0,
            "vip_theme_type": 0,
            "vip_label": {
                "path": "",
                "text": "年度大会员",
                "label_theme": "annual_vip",
                "text_color": "#FFFFFF",
                "bg_style": 1,
                "bg_color": "#FB7299",
                "border_color": "",
                "use_img_label": true,
                "img_label_uri_hans": "",
                "img_label_uri_hant": "",
                "img_label_uri_hans_static": "https://i0.hdslb.com/bfs/vip/8d4f8bfc713826a5412a0a27eaaac4d6b9ede1d9.png",
                "img_label_uri_hant_static": "https://i0.hdslb.com/bfs/activity-plat/static/20220614/e369244d0b14644f5e1a06431e22a4d5/VEW8fCC0hg.png"
            },
            "vip_avatar_subscript": 1,
            "vip_nickname_color": "#FB7299",
            "vip": {
                "type": 2,
                "status": 1,
                "due_date": 1707494400000,
                "vip_pay_type": 0,
                "theme_type": 0,
                "label": {
                    "path": "",
                    "text": "年度大会员",
                    "label_theme": "annual_vip",
                    "text_color": "#FFFFFF",
                    "bg_style": 1,
                    "bg_color": "#FB7299",
                    "border_color": "",
                    "use_img_label": true,
                    "img_label_uri_hans": "",
                    "img_label_uri_hant": "",
                    "img_label_uri_hans_static": "https://i0.hdslb.com/bfs/vip/8d4f8bfc713826a5412a0a27eaaac4d6b9ede1d9.png",
                    "img_label_uri_hant_static": "https://i0.hdslb.com/bfs/activity-plat/static/20220614/e369244d0b14644f5e1a06431e22a4d5/VEW8fCC0hg.png"
                },
                "avatar_subscript": 1,
                "nickname_color": "#FB7299",
                "role": 3,
                "avatar_subscript_url": "",
                "tv_vip_status": 0,
                "tv_vip_pay_type": 0,
                "tv_due_date": 1640793600
            },
            "wallet": {
                "mid": 293793435,
                "bcoin_balance": 5,
                "coupon_balance": 5,
                "coupon_due_time": 0
            },
            "has_shop": true,
            "shop_url": "https://gf.bilibili.com?msource=main_station",
            "allowance_count": 0,
            "answer_status": 0,
            "is_senior_member": 1,
            "wbi_img": {
                "img_url": "https://i0.hdslb.com/bfs/wbi/653657f524a547ac981ded72ea172057.png",
                "sub_url": "https://i0.hdslb.com/bfs/wbi/6e4909c702f846728e64f6007736a338.png"
            },
            "is_jury": false
        }
    }"##;
    let response: Response<NavUserInfo> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert!(data.is_login);
    assert_eq!(
        data.face,
        "https://i0.hdslb.com/bfs/face/aebb2639a0d47f2ce1fec0631f412eaf53d4a0be.jpg"
    );
    assert_eq!(data.uname, "社会易姐QwQ");

    let json_str = r#"{
        "code": -101,
        "message": "账号未登录",
        "ttl": 1,
        "data": {
            "isLogin": false,
            "wbi_img": {
                "img_url": "https://i0.hdslb.com/bfs/wbi/653657f524a547ac981ded72ea172057.png",
                "sub_url": "https://i0.hdslb.com/bfs/wbi/6e4909c702f846728e64f6007736a338.png"
            }
        }
    }"#;
    let response: Response<NavUserInfo> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, -101);
    assert_eq!(response.message, "账号未登录");
    let data = response.data.unwrap();
    assert!(!data.is_login);
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct RoomId {
    pub room_id: u64,
}

pub async fn get_room_id(user_id: u64) -> Result<RoomId> {
    const URL: &str = "https://api.live.bilibili.com/room/v2/Room/room_id_by_uid";
    let client = client();
    let response = client.get(URL).query(&[("uid", user_id)]).send().await?;
    let json: Response<RoomId> = response.json().await?;
    tracing::info!(
        "Fetched room ID: {}",
        json.data.as_ref().map(|d| d.room_id).unwrap_or(0)
    );
    json.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_room_id() {
    let json_str = r#"{
        "code": 0,
        "msg": "ok",
        "message": "ok",
        "data": {
            "room_id": 123456
        }
    }"#;
    let response: Response<RoomId> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "ok");
    let data = response.into_data().unwrap();
    assert_eq!(data.room_id, 123456);
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct RoomInfo {
    pub title: String,
    pub area_id: u64,
    pub parent_area_id: u64,
}

pub async fn get_room_info(room_id: u64) -> Result<RoomInfo> {
    const URL: &str = "https://api.live.bilibili.com/room/v1/Room/get_info";
    let client = client();
    let response = client
        .get(URL)
        .query(&[("room_id", room_id)])
        .send()
        .await?;
    let json: Response<RoomInfo> = response.json().await?;
    tracing::info!(
        "Fetched room info for room_id {}: title='{}', area_id={}, parent_area_id={}",
        room_id,
        json.data
            .as_ref()
            .map(|d| d.title.as_str())
            .unwrap_or("N/A"),
        json.data.as_ref().map(|d| d.area_id).unwrap_or(0),
        json.data.as_ref().map(|d| d.parent_area_id).unwrap_or(0)
    );
    json.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_room_info() {
    let json_str = r#"{
        "code": 0,
        "msg": "ok",
        "message": "ok",
        "data": {
            "uid": 9617619,
            "room_id": 5440,
            "short_id": 1,
            "attention": 11919499,
            "online": 0,
            "is_portrait": false,
            "description": "欢迎加入bilibili《快乐运动研究社》，和B站UP主们一起探讨有关运动的经历感受，解决身体和情绪的“疑难杂症”，寻找适合自己的运动，一起跟练！本期我们一起探讨：运动健身能缓解社交恐惧吗？",
            "live_status": 2,
            "area_id": 145,
            "parent_area_id": 1,
            "parent_area_name": "娱乐",
            "old_area_id": 6,
            "background": "",
            "title": "快乐运动研究社",
            "user_cover": "https://i0.hdslb.com/bfs/live/new_room_cover/96943b8d106a777a34cf796421bb4254163b30e1.jpg",
            "keyframe": "https://i0.hdslb.com/bfs/live-key-frame/keyframe08121926000000005440np0q7a.jpg",
            "is_strict_room": false,
            "live_time": "0000-00-00 00:00:00",
            "tags": "",
            "is_anchor": 0,
            "room_silent_type": "",
            "room_silent_level": 1,
            "room_silent_second": 0,
            "area_name": "视频聊天",
            "pendants": "",
            "area_pendants": "",
            "hot_words": [
                "2333333",
                "喂，妖妖零吗",
                "红红火火恍恍惚惚",
                "FFFFFFFFFF",
                "Yooooooo",
                "啪啪啪啪啪",
                "666666666",
                "老司机带带我",
                "你为什么这么熟练啊",
                "gg",
                "prprpr",
                "向大佬低头",
                "请大家注意弹幕礼仪哦！",
                "还有这种操作！",
                "囍",
                "打call",
                "你气不气？",
                "队友呢？"
            ],
            "hot_words_status": 0,
            "verify": "",
            "new_pendants": {
                "frame": {
                    "name": "",
                    "value": "",
                    "position": 0,
                    "desc": "",
                    "area": 0,
                    "area_old": 0,
                    "bg_color": "",
                    "bg_pic": "",
                    "use_old_area": false
                },
                "badge": {
                    "name": "v_company",
                    "position": 3,
                    "value": "",
                    "desc": "哔哩哔哩直播官方账号"
                },
                "mobile_frame": {
                    "name": "",
                    "value": "",
                    "position": 0,
                    "desc": "",
                    "area": 0,
                    "area_old": 0,
                    "bg_color": "",
                    "bg_pic": "",
                    "use_old_area": false
                },
                "mobile_badge": null
            },
            "up_session": "",
            "pk_status": 0,
            "pk_id": 0,
            "battle_id": 0,
            "allow_change_area_time": 0,
            "allow_upload_cover_time": 0,
            "studio_info": {
            "status": 0,
            "master_list": []
            }
        }
    }"#;
    let response: Response<RoomInfo> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "ok");
    let data = response.into_data().unwrap();
    assert_eq!(data.title, "快乐运动研究社");
    assert_eq!(data.area_id, 145);
    assert_eq!(data.parent_area_id, 1);
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct Area {
    pub id: u64,
    pub name: SmolStr,
    pub list: Vec<SubArea>,
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct SubArea {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub id: u64,
    pub name: SmolStr,
}

pub async fn get_live_area_list() -> Result<Vec<Area>> {
    const URL: &str = "https://api.live.bilibili.com/room/v1/Area/getList";
    let client = client();
    let response = client.get(URL).send().await?;
    let json: Response<Vec<Area>> = response.json().await?;
    tracing::info!(
        "Fetched area list with {} areas",
        json.data.as_ref().map(|d| d.len()).unwrap_or(0)
    );
    json.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_area_list() {
    let json_str = r#"{
        "code": 0,
        "msg": "success",
        "message": "success",
        "data": [
            {
                "id": 2,
                "name": "网游",
                "list": [
                    {
                        "id": "86",
                        "parent_id": "2",
                        "old_area_id": "4",
                        "name": "英雄联盟",
                        "act_id": "0",
                        "pk_status": "0",
                        "hot_status": 1,
                        "lock_status": "0",
                        "pic": "http://i0.hdslb.com/bfs/vc/dcfb14f14ec83e503147a262e7607858b05d7ac0.png",
                        "parent_name": "网游",
                        "area_type": 0
                    },
                    {
                        "id": "252",
                        "parent_id": "2",
                        "old_area_id": "3",
                        "name": "逃离塔科夫",
                        "act_id": "0",
                        "pk_status": "0",
                        "hot_status": 1,
                        "lock_status": "0",
                        "pic": "http://i0.hdslb.com/bfs/vc/762a7de3dd5fe8165d1d55b232484a017941592f.png",
                        "parent_name": "网游",
                        "area_type": 0
                    },
                    {
                        "id": "80",
                        "parent_id": "2",
                        "old_area_id": "1",
                        "name": "绝地求生",
                        "act_id": "0",
                        "pk_status": "0",
                        "hot_status": 1,
                        "lock_status": "0",
                        "pic": "http://i0.hdslb.com/bfs/vc/43ca83fdcd10505eaeef1b76cf8ce642a53b94da.png",
                        "parent_name": "网游",
                        "area_type": 0
                    }
                ]
            }
        ]
    }"#;

    let response: Response<Vec<Area>> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "success");
    let data = response.into_data().unwrap();
    assert_eq!(data.len(), 1);
    let area = &data[0];
    assert_eq!(area.id, 2);
    assert_eq!(area.name, "网游");
    assert_eq!(area.list.len(), 3);
    assert_eq!(area.list[0].id, 86);
    assert_eq!(area.list[0].name, "英雄联盟");
    assert_eq!(area.list[1].id, 252);
    assert_eq!(area.list[1].name, "逃离塔科夫");
    assert_eq!(area.list[2].id, 80);
    assert_eq!(area.list[2].name, "绝地求生");
}

pub async fn update_title(room_id: u64, title: &str, csrf: &str) -> Result<()> {
    const URL: &str = "https://api.live.bilibili.com/room/v1/Room/update";
    let client = client();
    let response = client
        .post(URL)
        .form(&[
            ("room_id", room_id.to_string().as_str()),
            ("platform", "pc_link"),
            ("title", title),
            ("csrf_token", csrf),
            ("csrf", csrf),
        ])
        .send()
        .await?;
    let json: Response<NoneData> = response.json().await?;
    tracing::info!(
        "Updated live stream title to '{}' for room_id {}",
        title,
        room_id
    );
    json.into_data()?;
    Ok(())
}

pub async fn update_area(room_id: u64, area_id: u64, csrf: &str) -> Result<()> {
    const URL: &str = "https://api.live.bilibili.com/room/v1/Room/update";
    let client = client();
    let response = client
        .post(URL)
        .form(&[
            ("room_id", room_id.to_string().as_str()),
            ("area_id", area_id.to_string().as_str()),
            ("platform", "pc_link"),
            ("csrf_token", csrf),
            ("csrf", csrf),
        ])
        .send()
        .await
        .unwrap();
    let json: Response<NoneData> = response.json().await?;
    tracing::info!(
        "Updated live stream area to '{}' for room_id {}",
        area_id,
        room_id
    );
    json.into_data()?;
    Ok(())
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct Timpstamp {
    pub now: u64,
}

pub async fn get_timestamp() -> Result<u64> {
    const URL: &str = "https://api.bilibili.com/x/report/click/now";
    let client = client();
    let response = client.get(URL).send().await?;
    let json: Response<Timpstamp> = response.json().await?;
    tracing::info!(
        "Fetched timestamp: {}",
        json.data.as_ref().map(|d| d.now).unwrap_or(0)
    );
    let data = json.into_data()?;
    Ok(data.now)
}

#[cfg(test)]
#[test]
fn deserialize_timestamp() {
    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "now": 1592666471
        }
    }"#;
    let response: Response<Timpstamp> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(data.now, 1592666471);
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct LiveVersion {
    pub curr_version: SmolStr,
    pub build: u64,
}

pub async fn get_live_version(timestamp: u64) -> Result<LiveVersion> {
    const URL: &str =
        "https://api.live.bilibili.com/xlive/app-blink/v1/liveVersionInfo/getHomePageLiveVersion";
    let client = client();
    let response = client
        .get(URL)
        .query(&app_sign(vec![
            ("system_version", 2.into()),
            ("ts", timestamp.into()),
        ]))
        .send()
        .await?;
    let json: Response<LiveVersion> = response.json().await?;
    tracing::info!(
        "Fetched live version: {} (build {})",
        json.data
            .as_ref()
            .map(|d| d.curr_version.as_str())
            .unwrap_or("N/A"),
        json.data.as_ref().map(|d| d.build).unwrap_or(0)
    );
    json.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_live_version() {
    let json_str = r#"{
        "code": 0,
        "message": "0",
        "ttl": 1,
        "data": {
            "curr_version": "7.19.0.9432",
            "build": 9432,
            "instruction": "\u3010\u65b0\u589e\u3011\u65b0\u589e\u7f8e\u989c\u8c03\u6574\u5165\u53e3\n\u3010\u4f18\u5316\u3011\u5df2\u77e5\u95ee\u9898\u4f18\u5316",
            "file_size": "300867136",
            "file_md5": "e1619a8e2603aa94b58a58121f94403f",
            "content": "<p>\u3010\u65b0\u589e\u3011\u65b0\u589e\u7f8e\u989c\u8c03\u6574\u5165\u53e3<br>\u3010\u4f18\u5316\u3011\u5df2\u77e5\u95ee\u9898\u4f18\u5316</p><p></p><p><br></p>",
            "download_url": "https://dl.hdslb.com/bili/bililive/win/Livehime-Win-beta-7.19.0.9432-x64.exe",
            "hdiffpatch_switch": 1
        }
    }"#;
    let response: Response<LiveVersion> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "0");
    let data = response.into_data().unwrap();
    assert_eq!(data.curr_version, "7.19.0.9432");
    assert_eq!(data.build, 9432);
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct StartLiveResponse {
    pub rtmp: Rtmp,
    pub protocols: Vec<Protocol>,
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct Rtmp {
    pub addr: SmolStr,
    pub code: SmolStr,
}

#[derive(serde::Deserialize, Default, Debug)]
pub struct Protocol {
    pub protocol: SmolStr,
    pub addr: SmolStr,
    pub code: SmolStr,
}

pub async fn start_live(
    room_id: u64,
    area_id: u64,
    csrf: &str,
    version: LiveVersion,
    timestamp: u64,
) -> Result<StartLiveResponse> {
    const URL: &str = "https://api.live.bilibili.com/room/v1/Room/startLive";
    let client = client();
    let response = client
        .post(URL)
        .form(&app_sign(vec![
            ("room_id", room_id.into()),
            ("area_v2", area_id.into()),
            ("platform", "pc_link".into()),
            ("backup_stream", "0".into()),
            ("csrf_token", csrf.to_string().into()),
            ("csrf", csrf.to_string().into()),
            ("build", version.build.into()),
            ("version", version.curr_version.to_string().into()),
            ("ts", timestamp.into()),
        ]))
        .send()
        .await?;
    let response: Response<StartLiveResponse> = response.json().await?;
    tracing::info!(
        "Started live stream: RTMP addr {}, code {}, {} protocol(s) available",
        response
            .data
            .as_ref()
            .map(|d| d.rtmp.addr.as_str())
            .unwrap_or("N/A"),
        response
            .data
            .as_ref()
            .map(|d| d.rtmp.code.as_str())
            .unwrap_or("N/A"),
        response
            .data
            .as_ref()
            .map(|d| d.protocols.len())
            .unwrap_or(0)
    );
    response.into_data()
}

#[cfg(test)]
#[test]
fn deserialize_start_live_response() {
    let json_str = r#"{
        "code": 0,
        "data":{
            "change": 1,
            "status": "LIVE",
            "try_time": "0000-00-00 00:00:00",
            "room_type": 0,
            "live_key": "608336837537435443",
            "sub_session_key": "608336837537435443sub_time:1747292297",
            "rtmp":{
                "type": 1,
                "addr": "rtmp://live-push.bilivideo.com/live-bvc/",
                "code": "?streamname=live_348892132_32373699\u0026key=e03061d4a7529d8eaa322dc4d330ca1c\u0026schedule=rtmp\u0026pflag=11",
                "new_link": "https://core.bilivideo.com/video/uplinkcore/selfbuild/schedule?up_rtmp=live-push.bilivideo.com%2Flive-bvc%2F%3Fstreamname%3Dlive_348892132_32373699%26key%3De73061d8a7539d8eaa233dc4d880ca1c%26schedule%3Drtmp%26pflag%3D11\u0026edge=edge",
                "provider": "live"
            },
            "protocols":[
                {
                    "protocol": "rtmp",
                    "addr": "rtmp://live-push.bilivideo.com/live-bvc/","code":"?streamname=live_348892132_32373699\u0026key=e73061d4a1002d8eaa322dc4d880ca1c\u0026schedule=rtmp\u0026pflag=11",
                    "new_link": "https://core.bilivideo.com/video/uplinkcore/selfbuild/schedule?up_rtmp=live-push.bilivideo.com%2Flive-bvc%2F%3Fstreamname%3Dlive_348892132_32373699%26key%3De10298d4a7539d8eaa322dc4d220ca1c%26schedule%3Drtmp%26pflag%3D11\u0026edge=edge",
                    "provider": "txy"
                }
            ],
            "notice":{
                "type": 1,
                "status": 0,
                "title": "",
                "msg": "",
                "button_text": "",
                "button_url": ""
            },
            "qr": "",
            "need_face_auth": false,
            "service_source": "live-streaming",
            "rtmp_backup": null,
            "up_stream_extra":{
                "isp": "电信"
            }
        },
        "message": "",
        "msg": ""
    }"#;
    let response: Response<StartLiveResponse> = serde_json::from_str(json_str).unwrap();
    assert_eq!(response.code, 0);
    assert_eq!(response.message, "");
    let data = response.into_data().unwrap();
    assert_eq!(data.rtmp.addr, "rtmp://live-push.bilivideo.com/live-bvc/");
    assert_eq!(
        data.rtmp.code,
        r"?streamname=live_348892132_32373699&key=e03061d4a7529d8eaa322dc4d330ca1c&schedule=rtmp&pflag=11"
    );
    assert_eq!(data.protocols.len(), 1);
    assert_eq!(data.protocols[0].protocol, "rtmp");
    assert_eq!(
        data.protocols[0].addr,
        "rtmp://live-push.bilivideo.com/live-bvc/"
    );
    assert_eq!(
        data.protocols[0].code,
        r"?streamname=live_348892132_32373699&key=e73061d4a1002d8eaa322dc4d880ca1c&schedule=rtmp&pflag=11"
    );
}

pub async fn stop_live(room_id: u64, csrf: &str) -> Result<()> {
    const URL: &str = "https://api.live.bilibili.com/room/v1/Room/stopLive";
    let client = client();
    let response = client
        .post(URL)
        .form(&[
            ("platform", "pc_link"),
            ("room_id", room_id.to_string().as_str()),
            ("csrf", csrf),
        ])
        .send()
        .await?;
    let json: Response<NoneData> = response.json().await?;
    tracing::info!(
        "Stopped live stream for room_id {}: code {}, message {}",
        room_id,
        json.code,
        json.message
    );
    json.into_data()?;
    Ok(())
}

pub fn app_sign(mut params: Vec<(&str, serde_json::Value)>) -> Vec<(&str, serde_json::Value)> {
    // B站直播姬 App Key
    const APP_KEY: &str = "aae92bc66f3edfab";
    const APP_SEC: &str = "af125a0d5279fd576c1b4418a3e8276d";

    params.push(("appkey", APP_KEY.into()));
    params.sort_by_key(|(k, _)| *k);
    let mut query = serde_urlencoded::to_string(&params).unwrap();
    query.push_str(APP_SEC);

    params.push((
        "sign",
        format!("{:x}", md5::compute(query.as_bytes())).into(),
    ));
    params
}

pub static COOKIE_STORE: LazyLock<Arc<CookieStoreMutex>> = LazyLock::new(|| {
    let store = load_cookies().unwrap_or_default();
    Arc::new(CookieStoreMutex::new(store))
});

fn load_cookies() -> Option<CookieStore> {
    let dirs = directories::ProjectDirs::from("cc", "wybxc", "bili-live-tool")?;
    let path = dirs.data_dir().join("cookies.json");
    let file = std::fs::File::open(&path).ok()?;
    cookie_store::serde::json::load(BufReader::new(file)).ok()
}

pub fn save_cookies() {
    let dirs = match directories::ProjectDirs::from("cc", "wybxc", "bili-live-tool") {
        Some(dirs) => dirs,
        None => return,
    };
    let path = dirs.data_dir().join("cookies.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(mut file) = std::fs::File::create(&path) {
        let _ = cookie_store::serde::json::save(&COOKIE_STORE.lock().unwrap(), &mut file);
    }
}

pub fn clear_cookies() {
    COOKIE_STORE.lock().unwrap().clear();
    save_cookies();
}

pub fn get_csrf_token() -> Option<SmolStr> {
    let store = COOKIE_STORE.lock().unwrap();
    for cookie in store.iter_any() {
        if cookie.name() == "bili_jct" {
            return Some(cookie.value().into());
        }
    }
    None
}

fn client() -> reqwest_middleware::ClientWithMiddleware {
    static CLIENT: LazyLock<reqwest_middleware::ClientWithMiddleware> = LazyLock::new(|| {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
            .no_proxy() // TODO: make this configurable
            .cookie_provider(COOKIE_STORE.clone())
            .build()
            .unwrap();

        reqwest_middleware::ClientBuilder::new(client)
            .with(reqwest_tracing::TracingMiddleware::default())
            .build()
    });

    CLIENT.clone()
}

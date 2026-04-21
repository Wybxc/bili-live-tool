#![allow(dead_code)]

use std::sync::{Arc, LazyLock};

use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
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

pub static COOKIE_STORE: LazyLock<Arc<CookieStoreMutex>> = LazyLock::new(|| {
    let store = CookieStore::new(); // TODO: make this persistent
    Arc::new(CookieStoreMutex::new(store))
});

fn client() -> reqwest_middleware::ClientWithMiddleware {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .no_proxy() // TODO: make this configurable
        .cookie_provider(COOKIE_STORE.clone())
        .build()
        .unwrap();

    reqwest_middleware::ClientBuilder::new(client)
        .with(reqwest_tracing::TracingMiddleware::default())
        .build()
}

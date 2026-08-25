use std::{borrow::Cow, io::Cursor, sync::Arc};

use gpui::*;

pub fn weak_update<T>(
    cx: &mut AsyncWindowContext,
    weak: &WeakEntity<T>,
    f: impl FnOnce(&mut T, &mut Context<T>),
) where
    T: 'static,
{
    let _ = cx.update(|_, cx| {
        weak.update(cx, |this, cx| {
            f(this, cx);
            cx.notify();
        })
    });
}

pub fn weak_update_in<T>(
    cx: &mut AsyncWindowContext,
    weak: &WeakEntity<T>,
    f: impl FnOnce(&mut T, &mut Window, &mut Context<T>),
) where
    T: 'static,
{
    let _ = cx.update(|window, cx| {
        weak.update(cx, |this, cx| {
            f(this, window, cx);
            cx.notify();
        })
    });
}

pub fn weak_emit<T, E>(cx: &mut AsyncWindowContext, weak: &WeakEntity<T>, event: E)
where
    T: EventEmitter<E> + 'static,
    E: 'static,
{
    let _ = cx.update(|_, cx| weak.update(cx, |_, cx| cx.emit(event)));
}

pub fn weak_read<T, R>(
    cx: &mut AsyncWindowContext,
    weak: &WeakEntity<T>,
    f: impl FnOnce(&T) -> R,
) -> Result<R>
where
    T: 'static,
{
    cx.update(|_, cx| weak.read_with(cx, |this, _| f(this)))
        .flatten()
}

pub fn clipboard_copy<'a>(value: impl Into<Cow<'a, str>>) -> SharedString {
    match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(value)) {
        Ok(_) => "复制成功".into(),
        Err(error) => format!("复制失败：{error}").into(),
    }
}

pub fn encode_qr(text: &str) -> Result<Arc<Image>> {
    let image = qrcode::QrCode::new(text)?
        .render::<image::Luma<u8>>()
        .build();
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
}

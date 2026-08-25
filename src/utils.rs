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

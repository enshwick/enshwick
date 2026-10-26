//! Lines and bars on the HUD, drawn each frame at an anchor.

use core::ffi::c_void;
use core::marker::PhantomData;
use std::sync::Mutex;

use super::{Label, client_only};
use crate::abi::{Str, UiFrame, UiWidget};
use crate::{Host, Load, Off};

/// Where on the HUD a line or bar shows; items at one anchor stack in the
/// order drawn.
///
/// # Examples
///
/// See [`Hud`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Anchor {
    /// The top left corner.
    TopLeft,
    /// The top edge, centred.
    TopCenter,
    /// The top right corner.
    #[default]
    TopRight,
    /// The bottom left corner.
    BottomLeft,
    /// The bottom edge, centred.
    BottomCenter,
    /// The bottom right corner.
    BottomRight,
}

impl Anchor {
    fn to_raw(self) -> u32 {
        match self {
            Self::TopLeft => UiWidget::TOP_LEFT,
            Self::TopCenter => UiWidget::TOP_CENTER,
            Self::TopRight => UiWidget::TOP_RIGHT,
            Self::BottomLeft => UiWidget::BOTTOM_LEFT,
            Self::BottomCenter => UiWidget::BOTTOM_CENTER,
            Self::BottomRight => UiWidget::BOTTOM_RIGHT,
        }
    }
}

/// The HUD of one frame, handed to a callback of [`Load::on_hud`]: what it
/// draws shows this frame only, so an item goes away when the mod stops
/// drawing it. Valid for the callback only, on its thread.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Anchor, Label, Style};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_hud(|hud| {
///         hud.text(Anchor::TopRight, Label::new("Raid in 2:00").style(Style::Warning));
///         hud.progress(Anchor::TopRight, 0.4);
///     })?;
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct Hud<'f> {
    host: Host,
    token: u64,
    _life: PhantomData<&'f mut ()>,
    _thread: PhantomData<*const ()>,
}

impl Hud<'_> {
    fn draw(&mut self, mut widget: UiWidget) {
        let _drawn = (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut widget);
    }

    /// Draws a line of text at `anchor`.
    ///
    /// # Examples
    ///
    /// See [`Hud`].
    pub fn text<'t>(&mut self, anchor: Anchor, label: impl Into<Label<'t>>) {
        let label = label.into();
        self.draw(UiWidget {
            kind: UiWidget::LABEL,
            flags: label.flags,
            style: label.style.to_raw(),
            text: Str::new(label.text),
            anchor: anchor.to_raw(),
            ..UiWidget::EMPTY
        });
    }

    /// Draws a bar filled to `fraction`, from 0 to 1, at `anchor`.
    ///
    /// # Examples
    ///
    /// See [`Hud`].
    pub fn progress(&mut self, anchor: Anchor, fraction: f32) {
        self.draw(UiWidget {
            kind: UiWidget::PROGRESS,
            value: if fraction.is_finite() {
                fraction.clamp(0.0, 1.0)
            } else {
                0.0
            },
            min: 0.0,
            max: 1.0,
            anchor: anchor.to_raw(),
            ..UiWidget::EMPTY
        });
    }
}

struct Drawer<F> {
    draw: Mutex<F>,
}

unsafe extern "system" fn draw_hud<F>(frame: *const UiFrame, user: *mut c_void)
where
    F: FnMut(&mut Hud<'_>),
{
    // SAFETY: the loader passes a valid frame for the call.
    let Some(raw) = (unsafe { frame.as_ref() }) else {
        return;
    };
    let Some(host) = crate::host() else {
        return;
    };
    // SAFETY: `user` is the leaked `Drawer<F>` this function was registered
    // with.
    let drawer = unsafe { &*user.cast::<Drawer<F>>() };
    crate::host::guard(
        "panic in a HUD callback",
        || (),
        || {
            let Ok(mut draw) = drawer.draw.lock() else {
                return;
            };
            (draw)(&mut Hud {
                host,
                token: raw.token,
                _life: PhantomData,
                _thread: PhantomData,
            });
        },
    );
}

impl Load<'_> {
    /// Runs `draw` once per frame while the game draws its HUD, on the
    /// thread that draws it; what it draws hides with the HUD, in menus and
    /// in screenshot mode. Client only. A panic in `draw` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server or after `init`.
    ///
    /// # Examples
    ///
    /// See [`Hud`].
    pub fn on_hud<F>(&mut self, draw: F) -> Result<(), Off>
    where
        F: FnMut(&mut Hud<'_>) + Send + 'static,
    {
        let host = self.host();
        client_only(host, "HUD callbacks")?;
        crate::load::hand_over(
            Drawer {
                draw: Mutex::new(draw),
            },
            |user| (host.api().ui_hud)(host.handle(), draw_hud::<F>, user),
            "HUD callback",
        )
    }
}

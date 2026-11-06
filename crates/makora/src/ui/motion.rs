//! Motion shared by the screens, timed by the theme's motion tokens and skipped when the system
//! asks for reduced motion.

use gpui_kit::base::motion::{Transition, TransitionId, transition};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, ElementId, IntoElement, Styled, Window, px,
};

/// How far content rises while it fades in.
const ENTER_RISE: f32 = 6.;

/// Fades `element` in while it rises into place. A new `id` plays it again, so key it by what
/// the element shows, such as the selected tab.
pub fn enter<E>(id: impl Into<ElementId>, element: E, cx: &App) -> AnyElement
where
    E: Styled + IntoElement + 'static,
{
    let element = element.relative();
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    let motion = cx.theme().motion_tokens();
    let easing = motion.easing_enter.clone();
    element
        .with_animation(
            id,
            Animation::new(motion.duration_slow).with_easing(move |t| easing.sample(t)),
            |this, t| this.opacity(t).top(px(ENTER_RISE * (1. - t))),
        )
        .into_any_element()
}

/// Glides a value toward `target` whenever it changes, on the theme's move curve.
pub fn glide(id: impl Into<TransitionId>, target: f32, window: &mut Window, cx: &mut App) -> f32 {
    let motion = cx.theme().motion_tokens();
    let policy = Transition::new(motion.duration_slow).easing(motion.easing_move.clone());
    transition(id, target, policy, window, cx)
}

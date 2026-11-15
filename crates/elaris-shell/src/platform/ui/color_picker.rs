//! The shared theme picker in the space editor.

use super::theme_picker::{PickerAppearance, TintChange, theme_picker};
use elaris_core::{SpaceColors, SpaceId, WindowColor};
use gpui::IntoElement;

pub(super) fn color_picker(
    id: &'static str,
    color: WindowColor,
    colors: SpaceColors,
    space: Option<SpaceId>,
    appearance: PickerAppearance,
    on_change: TintChange,
) -> impl IntoElement {
    theme_picker(id, color, colors, space, on_change).with_staged_appearance(appearance)
}

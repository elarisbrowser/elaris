// SPDX-License-Identifier: GPL-3.0-only

//! Native AppKit host for the shared Site Control Centre presentation.

use objc2_05::rc::Retained;
use objc2_05::{msg_send_id, sel};
use objc2_app_kit_02::{
    NSAccessibility, NSBezelStyle, NSBox, NSBoxType, NSButton, NSImageView, NSLineBreakMode,
    NSView, NSViewController,
};
use objc2_foundation_02::{MainThreadMarker, NSSize, NSString};

use crate::permissions::PermissionState;
use crate::site_controls::{SiteControlAction, SiteControlPresentation};
use crate::ui::site_control_style as design;
use crate::ui::site_control_style::Palette;

use super::actions::{action_target_class, permission_label, permission_symbol};
use super::native::{
    FontWeight, add_hover_tracking, add_subview, hover_button, label, ns_color, rect,
    rounded_surface, set_top_frame, set_top_frame_at, symbol_image, top_frame,
};
use super::{BuiltContent, CORNER_RADIUS, HEIGHT, HoverSurface, WIDTH};

pub(super) fn build_content(data: &SiteControlPresentation, palette: Palette) -> BuiltContent {
    let mtm = MainThreadMarker::new().expect("popover construction runs on AppKit's main thread");
    let target_class = action_target_class();
    // SAFETY: the registered class is an NSView subclass; initWithFrame is valid on main.
    let target: Retained<NSView> = unsafe { msg_send_id![target_class, new] };
    let controller: Retained<NSViewController> =
        unsafe { msg_send_id![mtm.alloc::<NSViewController>(), init] };
    let height = if data.origin.is_some() {
        HEIGHT
    } else {
        design::INTERNAL_HEIGHT as f64
    };
    let content =
        unsafe { NSBox::initWithFrame(mtm.alloc::<NSBox>(), rect(0.0, 0.0, WIDTH, height)) };
    let panel = ns_color(palette.panel);
    let border = ns_color(palette.border);
    // SAFETY: this retained custom box is the content root for the active popover.
    unsafe {
        content.setBoxType(NSBoxType::NSBoxCustom);
        content.setTransparent(false);
        content.setContentViewMargins(NSSize::new(0.0, 0.0));
        content.setFillColor(&panel);
        content.setBorderColor(&border);
        content.setBorderWidth(1.0);
        content.setCornerRadius(CORNER_RADIUS);
    }
    unsafe { controller.setView(&content) };
    let mut statuses = Vec::with_capacity(4);
    let mut hover_surfaces = Vec::with_capacity(8);
    let tile_width =
        (WIDTH - (design::PADDING * 2.0) as f64 - (design::ACTION_GAP * 3.0) as f64) / 4.0;

    for (index, action) in SiteControlAction::ALL.into_iter().enumerate() {
        let image_name = NSString::from_str(action.symbol());
        let image_description = NSString::from_str(action.label());
        // SAFETY: AppKit image lookup is performed on the main thread.
        let Some(image) = symbol_image(
            &image_name,
            &image_description,
            design::GLYPH as f64,
            &ns_color(palette.icon),
        ) else {
            continue;
        };
        let button = hover_button();
        let icon_color = ns_color(palette.icon);
        let tile_x =
            design::PADDING as f64 + index as f64 * (tile_width + design::ACTION_GAP as f64);
        let tile_frame = top_frame(
            tile_x,
            design::PADDING as f64,
            tile_width,
            design::ACTION_HEIGHT as f64,
            height,
        );
        let tile_color = ns_color(palette.tile);
        let tile_hover = ns_color(palette.hover);
        let tile = rounded_surface(mtm, tile_frame, &tile_color, design::ACTION_RADIUS as f64);
        add_subview(&content, &tile);
        hover_surfaces.push(HoverSurface {
            view: tile,
            resting_color: tile_color,
            hover_color: tile_hover,
            hover_only: false,
        });
        unsafe {
            button.setFrame(tile_frame);
            button.setBordered(false);
            button.setImage(Some(&image));
            button.setTarget(Some(&*target));
            button.setAction(Some(sel!(siteAction:)));
            button.setContentTintColor(Some(&icon_color));
            button.setToolTip(Some(&NSString::from_str(action.label())));
            button.setTag(index as isize);
        }
        add_hover_tracking(&button, mtm);
        add_subview(&content, &button);
    }

    let heading = label("This site", 12.0, FontWeight::Semibold, palette.text, mtm);
    set_top_frame_at(
        &heading,
        14.0,
        design::HEADING_TOP as f64,
        WIDTH - 28.0,
        design::HEADING_HEIGHT as f64,
        height,
    );
    add_subview(&content, &heading);

    if let Some(origin) = data.origin.as_deref() {
        for (index, (kind, permission)) in data.permissions.into_iter().enumerate() {
            let top = design::ROWS_TOP as f64
                + index as f64 * (design::ROW_HEIGHT + design::ROW_GAP) as f64;
            let row_surface = rounded_surface(
                mtm,
                top_frame(
                    design::PADDING as f64,
                    top,
                    WIDTH - (design::PADDING * 2.0) as f64,
                    design::ROW_HEIGHT as f64,
                    height,
                ),
                &ns_color(palette.hover),
                design::ROW_RADIUS as f64,
            );
            row_surface.setHidden(true);
            add_subview(&content, &row_surface);
            hover_surfaces.push(HoverSurface {
                view: row_surface,
                resting_color: ns_color(palette.panel),
                hover_color: ns_color(palette.hover),
                hover_only: true,
            });
            let symbol = permission_symbol(kind);
            let image_name = NSString::from_str(symbol);
            let description = NSString::from_str(kind.label());
            let active = permission == PermissionState::Allow;
            let icon_color = ns_color(if active { palette.panel } else { palette.muted });
            let circle_color = ns_color(if active { palette.text } else { palette.tile });
            let icon = symbol_image(
                &image_name,
                &description,
                design::ROW_GLYPH as f64,
                &icon_color,
            );
            if let Some(image) = icon {
                let circle_x = design::PADDING as f64 + 6.0;
                let circle_y = top + (design::ROW_HEIGHT - design::ICON_CIRCLE) as f64 / 2.0;
                let circle_frame = top_frame(
                    circle_x,
                    circle_y,
                    design::ICON_CIRCLE as f64,
                    design::ICON_CIRCLE as f64,
                    height,
                );
                let circle = rounded_surface(
                    mtm,
                    circle_frame,
                    &circle_color,
                    f64::from(design::ICON_CIRCLE) / 2.0,
                );
                add_subview(&content, &circle);
                let glyph_size = design::ROW_GLYPH as f64;
                let glyph = unsafe {
                    NSImageView::initWithFrame(
                        mtm.alloc::<NSImageView>(),
                        top_frame(
                            circle_x + (f64::from(design::ICON_CIRCLE) - glyph_size) / 2.0,
                            circle_y + (f64::from(design::ICON_CIRCLE) - glyph_size) / 2.0,
                            glyph_size,
                            glyph_size,
                            height,
                        ),
                    )
                };
                unsafe {
                    glyph.setImage(Some(&image));
                    glyph.setContentTintColor(Some(&icon_color));
                }
                add_subview(&content, &glyph);
            }
            let text_x = (design::PADDING + 6.0 + design::ICON_CIRCLE + 10.0) as f64;
            let text_width = WIDTH - text_x - design::PADDING as f64 - 6.0;
            let name = label(kind.label(), 12.5, FontWeight::Medium, palette.text, mtm);
            set_top_frame_at(&name, text_x, top + 3.0, text_width, 15.0, height);
            add_subview(&content, &name);
            let status = label(
                permission_label(permission),
                11.5,
                FontWeight::Regular,
                palette.muted,
                mtm,
            );
            set_top_frame_at(&status, text_x, top + 19.0, text_width, 14.0, height);
            add_subview(&content, &status);
            statuses.push(status);

            let row = hover_button();
            unsafe {
                row.setFrame(top_frame(
                    design::PADDING as f64,
                    top,
                    WIDTH - (design::PADDING * 2.0) as f64,
                    design::ROW_HEIGHT as f64,
                    height,
                ));
                // This is a transparent hit target above the custom row contents. A standard
                // push bezel paints a permanent button face in AppKit, unlike Yab's hover-only
                // row wash, and makes the row look selected even when the pointer is elsewhere.
                row.setBordered(false);
                row.setTitle(&NSString::from_str(""));
                row.setTarget(Some(&*target));
                row.setAction(Some(sel!(permissionRow:)));
                let accessibility_label = NSString::from_str(&format!(
                    "{}: {}. Choose Ask, Allow or Block.",
                    kind.label(),
                    permission_label(permission)
                ));
                row.setAccessibilityLabel(Some(&accessibility_label));
                row.setTag((SiteControlAction::ALL.len() + kind.index()) as isize);
            }
            add_hover_tracking(&row, mtm);
            add_subview(&content, &row);
        }

        let divider = unsafe {
            NSBox::initWithFrame(
                mtm.alloc::<NSBox>(),
                top_frame(
                    design::PADDING as f64,
                    design::SEPARATOR_TOP as f64,
                    WIDTH - (design::PADDING * 2.0) as f64,
                    1.0,
                    height,
                ),
            )
        };
        unsafe {
            divider.setBoxType(NSBoxType::NSBoxSeparator);
            divider.setFillColor(&border);
        };
        add_subview(&content, &divider);

        let security = if data.https_scheme { "HTTPS" } else { "HTTP" };
        let chip_width = 74.0;
        let chip = unsafe {
            NSBox::initWithFrame(
                mtm.alloc::<NSBox>(),
                top_frame(
                    design::PADDING as f64,
                    design::FOOTER_TOP as f64,
                    chip_width,
                    design::FOOTER_HEIGHT as f64,
                    height,
                ),
            )
        };
        unsafe {
            chip.setBoxType(NSBoxType::NSBoxCustom);
            chip.setTransparent(false);
            chip.setContentViewMargins(NSSize::new(0.0, 0.0));
            chip.setFillColor(&ns_color(palette.tile));
            chip.setBorderWidth(0.0);
            chip.setCornerRadius(8.0);
            chip.setToolTip(Some(&NSString::from_str(
                "URL scheme only; certificate status is unavailable",
            )));
        }
        add_subview(&content, &chip);

        if let Some(image) = symbol_image(
            &NSString::from_str("info.circle"),
            &NSString::from_str(security),
            10.0,
            &ns_color(palette.text),
        ) {
            let mark = unsafe {
                NSImageView::initWithFrame(
                    mtm.alloc::<NSImageView>(),
                    top_frame(
                        design::PADDING as f64 + 10.0,
                        design::FOOTER_TOP as f64 + 9.0,
                        10.0,
                        10.0,
                        height,
                    ),
                )
            };
            unsafe {
                mark.setImage(Some(&image));
                mark.setContentTintColor(Some(&ns_color(palette.text)));
                mark.setToolTip(Some(&NSString::from_str(
                    "URL scheme only; certificate status is unavailable",
                )));
            }
            add_subview(&content, &mark);
        }
        let security_label = label(security, 12.0, FontWeight::Medium, palette.text, mtm);
        set_top_frame_at(
            &security_label,
            design::PADDING as f64 + 26.0,
            design::FOOTER_TOP as f64 + 5.0,
            40.0,
            18.0,
            height,
        );
        add_subview(&content, &security_label);

        let host = origin.rsplit("//").next().unwrap_or(origin);
        let host_field = label(host, 11.5, FontWeight::Regular, palette.muted, mtm);
        let host_x = design::PADDING as f64 + chip_width + 8.0;
        let more_x = WIDTH - 36.0;
        let host_width = more_x - host_x - 8.0;
        set_top_frame_at(
            &host_field,
            host_x,
            design::FOOTER_TOP as f64 + 5.0,
            host_width,
            18.0,
            height,
        );
        unsafe {
            host_field.setUsesSingleLineMode(true);
            host_field.setMaximumNumberOfLines(1);
            host_field.setLineBreakMode(NSLineBreakMode::NSLineBreakByTruncatingTail);
            host_field.setToolTip(Some(&NSString::from_str(host)));
        }
        add_subview(&content, &host_field);

        let more_description = NSString::from_str("More site actions");
        if let Some(image) = symbol_image(
            &NSString::from_str("ellipsis"),
            &more_description,
            13.0,
            &ns_color(palette.icon),
        ) {
            let more = unsafe {
                NSButton::buttonWithImage_target_action(
                    &image,
                    Some(&*target),
                    Some(sel!(siteOverflow:)),
                    mtm,
                )
            };
            unsafe {
                more.setFrame(top_frame(
                    WIDTH - 36.0,
                    design::FOOTER_TOP as f64,
                    28.0,
                    design::FOOTER_HEIGHT as f64,
                    height,
                ));
                more.setBezelStyle(NSBezelStyle::AccessoryBarAction);
                more.setBordered(false);
                more.setContentTintColor(Some(&ns_color(palette.icon)));
                more.setToolTip(Some(&more_description));
            }
            add_subview(&content, &more);
        }
    } else {
        let message = if data.url.is_some() {
            "Elaris pages do not use website permissions."
        } else {
            "Open a website to manage its permissions."
        };
        let message = label(message, 13.0, FontWeight::Regular, palette.muted, mtm);
        set_top_frame(
            &message,
            design::HEADING_TOP as f64 + design::HEADING_HEIGHT as f64 + 6.0,
            WIDTH - (design::PADDING * 2.0) as f64,
            36.0,
            height,
        );
        add_subview(&content, &message);
    }

    BuiltContent {
        controller,
        statuses,
        hover_surfaces,
        target,
        height,
    }
}

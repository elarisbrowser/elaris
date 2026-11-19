// SPDX-License-Identifier: GPL-3.0-only

//! Native AppKit host for the shared Site Control Centre presentation.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2_05::rc::Retained;
use objc2_05::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2_05::{ClassType, msg_send, msg_send_id, sel};
use objc2_app_kit_02::{
    NSAccessibility, NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSBezelStyle,
    NSBox, NSBoxType, NSButton, NSColor, NSEvent, NSEventMask, NSFont, NSFontWeightMedium,
    NSFontWeightRegular, NSFontWeightSemibold, NSImage, NSImageSymbolConfiguration, NSImageView,
    NSLineBreakMode, NSMenu, NSMenuItem, NSPopover, NSPopoverBehavior, NSTextField, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSViewController,
};
use objc2_foundation_02::{MainThreadMarker, NSPoint, NSRect, NSRectEdge, NSSize, NSString};
use rune::WindowHandle as RuneWindowHandle;
use rune::raw_window_handle::RawWindowHandle;

use crate::app::{self, AppState};
use crate::permissions::{PermissionKind, PermissionState};
use crate::site_controls::{SiteControlAction, SiteControlPresentation};
use crate::ui::site_control_style as design;
use crate::ui::site_control_style::Palette;

const WIDTH: f64 = design::WIDTH as f64;
const HEIGHT: f64 = design::HEIGHT as f64;
const CORNER_RADIUS: f64 = design::OUTER_RADIUS as f64;

struct Host {
    anchor_view: Retained<NSView>,
    popover: Retained<NSPopover>,
    controller: Option<Retained<NSViewController>>,
    presentation: Option<SiteControlPresentation>,
    statuses: Vec<Retained<NSTextField>>,
    hover_surfaces: Vec<HoverSurface>,
    action_target: Option<Retained<NSView>>,
    _escape_monitor: Option<Retained<AnyObject>>,
}

struct HoverSurface {
    view: Retained<NSBox>,
    resting_color: Retained<NSColor>,
    hover_color: Retained<NSColor>,
    hover_only: bool,
}

struct BuiltContent {
    controller: Retained<NSViewController>,
    statuses: Vec<Retained<NSTextField>>,
    hover_surfaces: Vec<HoverSurface>,
    target: Retained<NSView>,
    height: f64,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
    static APP_HANDLE: RefCell<Option<rune::AppHandle<AppState>>> = const { RefCell::new(None) };
}

/// Retains an AppKit popover and the Rune canvas used as its coordinate-space anchor.
pub(crate) fn install(window: &RuneWindowHandle, app: rune::AppHandle<AppState>) {
    let result = window.with_raw_handle(|handle| {
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: Rune owns this AppKit view for the window lifetime; this callback runs on main.
        let Some(anchor_view) =
            (unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) })
        else {
            return;
        };
        // SAFETY: AppKit objects are created on Rune's main/AppKit thread.
        let mtm = MainThreadMarker::new().expect("window hooks run on AppKit's main thread");
        let popover: Retained<NSPopover> = unsafe { msg_send_id![mtm.alloc::<NSPopover>(), init] };
        let escape_block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // AppKit delivers key events on its main thread. Consume Escape only while the
            // Site Control Centre is visible; leave every other key event untouched.
            let escape_pressed = unsafe { event.as_ref().keyCode() == 53 };
            let closed = escape_pressed
                && HOST.with(|slot| {
                    let Ok(mut slot) = slot.try_borrow_mut() else {
                        return false;
                    };
                    let Some(host) = slot.as_mut() else {
                        return false;
                    };
                    if !unsafe { host.popover.isShown() } {
                        return false;
                    }
                    close_host(host);
                    true
                });
            if closed {
                std::ptr::null_mut()
            } else {
                event.as_ptr()
            }
        });
        // SAFETY: AppKit retains the local monitor block until its returned token is removed.
        let escape_monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown,
                &escape_block,
            )
        };
        // SAFETY: these setters are called on the popover's AppKit thread.
        unsafe {
            popover.setBehavior(NSPopoverBehavior::Transient);
            popover.setAnimates(true);
        }
        HOST.with(|host| {
            *host.borrow_mut() = Some(Host {
                anchor_view,
                popover,
                controller: None,
                presentation: None,
                statuses: Vec::new(),
                hover_surfaces: Vec::new(),
                action_target: None,
                _escape_monitor: escape_monitor,
            });
        });
        APP_HANDLE.with(|slot| *slot.borrow_mut() = Some(app));
    });
    if let Err(error) = result {
        eprintln!("Elaris: cannot attach site popover to Rune window: {error}");
    }
}

/// Opens the native popover at the Rune button's current logical-point bounds.
pub(crate) fn show(data: SiteControlPresentation, anchor: [f32; 4], palette: Palette) -> bool {
    let mut shown = false;
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        let Some(host) = host.as_mut() else {
            return;
        };
        // Native transient dismissal handles outside clicks; the local key monitor handles Escape.
        if unsafe { host.popover.isShown() } {
            close_host(host);
            return;
        }

        // SAFETY: AppKit exports these process-lifetime appearance-name constants.
        let appearance_name = unsafe {
            if palette.dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        let appearance = NSAppearance::appearanceNamed(appearance_name);
        // SAFETY: the native popover and appearance are retained on AppKit's main thread.
        unsafe { host.popover.setAppearance(appearance.as_deref()) };
        let BuiltContent {
            controller,
            statuses,
            hover_surfaces,
            target,
            height,
        } = build_content(&data, palette);
        // SAFETY: the content controller and popover belong to this AppKit thread.
        unsafe {
            host.popover.setContentViewController(Some(&controller));
            host.popover.setContentSize(NSSize::new(WIDTH, height));
        }
        host.controller = Some(controller);
        host.statuses = statuses;
        host.hover_surfaces = hover_surfaces;
        host.action_target = Some(target);
        host.presentation = Some(data);

        let rect = anchor_rect(&host.anchor_view, anchor);
        // SAFETY: anchor view and popover belong to the current live window on AppKit's thread.
        unsafe {
            host.popover.showRelativeToRect_ofView_preferredEdge(
                rect,
                &host.anchor_view,
                NSRectEdge::NSMaxYEdge,
            );
        }
        shown = true;
    });
    shown
}

/// Updates the attached button bounds after Rune relayouts the sidebar or window.
pub(crate) fn update_anchor(anchor: [f32; 4]) {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        let Some(host) = host.as_mut() else {
            return;
        };
        if !unsafe { host.popover.isShown() } {
            return;
        }
        let rect = anchor_rect(&host.anchor_view, anchor);
        // SAFETY: the popover and anchor view are retained by Host on AppKit's main thread.
        unsafe { host.popover.setPositioningRect(rect) };
    });
}

/// Closes the transient popover if the active page or its tab changes.
pub(crate) fn close() {
    HOST.with(|host| {
        if let Some(host) = host.borrow_mut().as_mut() {
            close_host(host);
        }
    });
}

fn close_host(host: &mut Host) {
    // SAFETY: the popover is retained by Host and this call is on AppKit's main thread.
    unsafe { host.popover.performClose(None) };
    host.presentation = None;
}

fn build_content(data: &SiteControlPresentation, palette: Palette) -> BuiltContent {
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
        let tile_frame = top_frame(
            design::PADDING as f64
                + index as f64
                    * ((WIDTH
                        - (design::PADDING * 2.0) as f64
                        - (design::ACTION_GAP * 3.0) as f64)
                        / 4.0
                        + design::ACTION_GAP as f64),
            design::PADDING as f64,
            (WIDTH - (design::PADDING * 2.0) as f64 - (design::ACTION_GAP * 3.0) as f64) / 4.0,
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

fn add_subview(parent: &NSView, child: &NSView) {
    // SAFETY: both views are retained and AppKit mutates this hierarchy on its main thread.
    unsafe { parent.addSubview(child) };
}

fn rounded_surface(
    mtm: MainThreadMarker,
    frame: NSRect,
    fill: &NSColor,
    radius: f64,
) -> Retained<NSBox> {
    let surface = unsafe { NSBox::initWithFrame(mtm.alloc::<NSBox>(), frame) };
    // SAFETY: the custom box is retained and styled on AppKit's main thread.
    unsafe {
        surface.setBoxType(NSBoxType::NSBoxCustom);
        surface.setTransparent(false);
        surface.setContentViewMargins(NSSize::new(0.0, 0.0));
        surface.setFillColor(fill);
        surface.setBorderWidth(0.0);
        surface.setCornerRadius(radius);
    }
    surface
}

fn hover_button() -> Retained<NSButton> {
    // SAFETY: the registered NSButton subclass only overrides AppKit mouse tracking callbacks.
    unsafe { msg_send_id![hover_button_class(), new] }
}

fn add_hover_tracking(button: &NSButton, mtm: MainThreadMarker) {
    let options = NSTrackingAreaOptions::NSTrackingMouseEnteredAndExited
        | NSTrackingAreaOptions::NSTrackingActiveInKeyWindow
        | NSTrackingAreaOptions::NSTrackingInVisibleRect;
    // SAFETY: AppKit retains the tracking area when attached; InVisibleRect follows frame changes.
    let tracking_area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            mtm.alloc::<NSTrackingArea>(),
            rect(0.0, 0.0, 0.0, 0.0),
            options,
            Some(button),
            None,
        )
    };
    // SAFETY: both objects belong to AppKit's main thread and the button owns its tracking area.
    unsafe { button.addTrackingArea(&tracking_area) };
}

fn label(
    value: &str,
    size: f64,
    weight: FontWeight,
    foreground: rune::Color,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let text = NSString::from_str(value);
    // SAFETY: label creation and style changes run on AppKit's main thread.
    let field = unsafe { NSTextField::labelWithString(&text, mtm) };
    let font = font(size, weight);
    let color = ns_color(foreground);
    unsafe {
        field.setFont(Some(&font));
        field.setTextColor(Some(&color));
    }
    field
}

#[derive(Clone, Copy)]
enum FontWeight {
    Regular,
    Medium,
    Semibold,
}

fn font(size: f64, weight: FontWeight) -> Retained<NSFont> {
    // SAFETY: AppKit font construction runs on its main thread.
    unsafe {
        let weight = match weight {
            FontWeight::Regular => NSFontWeightRegular,
            FontWeight::Medium => NSFontWeightMedium,
            FontWeight::Semibold => NSFontWeightSemibold,
        };
        NSFont::systemFontOfSize_weight(size, weight)
    }
}

fn symbol_image(
    name: &NSString,
    description: &NSString,
    size: f64,
    tint: &NSColor,
) -> Option<Retained<NSImage>> {
    // Bake the semantic foreground into the SF Symbol. NSButton's content tint does not reliably
    // reach symbol images in our AppKit host, even when they are marked as templates.
    let source = unsafe {
        NSImage::imageWithSystemSymbolName_accessibilityDescription(name, Some(description))
    }?;
    let configuration =
        unsafe { NSImageSymbolConfiguration::configurationWithHierarchicalColor(tint) };
    let image = unsafe { source.imageWithSymbolConfiguration(&configuration) }?;
    unsafe {
        image.setTemplate(false);
        image.setSize(NSSize::new(size, size));
    }
    Some(image)
}

fn permission_menu(target: &AnyObject, sender: &AnyObject, kind: PermissionKind) {
    let mtm = MainThreadMarker::new().expect("permission menus run on AppKit's main thread");
    let menu =
        unsafe { NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str("Permission")) };
    let selected = HOST.with(|host| {
        host.borrow()
            .as_ref()
            .and_then(|host| host.presentation.as_ref())
            .map(|data| data.permissions[kind.index()].1)
    });
    for (index, (permission, title)) in [
        (PermissionState::Ask, "Ask"),
        (PermissionState::Allow, "Allow"),
        (PermissionState::Block, "Block"),
    ]
    .into_iter()
    .enumerate()
    {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(title),
                Some(sel!(permissionSelected:)),
                &NSString::from_str(""),
            )
        };
        unsafe {
            item.setTarget(Some(target));
            item.setTag((kind.index() * 3 + index) as isize);
            if Some(permission) == selected {
                item.setState(1);
            }
            menu.addItem(&item);
        }
    }
    // SAFETY: this native row button and its menu are live on AppKit's main thread.
    unsafe {
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, 0.0),
            Some(&*(std::ptr::from_ref(sender).cast::<NSView>())),
        );
    }
    let _ = mtm;
}

fn overflow_menu(target: &AnyObject, sender: &AnyObject) {
    let mtm = MainThreadMarker::new().expect("native menus run on AppKit's main thread");
    let menu = unsafe { NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str("Site")) };
    for (tag, title) in [
        (0, "Reset permissions for this site"),
        (1, "Copy page address"),
    ] {
        if tag == 1
            && HOST.with(|host| {
                host.borrow()
                    .as_ref()
                    .and_then(|host| host.presentation.as_ref())
                    .and_then(|data| data.url.as_ref())
                    .is_none()
            })
        {
            continue;
        }
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(title),
                Some(sel!(siteOverflowSelected:)),
                &NSString::from_str(""),
            )
        };
        unsafe {
            item.setTarget(Some(target));
            item.setTag(tag);
            menu.addItem(&item);
        }
    }
    // SAFETY: the menu and its in-popover button are live on AppKit's main thread.
    unsafe {
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, 0.0),
            Some(&*(std::ptr::from_ref(sender).cast::<NSView>())),
        );
    }
}

unsafe extern "C" fn site_action(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let Some(action) = SiteControlAction::ALL.get(tag.max(0) as usize).copied() else {
        return;
    };
    let data = HOST.with(|host| {
        host.borrow()
            .as_ref()
            .and_then(|host| host.presentation.clone())
    });
    let Some(data) = data else { return };
    APP_HANDLE.with(|handle| {
        if let Some(handle) = handle.borrow().as_ref() {
            let should_close = action == SiteControlAction::Settings;
            handle.update(move |state, tx| match action {
                SiteControlAction::CopyUrl => {
                    if let Some(url) = data.url.as_deref() {
                        tx.copy(url);
                    }
                }
                SiteControlAction::Reload => {
                    if data.tab == state.browser.active_tab().map(|tab| tab.id())
                        && let Some(tab) = data.tab
                    {
                        state.browser.reload(tab);
                    }
                }
                SiteControlAction::DuplicateTab => {
                    if data.tab == state.browser.active_tab().map(|tab| tab.id())
                        && let Some(tab) = data.tab
                    {
                        state.duplicate(tab);
                    }
                }
                SiteControlAction::Settings => app::open_settings(state, tx),
            });
            if should_close {
                close();
            }
        }
    });
}

unsafe extern "C" fn permission_row(target: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let Some(kind_index) = tag.checked_sub(SiteControlAction::ALL.len() as isize) else {
        return;
    };
    let Some(kind) = PermissionKind::ALL.get(kind_index as usize).copied() else {
        return;
    };
    permission_menu(target, sender, kind);
}

unsafe extern "C" fn permission_selected(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    if tag < 0 {
        return;
    }
    let kind = PermissionKind::ALL[(tag as usize / 3).min(3)];
    let permission = match tag as usize % 3 {
        0 => PermissionState::Ask,
        1 => PermissionState::Allow,
        _ => PermissionState::Block,
    };
    let context = HOST.with(|host| {
        let host = host.borrow();
        let host = host.as_ref()?;
        Some((
            host.presentation.as_ref()?.origin.clone()?,
            host.statuses.get(kind.index())?.clone(),
        ))
    });
    let Some((origin, status)) = context else {
        return;
    };
    APP_HANDLE.with(|handle| {
        if let Some(handle) = handle.borrow().as_ref() {
            let expected_origin = origin.clone();
            handle.update(move |state, _| {
                if state.current_permission_origin().as_deref() == Some(&expected_origin) {
                    state.set_site_permission(kind, permission);
                }
            });
        }
    });
    let label = NSString::from_str(permission_label(permission));
    unsafe { status.setStringValue(&label) };
    HOST.with(|host| {
        if let Some(data) = host
            .borrow_mut()
            .as_mut()
            .and_then(|host| host.presentation.as_mut())
        {
            data.permissions[kind.index()].1 = permission;
        }
    });
}

unsafe extern "C" fn site_overflow(target: &AnyObject, _: Sel, sender: &AnyObject) {
    overflow_menu(target, sender);
}

unsafe extern "C" fn site_overflow_selected(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let data = HOST.with(|host| {
        host.borrow()
            .as_ref()
            .and_then(|host| host.presentation.clone())
    });
    let Some(data) = data else { return };
    if tag == 0 {
        let Some(origin) = data.origin.clone() else {
            return;
        };
        APP_HANDLE.with(|handle| {
            if let Some(handle) = handle.borrow().as_ref() {
                handle.update(move |state, _| {
                    if state.current_permission_origin().as_deref() == Some(&origin) {
                        state.reset_site_permissions();
                    }
                });
            }
        });
        for status in HOST.with(|host| {
            host.borrow()
                .as_ref()
                .map(|host| host.statuses.clone())
                .unwrap_or_default()
        }) {
            unsafe { status.setStringValue(&NSString::from_str("Ask")) };
        }
    } else if let Some(url) = data.url {
        APP_HANDLE.with(|handle| {
            if let Some(handle) = handle.borrow().as_ref() {
                handle.update(move |_, tx| tx.copy(url));
            }
        });
    }
    close();
}

unsafe extern "C" fn hover_entered(button: &AnyObject, _: Sel, _: &AnyObject) {
    let tag: isize = unsafe { msg_send![button, tag] };
    update_hover_surface(tag, true);
}

unsafe extern "C" fn hover_exited(button: &AnyObject, _: Sel, _: &AnyObject) {
    let tag: isize = unsafe { msg_send![button, tag] };
    update_hover_surface(tag, false);
}

fn update_hover_surface(tag: isize, hovered: bool) {
    let Ok(index) = usize::try_from(tag) else {
        return;
    };
    HOST.with(|slot| {
        let host = slot.borrow();
        let Some(surface) = host
            .as_ref()
            .and_then(|host| host.hover_surfaces.get(index))
        else {
            return;
        };
        // SAFETY: tracking callbacks and their views are on AppKit's main thread.
        unsafe {
            if surface.hover_only {
                surface.view.setHidden(!hovered);
            } else {
                surface.view.setFillColor(if hovered {
                    &surface.hover_color
                } else {
                    &surface.resting_color
                });
            }
        }
    });
}

fn hover_button_class() -> &'static AnyClass {
    const NAME: &str = "ElarisSiteControlHoverButton";
    if let Some(class) = AnyClass::get(NAME) {
        return class;
    }
    let mut builder =
        ClassBuilder::new(NAME, NSButton::class()).expect("register site popover hover button");
    // SAFETY: AppKit calls these NSResponder mouse-tracking callbacks with the event argument.
    unsafe {
        builder.add_method(
            sel!(mouseEntered:),
            hover_entered as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(mouseExited:),
            hover_exited as unsafe extern "C" fn(_, _, _),
        );
    }
    builder.register()
}

fn action_target_class() -> &'static AnyClass {
    const NAME: &str = "ElarisSiteControlActionTarget";
    if let Some(class) = AnyClass::get(NAME) {
        return class;
    }
    let mut builder = ClassBuilder::new(NAME, NSView::class())
        .expect("register site popover action target class");
    // SAFETY: selectors use AppKit's target/action ABI: receiver, selector, sender.
    unsafe {
        builder.add_method(
            sel!(siteAction:),
            site_action as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(permissionRow:),
            permission_row as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(permissionSelected:),
            permission_selected as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(siteOverflow:),
            site_overflow as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(siteOverflowSelected:),
            site_overflow_selected as unsafe extern "C" fn(_, _, _),
        );
    }
    builder.register()
}

fn permission_symbol(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Microphone => "mic",
        PermissionKind::Camera => "camera",
        PermissionKind::Location => "location",
        PermissionKind::Notifications => "bell",
    }
}

fn permission_label(permission: PermissionState) -> &'static str {
    match permission {
        PermissionState::Ask => "Ask",
        PermissionState::Allow => "Allowed",
        PermissionState::Block => "Blocked for this site",
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

fn anchor_rect(view: &NSView, rune_rect: [f32; 4]) -> NSRect {
    // Rune reports window-point rectangles from the top-left. Winit's AppKit content view is
    // flipped, so its local coordinates use that same origin; keep the conversion correct if
    // the host view ever changes. Both APIs use logical points, not backing pixels.
    let bounds = view.bounds();
    let height = f64::from(rune_rect[3]);
    let y = if view.isFlipped() {
        bounds.origin.y + f64::from(rune_rect[1])
    } else {
        bounds.origin.y + bounds.size.height - f64::from(rune_rect[1]) - height
    };
    rect(
        bounds.origin.x + f64::from(rune_rect[0]),
        y,
        f64::from(rune_rect[2]),
        height,
    )
}

fn top_frame(x: f64, top: f64, width: f64, height: f64, content_height: f64) -> NSRect {
    rect(x, content_height - top - height, width, height)
}

fn set_top_frame(view: &NSView, top: f64, width: f64, height: f64, content_height: f64) {
    set_top_frame_at(view, 12.0, top, width, height, content_height);
}

fn set_top_frame_at(view: &NSView, x: f64, top: f64, width: f64, height: f64, content_height: f64) {
    // SAFETY: `view` is owned by the popover controller and this runs on AppKit's main thread.
    unsafe { view.setFrame(top_frame(x, top, width, height, content_height)) };
}

fn ns_color(color: rune::Color) -> Retained<NSColor> {
    // Rune stores linear sRGB; AppKit's sRGB initializer expects encoded components.
    unsafe {
        NSColor::colorWithSRGBRed_green_blue_alpha(
            rune::color::srgb_encode(color.r) as f64,
            rune::color::srgb_encode(color.g) as f64,
            rune::color::srgb_encode(color.b) as f64,
            color.a as f64,
        )
    }
}

//! Popup menus drawn in the dock's color.
//!
//! A user-chosen `dock_color` makes the dock's and launcher's right-click
//! menus owner-drawn so they match the dock. Without one, menus stay the
//! standard Windows ones. While a menu's modal loop runs, the dock's state
//! is borrowed, so the colors and labels live here and the window procedures
//! hand `WM_MEASUREITEM` and `WM_DRAWITEM` to [`handle`] before they borrow anything.

use std::cell::RefCell;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_GRAYED, ODS_SELECTED, ODT_MENU,
};
use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PWSTR;

use crate::theme::{Palette, Tint};

/// A menu's colors, as GDI wants them.
#[derive(Clone, Copy)]
pub struct Style {
    background: COLORREF,
    hover: COLORREF,
    text: COLORREF,
    disabled: COLORREF,
    separator: COLORREF,
    dpi: u32,
}

impl Style {
    /// The style for a surface in `palette`, or None without a dock color.
    pub fn new(palette: &Palette, tint: Option<Tint>, dpi: u32) -> Option<Self> {
        tint?;
        let background = palette.acrylic_tint;
        let text = D2D1_COLOR_F { a: 1.0, ..palette.text };
        Some(Self {
            background: colorref(background),
            hover: colorref(mix(background, text, 0.12)),
            text: colorref(text),
            disabled: colorref(mix(background, text, 0.42)),
            separator: colorref(mix(background, text, 0.16)),
            dpi: dpi.max(96),
        })
    }

    fn px(&self, dip: i32) -> i32 {
        dip * self.dpi as i32 / 96
    }
}

fn mix(a: D2D1_COLOR_F, b: D2D1_COLOR_F, t: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

fn colorref(c: D2D1_COLOR_F) -> COLORREF {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    COLORREF(byte(c.r) | byte(c.g) << 8 | byte(c.b) << 16)
}

struct Item {
    /// The label before a tab, and the shortcut after it.
    label: Vec<u16>,
    shortcut: Vec<u16>,
    separator: bool,
}

struct Active {
    style: Style,
    items: Vec<Item>,
    font: HFONT,
    brush: HBRUSH,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// Shows `menu` at `point` and returns the chosen command, owner-drawn in
/// `style` when there is one. `TPM_RETURNCMD` is always added.
pub fn track(menu: HMENU, style: Option<Style>, flags: TRACK_POPUP_MENU_FLAGS, point: (i32, i32), owner: HWND) -> i32 {
    if let Some(style) = style {
        unsafe { prepare(menu, style) };
    }
    let command = unsafe { TrackPopupMenu(menu, flags | TPM_RETURNCMD, point.0, point.1, None, owner, None).0 };
    if let Some(active) = ACTIVE.with(|cell| cell.borrow_mut().take()) {
        unsafe {
            let _ = DeleteObject(active.font.into());
            let _ = DeleteObject(active.brush.into());
        }
    }
    command
}

unsafe fn prepare(menu: HMENU, style: Style) {
    unsafe {
        let mut items = Vec::new();
        let count = GetMenuItemCount(Some(menu)).max(0) as u32;
        for position in 0..count {
            let mut buffer = [0u16; 256];
            let mut info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_STRING,
                dwTypeData: PWSTR(buffer.as_mut_ptr()),
                cch: buffer.len() as u32,
                ..Default::default()
            };
            if GetMenuItemInfoW(menu, position, true, &mut info).is_err() {
                continue;
            }
            let separator = info.fType.contains(MFT_SEPARATOR);
            let text = &buffer[..info.cch as usize];
            let (label, shortcut) = match text.iter().position(|&c| c == u16::from(b'\t')) {
                Some(tab) => (text[..tab].to_vec(), text[tab + 1..].to_vec()),
                None => (text.to_vec(), Vec::new()),
            };
            let owner_drawn = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_DATA,
                fType: MFT_OWNERDRAW | if separator { MFT_SEPARATOR } else { MENU_ITEM_TYPE(0) },
                dwItemData: items.len(),
                ..Default::default()
            };
            let _ = SetMenuItemInfoW(menu, position, true, &owner_drawn);
            items.push(Item {
                label,
                shortcut,
                separator,
            });
        }
        // The menu's own padding around the items takes the same color.
        let brush = CreateSolidBrush(style.background);
        let info = MENUINFO {
            cbSize: size_of::<MENUINFO>() as u32,
            fMask: MIM_BACKGROUND,
            hbrBack: brush,
            ..Default::default()
        };
        let _ = SetMenuInfo(menu, &info);
        ACTIVE.with(|cell| {
            *cell.borrow_mut() = Some(Active {
                style,
                items,
                font: menu_font(style.dpi),
                brush,
            })
        });
    }
}

/// The system menu font at `dpi`, so the menu matches standard ones in size.
fn menu_font(dpi: u32) -> HFONT {
    unsafe {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        if SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some((&raw mut metrics).cast()),
            0,
            dpi,
        )
        .is_ok()
        {
            CreateFontIndirectW(&metrics.lfMenuFont)
        } else {
            HFONT(GetStockObject(DEFAULT_GUI_FONT).0)
        }
    }
}

/// Measures and draws owner-drawn menu items. Returns None for other messages.
pub fn handle(msg: u32, _wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if msg != WM_MEASUREITEM && msg != WM_DRAWITEM {
        return None;
    }
    ACTIVE.with(|cell| {
        let active = cell.try_borrow().ok()?;
        let active = active.as_ref()?;
        unsafe {
            if msg == WM_MEASUREITEM {
                let measure = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
                if measure.CtlType != ODT_MENU {
                    return None;
                }
                let item = active.items.get(measure.itemData)?;
                measure_item(active, item, measure);
            } else {
                let draw = &*(lparam.0 as *const DRAWITEMSTRUCT);
                if draw.CtlType != ODT_MENU {
                    return None;
                }
                let item = active.items.get(draw.itemData)?;
                draw_item(active, item, draw);
            }
        }
        Some(LRESULT(1))
    })
}

unsafe fn measure_item(active: &Active, item: &Item, measure: &mut MEASUREITEMSTRUCT) {
    let style = &active.style;
    if item.separator {
        measure.itemHeight = style.px(9) as u32;
        measure.itemWidth = 0;
        return;
    }
    unsafe {
        let dc = GetDC(None);
        let old = SelectObject(dc, active.font.into());
        let extent = |text: &[u16]| {
            let mut size = SIZE::default();
            let _ = GetTextExtentPoint32W(dc, text, &mut size);
            size
        };
        let label = extent(&item.label);
        let shortcut = if item.shortcut.is_empty() {
            0
        } else {
            extent(&item.shortcut).cx + style.px(28)
        };
        SelectObject(dc, old);
        ReleaseDC(None, dc);
        measure.itemWidth = (label.cx + shortcut + style.px(24)) as u32;
        measure.itemHeight = (label.cy + style.px(14)) as u32;
    }
}

unsafe fn draw_item(active: &Active, item: &Item, draw: &DRAWITEMSTRUCT) {
    let style = &active.style;
    let dc = draw.hDC;
    let rect = draw.rcItem;
    let disabled = (draw.itemState.0 & ODS_GRAYED.0 != 0) || (draw.itemState.0 & ODS_DISABLED.0 != 0);
    let hovered = (draw.itemState.0 & ODS_SELECTED.0 != 0) && !disabled && !item.separator;
    unsafe {
        let fill = |r: &RECT, color: COLORREF| {
            let brush = CreateSolidBrush(color);
            FillRect(dc, r, brush);
            let _ = DeleteObject(brush.into());
        };
        fill(&rect, style.background);
        if item.separator {
            let y = (rect.top + rect.bottom) / 2;
            let line = RECT {
                left: rect.left + style.px(10),
                top: y,
                right: rect.right - style.px(10),
                bottom: y + style.px(1).max(1),
            };
            fill(&line, style.separator);
            return;
        }
        if hovered {
            // A rounded highlight inset from the edges, like Windows 11 menus.
            let brush = CreateSolidBrush(style.hover);
            let pen = CreatePen(PS_NULL, 0, COLORREF(0));
            let old_brush = SelectObject(dc, brush.into());
            let old_pen = SelectObject(dc, pen.into());
            let radius = style.px(8);
            let _ = RoundRect(
                dc,
                rect.left + style.px(4),
                rect.top + style.px(1),
                rect.right - style.px(4),
                rect.bottom - style.px(1),
                radius,
                radius,
            );
            SelectObject(dc, old_brush);
            SelectObject(dc, old_pen);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(pen.into());
        }
        let old_font = SelectObject(dc, active.font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, if disabled { style.disabled } else { style.text });
        let mut text_rect = RECT {
            left: rect.left + style.px(12),
            right: rect.right - style.px(12),
            ..rect
        };
        let mut label = item.label.clone();
        DrawTextW(
            dc,
            &mut label,
            &mut text_rect,
            DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX,
        );
        if !item.shortcut.is_empty() {
            SetTextColor(dc, style.disabled);
            let mut shortcut = item.shortcut.clone();
            DrawTextW(
                dc,
                &mut shortcut,
                &mut text_rect,
                DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_NOPREFIX,
            );
        }
        SelectObject(dc, old_font);
    }
}

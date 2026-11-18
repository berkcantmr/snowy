//! A small, non-activating notification bubble: an icon, a short message and a row of keycaps.
//!
//! The window is a click-through, never-focused layered popup. Everything is drawn once per
//! `show` into an off-screen bitmap (plain GDI, ClearType text); `WM_PAINT` only blits it, and
//! fades are done with the whole-window alpha so no repaint is needed while animating.

use anyhow::{anyhow, Result};
use std::sync::Once;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;

/// A key shown as a keycap icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Win,
    Ctrl,
    Alt,
    Shift,
    Char(char),
}

/// Glyph at the left of the bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastIcon {
    Lock,
    EditOn,
    EditOff,
}

// ---- timing -------------------------------------------------------------------------------

const FADE_IN: Duration = Duration::from_millis(120);
const HOLD: Duration = Duration::from_millis(3500);
const FADE_OUT: Duration = Duration::from_millis(250);

/// Whole-window alpha (0..=255) `t` into the bubble's life; `None` once it is over.
fn alpha_at(t: Duration) -> Option<u8> {
    let frac = |part: Duration, whole: Duration| part.as_secs_f32() / whole.as_secs_f32();
    if t < FADE_IN {
        Some((255.0 * frac(t, FADE_IN)) as u8)
    } else if t < FADE_IN + HOLD {
        Some(255)
    } else if t < FADE_IN + HOLD + FADE_OUT {
        Some((255.0 * (1.0 - frac(t - FADE_IN - HOLD, FADE_OUT))) as u8)
    } else {
        None
    }
}

// ---- palette (matches the dark settings theme) ----------------------------------------------

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}
const BG: COLORREF = rgb(34, 41, 53);
const BORDER: COLORREF = rgb(52, 62, 79);
const TEXT: COLORREF = rgb(234, 239, 246);
const SECONDARY: COLORREF = rgb(156, 168, 186);
const ACCENT: COLORREF = rgb(125, 190, 250);
const CAP_FILL: COLORREF = rgb(25, 30, 39);
const CAP_BORDER: COLORREF = rgb(70, 82, 102);
const CAP_EDGE: COLORREF = rgb(14, 17, 23);

// ---- pure layout math -------------------------------------------------------------------------

/// All sizes in physical pixels for one DPI scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Metrics {
    pad: i32,
    icon: i32,
    icon_gap: i32,
    max_text_w: i32,
    inline_gap: i32,
    row_gap: i32,
    cap_h: i32,
    cap_min_w: i32,
    cap_pad: i32,
    cap_edge: i32,
    sep_w: i32,
    corner: i32,
    body_px: i32,
    cap_px: i32,
    icon_px: i32,
    anchor_gap: i32,
    margin: i32,
}

impl Metrics {
    fn new(scale: f32) -> Self {
        let s = |v: f32| (v * scale).round() as i32;
        Self {
            pad: s(14.0),
            icon: s(22.0),
            icon_gap: s(12.0),
            max_text_w: s(340.0),
            inline_gap: s(14.0),
            row_gap: s(10.0),
            cap_h: s(26.0),
            cap_min_w: s(26.0),
            cap_pad: s(8.0),
            cap_edge: s(2.0).max(1),
            sep_w: s(14.0),
            corner: s(6.0),
            body_px: s(14.5),
            cap_px: s(13.0),
            icon_px: s(20.0),
            anchor_gap: s(14.0),
            margin: s(16.0),
        }
    }
}

/// Width of a keycap whose legend is `content_w` wide.
fn cap_width(m: &Metrics, content_w: i32) -> i32 {
    (content_w + 2 * m.cap_pad).max(m.cap_min_w)
}

/// Total width of a row of caps joined by "+" separators.
fn keys_row_width(m: &Metrics, caps: &[i32]) -> i32 {
    if caps.is_empty() {
        return 0;
    }
    caps.iter().sum::<i32>() + (caps.len() as i32 - 1) * m.sep_w
}

/// Where everything goes inside the bubble (client coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    size: (i32, i32),
    icon_pos: (i32, i32),
    text_pos: (i32, i32),
    keys_pos: (i32, i32),
}

/// Combine the measured text block and keycap row into the bubble geometry. The keycaps go to the
/// right of the text when the message is a single line and everything fits `max_text_w`;
/// otherwise they sit on their own row below.
fn compute_layout(m: &Metrics, text: (i32, i32), line_h: i32, keys_w: i32) -> Layout {
    let (tw, th) = text;
    let x0 = m.pad + m.icon + m.icon_gap;
    let single = th <= line_h * 3 / 2;
    let inline = keys_w > 0 && single && tw + m.inline_gap + keys_w <= m.max_text_w;

    let (body_w, body_h, text_y, keys_y, keys_x);
    if inline {
        body_w = tw + m.inline_gap + keys_w;
        body_h = th.max(m.cap_h);
        text_y = (body_h - th) / 2;
        keys_y = (body_h - m.cap_h) / 2;
        keys_x = x0 + tw + m.inline_gap;
    } else {
        body_w = tw.max(keys_w);
        body_h = if keys_w > 0 { th + m.row_gap + m.cap_h } else { th };
        text_y = 0;
        keys_y = th + m.row_gap;
        keys_x = x0;
    }
    let content_h = body_h.max(m.icon);
    let top = m.pad + (content_h - body_h) / 2;
    // The icon sits level with the first text line when the body is taller than the icon.
    let icon_y = if body_h > m.icon { top + text_y + (line_h - m.icon) / 2 } else { m.pad };
    Layout {
        size: (x0 + body_w + m.pad, content_h + 2 * m.pad),
        icon_pos: (m.pad, icon_y.max(m.pad.min(top))),
        text_pos: (x0, top + text_y),
        keys_pos: (keys_x, top + keys_y),
    }
}

/// Place a `size` bubble just above `anchor` (centred), flipping below if there is no room, and
/// clamp it inside `work`.
fn place(anchor: (i32, i32), size: (i32, i32), work: RECT, gap: i32) -> (i32, i32) {
    let mut x = anchor.0 - size.0 / 2;
    let mut y = anchor.1 - gap - size.1;
    if y < work.top {
        y = anchor.1 + gap;
    }
    x = x.min(work.right - size.0).max(work.left);
    y = y.min(work.bottom - size.1).max(work.top);
    (x, y)
}

/// Bottom-right corner of `work` with a margin.
fn place_corner(size: (i32, i32), work: RECT, margin: i32) -> (i32, i32) {
    ((work.right - size.0 - margin).max(work.left), (work.bottom - size.1 - margin).max(work.top))
}

// ---- GDI helpers ------------------------------------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

/// Owned GDI font.
struct Font(HFONT);

impl Font {
    fn new(face: &str, px: i32, weight: i32) -> Self {
        let mut name = wide(face);
        name.push(0);
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
        let f = unsafe {
            CreateFontW(
                -px,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                PCWSTR(name.as_ptr()),
            )
        };
        Font(f)
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: we own the font and it is no longer selected anywhere when dropped.
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

/// Face name GDI actually resolved `font` to (GDI silently substitutes missing fonts).
unsafe fn resolved_face(dc: HDC, font: &Font) -> String {
    let old = SelectObject(dc, font.0.into());
    let mut buf = [0u16; 64];
    let n = GetTextFaceW(dc, Some(&mut buf)).max(0) as usize;
    SelectObject(dc, old);
    String::from_utf16_lossy(&buf[..n.saturating_sub(1).min(buf.len())])
}

/// The icon font: Segoe Fluent Icons (Win11), else Segoe MDL2 Assets.
unsafe fn icon_font(dc: HDC, px: i32) -> Font {
    let fluent = Font::new("Segoe Fluent Icons", px, 400);
    if resolved_face(dc, &fluent).eq_ignore_ascii_case("Segoe Fluent Icons") {
        return fluent;
    }
    Font::new("Segoe MDL2 Assets", px, 400)
}

unsafe fn text_size(dc: HDC, font: &Font, s: &str) -> (i32, i32) {
    let old = SelectObject(dc, font.0.into());
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, &wide(s), &mut sz);
    SelectObject(dc, old);
    (sz.cx, sz.cy)
}

unsafe fn draw_text(dc: HDC, font: &Font, s: &str, r: RECT, color: COLORREF, flags: DRAW_TEXT_FORMAT) {
    let old = SelectObject(dc, font.0.into());
    SetTextColor(dc, color);
    let mut r = r;
    let mut buf = wide(s);
    DrawTextW(dc, &mut buf, &mut r, flags | DT_NOPREFIX);
    SelectObject(dc, old);
}

unsafe fn fill(dc: HDC, r: RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, &r, brush);
    let _ = DeleteObject(brush.into());
}

/// Rounded rectangle with the given fill and (optional) 1px border colour.
unsafe fn round_rect(dc: HDC, r: RECT, radius: i32, fill_c: COLORREF, border: Option<COLORREF>) {
    let brush = CreateSolidBrush(fill_c);
    let pen = match border {
        Some(c) => CreatePen(PS_SOLID, 1, c),
        None => HPEN(GetStockObject(NULL_PEN).0),
    };
    let (ob, op) = (SelectObject(dc, brush.into()), SelectObject(dc, pen.into()));
    // RoundRect excludes the right/bottom edge; with a pen it is drawn inside the extents.
    let extra = if border.is_some() { 0 } else { 1 };
    let _ = RoundRect(dc, r.left, r.top, r.right + extra, r.bottom + extra, radius * 2, radius * 2);
    SelectObject(dc, ob);
    SelectObject(dc, op);
    if border.is_some() {
        let _ = DeleteObject(pen.into());
    }
    let _ = DeleteObject(brush.into());
}

fn icon_glyph(icon: ToastIcon) -> &'static str {
    match icon {
        ToastIcon::Lock => "\u{E72E}",
        ToastIcon::EditOn => "\u{E70F}",
        ToastIcon::EditOff => "\u{E73E}",
    }
}

/// Legend text of a key (`None` for the Windows logo, which is drawn as shapes).
fn key_legend(key: Key) -> Option<String> {
    match key {
        Key::Win => None,
        Key::Ctrl => Some("Ctrl".into()),
        Key::Alt => Some("Alt".into()),
        Key::Shift => Some("\u{21E7}".into()),
        Key::Char(c) => Some(c.to_uppercase().collect()),
    }
}

struct Fonts {
    body: Font,
    cap: Font,
    shift: Font,
    icon: Font,
}

/// A fully drawn bubble.
struct Rendered {
    bitmap: HBITMAP,
    size: (i32, i32),
}

/// Measure and draw the bubble into an off-screen bitmap.
unsafe fn render(m: &Metrics, icon: ToastIcon, text: &str, keys: &[Key]) -> Result<Rendered> {
    let screen = GetDC(None);
    let dc = CreateCompatibleDC(Some(screen));
    if dc.is_invalid() {
        ReleaseDC(None, screen);
        return Err(anyhow!("CreateCompatibleDC failed"));
    }
    let result = render_in(dc, screen, m, icon, text, keys);
    let _ = DeleteDC(dc);
    ReleaseDC(None, screen);
    result
}

unsafe fn render_in(dc: HDC, screen: HDC, m: &Metrics, icon: ToastIcon, text: &str, keys: &[Key]) -> Result<Rendered> {
    let fonts = Fonts {
        body: Font::new("Segoe UI", m.body_px, 400),
        cap: Font::new("Segoe UI", m.cap_px, 600),
        shift: Font::new("Segoe UI Symbol", m.body_px, 400),
        icon: icon_font(dc, m.icon_px),
    };
    SetBkMode(dc, TRANSPARENT);

    // Measure the wrapped message.
    let old = SelectObject(dc, fonts.body.0.into());
    let mut tr = rect(0, 0, m.max_text_w, 0);
    let mut buf = wide(text);
    DrawTextW(dc, &mut buf, &mut tr, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | DT_LEFT);
    SelectObject(dc, old);
    let line_h = text_size(dc, &fonts.body, "Ag").1.max(1);
    let text_dim = (tr.right - tr.left, (tr.bottom - tr.top).min(line_h * 3));

    // Measure keycaps.
    let cap_font = |k: Key| if k == Key::Shift { &fonts.shift } else { &fonts.cap };
    let caps: Vec<i32> = keys
        .iter()
        .map(|&k| match key_legend(k) {
            Some(s) => cap_width(m, text_size(dc, cap_font(k), &s).0),
            None => m.cap_min_w,
        })
        .collect();
    let keys_w = keys_row_width(m, &caps);
    let lay = compute_layout(m, text_dim, line_h, keys_w);

    // Draw.
    let (w, h) = lay.size;
    let bitmap = CreateCompatibleBitmap(screen, w, h);
    if bitmap.is_invalid() {
        return Err(anyhow!("CreateCompatibleBitmap failed"));
    }
    let prev = SelectObject(dc, bitmap.into());
    fill(dc, rect(0, 0, w, h), BG);

    let (ix, iy) = lay.icon_pos;
    draw_text(
        dc,
        &fonts.icon,
        icon_glyph(icon),
        rect(ix, iy, ix + m.icon, iy + m.icon),
        ACCENT,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP,
    );
    let (tx, ty) = lay.text_pos;
    draw_text(dc, &fonts.body, text, rect(tx, ty, tx + text_dim.0 + 2, ty + text_dim.1), TEXT, DT_LEFT | DT_WORDBREAK);

    let (mut kx, ky) = lay.keys_pos;
    for (i, (&k, &cw)) in keys.iter().zip(&caps).enumerate() {
        if i > 0 {
            draw_text(
                dc,
                &fonts.body,
                "+",
                rect(kx, ky, kx + m.sep_w, ky + m.cap_h - m.cap_edge),
                SECONDARY,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            kx += m.sep_w;
        }
        draw_keycap(dc, m, &fonts, k, rect(kx, ky, kx + cw, ky + m.cap_h));
        kx += cw;
    }

    SelectObject(dc, prev);
    Ok(Rendered { bitmap, size: lay.size })
}

/// One physical-looking key: dark bottom edge, lighter face with a 1px border.
unsafe fn draw_keycap(dc: HDC, m: &Metrics, fonts: &Fonts, key: Key, r: RECT) {
    round_rect(dc, r, m.corner, CAP_EDGE, Some(CAP_BORDER));
    let face = rect(r.left + 1, r.top + 1, r.right - 1, r.bottom - m.cap_edge - 1);
    round_rect(dc, face, (m.corner - 1).max(1), CAP_FILL, None);
    let face = rect(r.left, r.top, r.right, r.bottom - m.cap_edge);
    match key_legend(key) {
        Some(s) => {
            let font = if key == Key::Shift { &fonts.shift } else { &fonts.cap };
            draw_text(dc, font, &s, face, TEXT, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);
        }
        None => {
            // Windows logo: 2x2 grid of small squares.
            let cell = ((face.bottom - face.top) as f32 * 0.20).round().max(3.0) as i32;
            let gap = (cell / 5).max(1);
            let total = cell * 2 + gap;
            let x = face.left + (face.right - face.left - total) / 2;
            let y = face.top + (face.bottom - face.top - total) / 2;
            for (cx, cy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (px, py) = (x + cx * (cell + gap), y + cy * (cell + gap));
                fill(dc, rect(px, py, px + cell, py + cell), TEXT);
            }
        }
    }
}

// ---- window -----------------------------------------------------------------------------------

static REGISTER: Once = Once::new();
const CLASS: PCWSTR = w!("SnowyToast");

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let bmp = HBITMAP(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut _);
            if !dc.is_invalid() && !bmp.is_invalid() {
                let mem = CreateCompatibleDC(Some(dc));
                let old = SelectObject(mem, bmp.into());
                let (w, h) = (ps.rcPaint.right - ps.rcPaint.left, ps.rcPaint.bottom - ps.rcPaint.top);
                let _ = BitBlt(dc, ps.rcPaint.left, ps.rcPaint.top, w, h, Some(mem), ps.rcPaint.left, ps.rcPaint.top, SRCCOPY);
                SelectObject(mem, old);
                let _ = DeleteDC(mem);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn register_class() {
    REGISTER.call_once(|| unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            log::warn!("toast: RegisterClassW failed");
        }
    });
}

/// Monitor containing `pt` (nearest if none): its work area and DPI scale.
unsafe fn monitor_info(pt: (i32, i32)) -> (RECT, f32) {
    let mon = MonitorFromPoint(POINT { x: pt.0, y: pt.1 }, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let work = if GetMonitorInfoW(mon, &mut mi).as_bool() { mi.rcWork } else { rect(0, 0, 1920, 1040) };
    let (mut dx, mut dy) = (96u32, 96u32);
    if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() || dx == 0 {
        dx = 96;
    }
    (work, dx as f32 / 96.0)
}

/// The notification bubble. Create once, then `show` / `tick`.
pub struct Toast {
    hwnd: HWND,
    bitmap: Option<HBITMAP>,
    /// When the current bubble started (shifted back when it replaces a visible one).
    started: Option<Instant>,
    alpha: u8,
}

impl Toast {
    /// Creates the (hidden) window. Call on the thread that pumps messages.
    pub fn new() -> Result<Self> {
        register_class();
        // SAFETY: plain window creation with a class registered above.
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
                CLASS,
                w!("Snowy"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hinst.into()),
                None,
            )?;
            // Rounded corners and a themed border (Windows 11; failures on Windows 10 are fine).
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const _,
                std::mem::size_of_val(&corner) as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &BORDER as *const _ as *const _,
                std::mem::size_of::<COLORREF>() as u32,
            );
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
            Ok(Self { hwnd, bitmap: None, started: None, alpha: 0 })
        }
    }

    /// Shows (or replaces) the bubble just above `anchor` (physical pixels), or in the
    /// bottom-right of the primary work area when `None`.
    pub fn show(&mut self, icon: ToastIcon, text: &str, keys: &[Key], anchor: Option<(i32, i32)>) {
        // SAFETY: GDI/user32 calls on handles we own; failures are logged and the call aborted.
        unsafe {
            // (0, 0) always lies on (or nearest to) the primary monitor.
            let (work, scale) = monitor_info(anchor.unwrap_or((0, 0)));
            let m = Metrics::new(scale);
            let r = match render(&m, icon, text, keys) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("toast: render failed: {e}");
                    return;
                }
            };
            let (x, y) = match anchor {
                Some(a) => place(a, r.size, work, m.anchor_gap),
                None => place_corner(r.size, work, m.margin),
            };
            let was_visible = self.is_visible();
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, r.bitmap.0 as isize);
            if let Some(old) = self.bitmap.replace(r.bitmap) {
                let _ = DeleteObject(old.into());
            }
            if let Err(e) = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, r.size.0, r.size.1, SWP_NOACTIVATE) {
                log::warn!("toast: SetWindowPos failed: {e}");
            }
            // A replaced bubble stays fully opaque instead of fading in again.
            let now = Instant::now();
            self.started = Some(if was_visible { now.checked_sub(FADE_IN).unwrap_or(now) } else { now });
            self.set_alpha(if was_visible { 255 } else { 0 });
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn set_alpha(&mut self, a: u8) {
        // SAFETY: valid window handle owned by self.
        unsafe {
            if let Err(e) = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), a, LWA_ALPHA) {
                log::warn!("toast: SetLayeredWindowAttributes failed: {e}");
            }
        }
        self.alpha = a;
    }

    /// Call every frame: fade-in, hold, fade-out, then hide.
    pub fn tick(&mut self) {
        let Some(start) = self.started else { return };
        match alpha_at(start.elapsed()) {
            Some(a) => {
                if a != self.alpha {
                    self.set_alpha(a);
                }
            }
            None => self.hide(),
        }
    }

    pub fn hide(&mut self) {
        if self.started.take().is_some() {
            // SAFETY: valid window handle owned by self.
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    pub fn is_visible(&self) -> bool {
        self.started.is_some()
    }
}

impl Drop for Toast {
    fn drop(&mut self) {
        // SAFETY: we own the window and the bitmap; the window is destroyed before the bitmap.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            if let Some(b) = self.bitmap.take() {
                let _ = DeleteObject(b.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: RECT = RECT { left: 0, top: 0, right: 1000, bottom: 600 };

    #[test]
    fn caps_have_minimum_width() {
        let m = Metrics::new(1.0);
        assert_eq!(cap_width(&m, 5), m.cap_min_w);
        assert_eq!(cap_width(&m, 30), 30 + 2 * m.cap_pad);
    }

    #[test]
    fn keys_row_includes_separators() {
        let m = Metrics::new(1.0);
        assert_eq!(keys_row_width(&m, &[]), 0);
        assert_eq!(keys_row_width(&m, &[26]), 26);
        assert_eq!(keys_row_width(&m, &[26, 30, 26]), 82 + 2 * m.sep_w);
    }

    #[test]
    fn metrics_scale_with_dpi() {
        assert_eq!(Metrics::new(2.0).pad, 2 * Metrics::new(1.0).pad);
    }

    #[test]
    fn short_text_puts_keys_inline() {
        let m = Metrics::new(1.0);
        let l = compute_layout(&m, (120, 20), 20, 100);
        assert_eq!(l.keys_pos.0, m.pad + m.icon + m.icon_gap + 120 + m.inline_gap);
        assert_eq!(l.size.1, m.cap_h.max(m.icon) + 2 * m.pad);
    }

    #[test]
    fn long_text_stacks_keys_below() {
        let m = Metrics::new(1.0);
        let l = compute_layout(&m, (340, 40), 20, 100);
        assert_eq!(l.keys_pos.0, l.text_pos.0);
        assert!(l.keys_pos.1 >= l.text_pos.1 + 40 + m.row_gap);
        assert_eq!(l.size.1, 40 + m.row_gap + m.cap_h + 2 * m.pad);
    }

    #[test]
    fn no_keys_means_no_row() {
        let m = Metrics::new(1.0);
        let l = compute_layout(&m, (200, 40), 20, 0);
        assert_eq!(l.size.1, 40 + 2 * m.pad);
    }

    #[test]
    fn anchor_places_above_and_centred() {
        assert_eq!(place((500, 300), (200, 60), WORK, 10), (400, 230));
    }

    #[test]
    fn anchor_flips_below_near_top() {
        assert_eq!(place((500, 20), (200, 60), WORK, 10), (400, 30));
    }

    #[test]
    fn anchor_clamped_into_work_area() {
        assert_eq!(place((5, 300), (200, 60), WORK, 10).0, 0);
        assert_eq!(place((995, 300), (200, 60), WORK, 10).0, 800);
        assert_eq!(place((500, 590), (200, 60), WORK, 10).1, 520);
    }

    #[test]
    fn corner_placement_uses_margin() {
        assert_eq!(place_corner((200, 60), WORK, 16), (784, 524));
    }

    #[test]
    fn alpha_curve() {
        assert_eq!(alpha_at(Duration::ZERO), Some(0));
        assert_eq!(alpha_at(Duration::from_millis(1000)), Some(255));
        let mid = alpha_at(FADE_IN + HOLD + FADE_OUT / 2).unwrap();
        assert!((120..135).contains(&mid));
        assert_eq!(alpha_at(Duration::from_secs(10)), None);
    }
}

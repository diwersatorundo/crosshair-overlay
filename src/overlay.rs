use crate::logging;
use crate::shared::Shared;
use crosshair_core::config::{Config, Rgba};
use crosshair_core::geometry::{build_scene, Point, Scene, Shape};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_FILL_MODE_WINDING, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Brush, ID2D1DCRenderTarget, ID2D1Factory, ID2D1PathGeometry,
    ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_ALIASED,
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
    GetMonitorInfoW, MonitorFromPoint, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_X,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClassNameW, GetClientRect,
    GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    KillTimer, PostQuitMessage, RegisterClassW, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, UpdateLayeredWindow, CW_USEDEFAULT, HTTRANSPARENT, HWND_TOPMOST, MSG,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WM_DISPLAYCHANGE, WM_ERASEBKGND, WM_HOTKEY, WM_NCHITTEST, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows_numerics::{Matrix3x2, Vector2};

const WINDOW_CLASS: PCWSTR = w!("CrosshairOverlayWindow");
const WINDOW_TITLE: PCWSTR = w!("Crosshair Overlay");
const HOTKEY_ID: i32 = 1;

thread_local! {
    static STATE: RefCell<Option<OverlayState>> = const { RefCell::new(None) };
}

pub fn spawn(shared: Arc<Shared>) {
    let _ = thread::Builder::new()
        .name("crosshair-overlay".to_string())
        .spawn(move || {
            logging::log("INFO", "Overlay thread started");
            loop {
                match run_overlay(&shared) {
                    Ok(()) => {
                        shared.set_overlay_running(false);
                        logging::log("INFO", "Overlay stopped");
                        break;
                    }
                    Err(err) => {
                        shared.set_overlay_running(false);
                        shared.set_overlay_error(Some(err.clone()));
                        logging::log("ERROR", format!("Overlay failure: {err}"));
                        thread::sleep(Duration::from_secs(2));
                    }
                }
            }
        });
}

fn run_overlay(shared: &Arc<Shared>) -> Result<(), String> {
    unsafe {
        let com_initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        let result = run_overlay_inner(shared);
        if com_initialized {
            CoUninitialize();
        }
        result
    }
}

unsafe fn run_overlay_inner(shared: &Arc<Shared>) -> Result<(), String> {
    let class_name = w!("CrosshairOverlayWindow");
    let hinstance = {
        let module = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        HINSTANCE(module.0)
    };

    let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
    wc.lpfnWndProc = Some(window_proc);
    wc.hInstance = hinstance;
    wc.lpszClassName = class_name;

    let registered = unsafe { RegisterClassW(&wc) };
    if registered == 0 {
        let error = unsafe { GetLastError() };
        if error.0 != 1410 {
            return Err(format!("RegisterClassW: {error:?}"));
        }
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            WINDOW_CLASS,
            WINDOW_TITLE,
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            64,
            64,
            None,
            None,
            Some(hinstance),
            None,
        )
        .map_err(|e| format!("CreateWindowExW: {e}"))?
    };

    let factory: ID2D1Factory =
        unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None::<*const _>) }
            .map_err(|e| format!("D2D1CreateFactory: {e}"))?;

    let snapshot = shared.snapshot();
    let mut state = OverlayState::new(
        hwnd,
        shared.clone(),
        factory,
        snapshot.config,
        snapshot.version,
    )?;

    let hotkey = unsafe {
        RegisterHotKey(
            Some(hwnd),
            HOTKEY_ID,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            VK_X.0 as u32,
        )
    };
    match hotkey {
        Ok(()) => {
            shared.set_hotkey_error(None);
            state.hotkey_registered = true;
        }
        Err(err) => {
            let message = format!("Ctrl+Alt+X недоступен: {err}");
            logging::log("WARN", &message);
            shared.set_hotkey_error(Some(message));
        }
    }

    STATE.with(|cell| {
        *cell.borrow_mut() = Some(state);
    });
    shared.set_overlay_running(true);
    shared.set_overlay_error(None);
    logging::log("INFO", "Overlay window ready");

    unsafe {
        SetTimer(Some(hwnd), 1, 50, None);
    }

    let mut message = MSG::default();
    let mut fatal_error = None;
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            if result.0 < 0 {
                fatal_error = Some("GetMessageW failed".to_string());
            }
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        fatal_error = STATE.with(|cell| {
            cell.borrow()
                .as_ref()
                .and_then(|state| state.fatal_error.clone())
        });
        if fatal_error.is_some() {
            break;
        }
    }

    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().take() {
            if state.hotkey_registered {
                unsafe {
                    let _ = UnregisterHotKey(Some(state.hwnd), HOTKEY_ID);
                }
            }
        }
    });
    unsafe {
        let _ = KillTimer(Some(hwnd), 1);
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = DestroyWindow(hwnd);
    }

    if let Some(err) = fatal_error {
        Err(err)
    } else {
        Ok(())
    }
}

struct OverlayState {
    hwnd: HWND,
    shared: Arc<Shared>,
    factory: ID2D1Factory,
    renderer: Option<Renderer>,
    config: Config,
    version: u64,
    dirty: bool,
    visible: bool,
    content_ready: bool,
    hotkey_registered: bool,
    last_center: (i32, i32),
    last_scale_height: i32,
    last_size: i32,
    fatal_error: Option<String>,
}

impl OverlayState {
    fn new(
        hwnd: HWND,
        shared: Arc<Shared>,
        factory: ID2D1Factory,
        config: Config,
        version: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            hwnd,
            shared,
            factory,
            renderer: None,
            config,
            version,
            dirty: true,
            visible: false,
            content_ready: false,
            hotkey_registered: false,
            last_center: (i32::MIN, i32::MIN),
            last_scale_height: -1,
            last_size: 0,
            fatal_error: None,
        })
    }

    fn tick(&mut self) -> Result<(), String> {
        let snapshot = self.shared.snapshot();
        if snapshot.version != self.version {
            self.version = snapshot.version;
            self.config = snapshot.config;
            self.dirty = true;
        }

        if !self.config.enabled {
            self.hide();
            return Ok(());
        }

        let Some(target) = (unsafe { foreground_target(&self.config) }) else {
            self.hide();
            return Ok(());
        };

        let scene = build_scene(&self.config, target.client_height as f32, 1.0);
        let half = (scene.extent + 6.0).ceil().max(32.0);
        let size = ((half * 2.0) as i32)
            .clamp(64, target.monitor_width.min(target.monitor_height).max(64));
        let center = target.center;
        let left = center.0 - size / 2;
        let top = center.1 - size / 2;

        let scale_changed = self.last_scale_height != target.client_height;
        let size_changed = self.last_size != size;
        if self.renderer.is_none() || size_changed {
            self.renderer = Some(Renderer::new(&self.factory, size)?);
            self.content_ready = false;
        }

        if self.dirty || scale_changed || size_changed || !self.content_ready {
            let renderer = self
                .renderer
                .as_ref()
                .ok_or_else(|| "Renderer is not initialized".to_string())?;
            unsafe { renderer.render(&self.factory, &scene, size)? };
            self.content_ready = true;
            self.dirty = false;
            self.last_scale_height = target.client_height;
            self.last_size = size;
            self.update_layered(left, top, size)?;
            if !self.visible {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                }
                self.visible = true;
            }
        } else {
            let moved = self.last_center != center || size_changed;
            if moved {
                unsafe {
                    let _ = SetWindowPos(
                        self.hwnd,
                        Some(HWND_TOPMOST),
                        left,
                        top,
                        size,
                        size,
                        SWP_NOACTIVATE,
                    );
                }
            } else {
                unsafe {
                    let _ = SetWindowPos(
                        self.hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE,
                    );
                }
            }
            if !self.visible {
                self.update_layered(left, top, size)?;
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                }
                self.visible = true;
            }
        }

        self.last_center = center;
        self.shared.set_overlay_error(None);
        Ok(())
    }

    fn update_layered(&self, left: i32, top: i32, size: i32) -> Result<(), String> {
        let renderer = self
            .renderer
            .as_ref()
            .ok_or_else(|| "Renderer is not initialized".to_string())?;
        let screen_dc = unsafe { GetDC(None) };
        if screen_dc.is_invalid() {
            return Err("GetDC failed".to_string());
        }
        let dst = POINT { x: left, y: top };
        let window_size = SIZE { cx: size, cy: size };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let result = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                Some(screen_dc),
                Some(&dst),
                Some(&window_size),
                Some(renderer.mem_dc),
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        unsafe {
            ReleaseDC(None, screen_dc);
        }
        result.map_err(|e| format!("UpdateLayeredWindow: {e}"))
    }

    fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    fn handle_hotkey(&mut self) {
        let (config, version) = self.shared.toggle_enabled();
        self.config = config.clone();
        self.version = version;
        self.dirty = true;
        if let Err(err) = self.shared.save(&config) {
            logging::log("ERROR", err);
        }
        if let Err(err) = self.tick() {
            self.fatal_error = Some(err);
            unsafe { PostQuitMessage(1) };
        }
    }
}

struct Renderer {
    target: Option<ID2D1DCRenderTarget>,
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
}

impl Renderer {
    fn new(factory: &ID2D1Factory, size: i32) -> Result<Self, String> {
        unsafe {
            let mem_dc = CreateCompatibleDC(None);
            if mem_dc.is_invalid() {
                return Err("CreateCompatibleDC failed".to_string());
            }

            let bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size,
                    biHeight: -size,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [Default::default()],
            };
            let mut bits: *mut c_void = null_mut();
            let bitmap = match CreateDIBSection(
                Some(mem_dc),
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            ) {
                Ok(bitmap) => bitmap,
                Err(err) => {
                    let _ = DeleteDC(mem_dc);
                    return Err(format!("CreateDIBSection: {err}"));
                }
            };
            let old_bitmap = SelectObject(mem_dc, bitmap.into());

            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = match factory.CreateDCRenderTarget(&props) {
                Ok(target) => target,
                Err(err) => {
                    SelectObject(mem_dc, old_bitmap);
                    let _ = DeleteObject(bitmap.into());
                    let _ = DeleteDC(mem_dc);
                    return Err(format!("CreateDCRenderTarget: {err}"));
                }
            };
            let rect = RECT {
                left: 0,
                top: 0,
                right: size,
                bottom: size,
            };
            if let Err(err) = target.BindDC(mem_dc, &rect) {
                drop(target);
                SelectObject(mem_dc, old_bitmap);
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(mem_dc);
                return Err(format!("BindDC: {err}"));
            }

            Ok(Self {
                target: Some(target),
                mem_dc,
                bitmap,
                old_bitmap,
            })
        }
    }

    unsafe fn render(
        &self,
        factory: &ID2D1Factory,
        scene: &Scene,
        size: i32,
    ) -> Result<(), String> {
        let target = self
            .target
            .as_ref()
            .ok_or_else(|| "D2D target missing".to_string())?;

        target.BeginDraw();
        target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
        target.Clear(Some(&D2D1_COLOR_F {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }));
        target.SetTransform(&Matrix3x2::translation(
            size as f32 * 0.5,
            size as f32 * 0.5,
        ));

        let draw_result = (|| -> Result<(), String> {
            for shape in &scene.shapes {
                match shape {
                    Shape::Fill { polygons, color } => {
                        let brush = solid_brush(target, *color)?;
                        let geometry = build_geometry(factory, polygons)?;
                        target.FillGeometry(&geometry, &brush, None::<&ID2D1Brush>);
                    }
                    Shape::Ring {
                        center,
                        radius,
                        width,
                        color,
                    } => {
                        target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                        let brush = solid_brush(target, *color)?;
                        let ellipse = D2D1_ELLIPSE {
                            point: Vector2::new(center.x, center.y),
                            radiusX: *radius,
                            radiusY: *radius,
                        };
                        target.DrawEllipse(&ellipse, &brush, *width, None::<&ID2D1StrokeStyle>);
                        target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
                    }
                }
            }
            Ok(())
        })();

        target.SetTransform(&Matrix3x2::identity());
        let end_result = target
            .EndDraw(None, None)
            .map_err(|e| format!("EndDraw: {e}"));
        draw_result.and(end_result)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.target.take();
            SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

unsafe fn solid_brush(
    target: &ID2D1RenderTarget,
    color: Rgba,
) -> Result<ID2D1SolidColorBrush, String> {
    let d2d_color = D2D1_COLOR_F {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: color.a as f32 / 255.0,
    };
    unsafe {
        target
            .CreateSolidColorBrush(&d2d_color, None)
            .map_err(|e| format!("CreateSolidColorBrush: {e}"))
    }
}

unsafe fn build_geometry(
    factory: &ID2D1Factory,
    polygons: &[Vec<Point>],
) -> Result<ID2D1PathGeometry, String> {
    unsafe {
        let geometry = factory
            .CreatePathGeometry()
            .map_err(|e| format!("CreatePathGeometry: {e}"))?;
        let sink = geometry
            .Open()
            .map_err(|e| format!("PathGeometry::Open: {e}"))?;
        sink.SetFillMode(D2D1_FILL_MODE_WINDING);
        for polygon in polygons {
            if polygon.len() < 3 {
                continue;
            }
            let points: Vec<Vector2> = polygon
                .iter()
                .map(|point| Vector2::new(point.x, point.y))
                .collect();
            sink.BeginFigure(points[0], D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLines(&points[1..]);
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
        sink.Close()
            .map_err(|e| format!("GeometrySink::Close: {e}"))?;
        Ok(geometry)
    }
}

struct ForegroundTarget {
    center: (i32, i32),
    client_height: i32,
    monitor_width: i32,
    monitor_height: i32,
}

unsafe fn foreground_target(config: &Config) -> Option<ForegroundTarget> {
    let foreground = GetForegroundWindow();
    if foreground.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(foreground, Some(&mut pid));
    if pid == 0 || pid == GetCurrentProcessId() {
        return None;
    }
    if !IsWindowVisible(foreground).as_bool() || IsIconic(foreground).as_bool() {
        return None;
    }
    let class = window_class_name(foreground);
    if is_desktop_or_shell(&class) {
        return None;
    }

    let mut client = RECT::default();
    if GetClientRect(foreground, &mut client).is_err() {
        return None;
    }
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    if width < 64 || height < 64 {
        return None;
    }
    let mut origin = POINT { x: 0, y: 0 };
    if !ClientToScreen(foreground, &mut origin).as_bool() {
        return None;
    }
    let center = (
        origin.x + width / 2 + config.offset_x,
        origin.y + height / 2 + config.offset_y,
    );

    let monitor = MonitorFromPoint(
        POINT {
            x: center.0,
            y: center.1,
        },
        MONITOR_DEFAULTTONEAREST,
    );
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return None;
    }
    let monitor_width = info.rcMonitor.right - info.rcMonitor.left;
    let monitor_height = info.rcMonitor.bottom - info.rcMonitor.top;

    Some(ForegroundTarget {
        center,
        client_height: height,
        monitor_width,
        monitor_height,
    })
}

fn window_class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if length <= 0 {
        return String::new();
    }
    let length = length as usize;
    String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
}

fn is_desktop_or_shell(class: &str) -> bool {
    matches!(
        class,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "MultitaskingViewFrame"
            | "ApplicationManager_DesktopShellWindow"
            | "Windows.UI.Core.CoreWindow"
            | "#32768"
            | "tooltips_class32"
    )
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TIMER => {
            tick_state();
            LRESULT(0)
        }
        WM_HOTKEY => {
            STATE.with(|cell| {
                if let Some(state) = cell.borrow_mut().as_mut() {
                    state.handle_hotkey();
                }
            });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            STATE.with(|cell| {
                if let Some(state) = cell.borrow_mut().as_mut() {
                    state.dirty = true;
                }
            });
            tick_state();
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_ERASEBKGND => LRESULT(1),
        windows::Win32::UI::WindowsAndMessaging::WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn tick_state() {
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            if let Err(err) = state.tick() {
                state.fatal_error = Some(err);
                unsafe { PostQuitMessage(1) };
            }
        }
    });
}

pub fn enable_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

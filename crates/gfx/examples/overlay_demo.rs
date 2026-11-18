//! Shows ~200 falling white dots for 5 seconds. Pass `--behind` for behind-windows mode.

use std::time::{Duration, Instant};

use snowy_core::{Effect, Env, Instance, MonitorInfo, Scene};
use snowy_gfx::OverlayWindow;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const HLSL: &str = r#"
cbuffer Frame : register(b0) { float2 viewport; float time; float _pad; };
struct VSIn { float2 pos : POS; float size : SIZE; float rot : ROT; float4 color : COLOR; float4 param : PARAM; uint vid : SV_VertexID; };
struct VSOut { float4 sv : SV_Position; float2 uv : TEXCOORD0; float4 color : COLOR; };
VSOut VSMain(VSIn i) {
    float2 c = float2((i.vid & 1) ? 1.0 : -1.0, (i.vid & 2) ? 1.0 : -1.0);
    float s = sin(i.rot), co = cos(i.rot);
    float2 p = i.pos + float2(c.x * co - c.y * s, c.x * s + c.y * co) * i.size;
    VSOut o;
    o.sv = float4(p.x / viewport.x * 2.0 - 1.0, 1.0 - p.y / viewport.y * 2.0, 0, 1);
    o.uv = c; o.color = i.color;
    return o;
}
float4 PSMain(VSOut i) : SV_Target {
    float a = saturate(1.0 - length(i.uv)) * i.color.a;
    return float4(i.color.rgb * a, a);
}
"#;

struct Dots {
    items: Vec<Instance>,
    size: (f32, f32),
}

impl Effect for Dots {
    fn resize(&mut self, w: f32, h: f32) {
        self.size = (w, h);
        self.items = (0..200)
            .map(|i| {
                let f = i as f32;
                Instance {
                    pos: [(f * 97.3) % w, (f * 53.7) % h],
                    size: 3.0 + (i % 4) as f32,
                    rot: 0.0,
                    color: [1.0, 1.0, 1.0, 0.9],
                    param: [0.0; 4],
                }
            })
            .collect();
    }
    fn update(&mut self, dt: f32, _env: &Env, _scene: &Scene) {
        for (i, d) in self.items.iter_mut().enumerate() {
            d.pos[1] += (60.0 + (i % 5) as f32 * 20.0) * dt;
            if d.pos[1] > self.size.1 {
                d.pos[1] -= self.size.1;
            }
        }
    }
    fn instances(&self) -> &[Instance] {
        &self.items
    }
    fn shader(&self) -> &'static str {
        HLSL
    }
}

fn main() -> anyhow::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let behind = std::env::args().any(|a| a == "--behind");
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let mon = MonitorInfo {
        id: "primary".into(),
        rect: (0, 0, w, h),
        work_rect: (0, 0, w, h),
        primary: true,
        dpi_scale: 1.0,
    };
    let mut overlay = OverlayWindow::new(&mon, behind)?;
    let mut fx = Dots { items: vec![], size: (0.0, 0.0) };
    let (ow, oh) = overlay.size();
    fx.resize(ow as f32, oh as f32);
    let env = Env { wind: 0.0, intensity: 1.0, speed: 1.0, time: 0.0 };

    let start = Instant::now();
    let mut last = start;
    while start.elapsed() < Duration::from_secs(5) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        fx.update(dt, &env, &Scene::EMPTY);
        overlay.render(&fx, start.elapsed().as_secs_f32())?;
    }
    overlay.clear()?;
    Ok(())
}

//! Shared types and contracts for Snowy. FROZEN CONTRACT: other crates depend on
//! these exact shapes. Change only with coordination.

pub mod config;
pub mod i18n;
pub mod props;
pub mod shutdown;

/// Something an effect did that the rest of the app should know about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldEvent {
    /// The user stacked snowballs on the ground: replace them with a persistent snowman.
    /// `x` is the overlay-local center, `balls` is 2 or 3, `height` the stack height in px.
    SnowmanBuilt { x: f32, balls: u8, height: f32 },
}

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Weather effects that can be shown on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectKind {
    Snow,
    HeavySnow,
    Rain,
    Fireflies,
    Sakura,
    Leaves,
    Starfall,
}

impl EffectKind {
    pub const ALL: [EffectKind; 7] = [
        EffectKind::Snow,
        EffectKind::HeavySnow,
        EffectKind::Rain,
        EffectKind::Fireflies,
        EffectKind::Sakura,
        EffectKind::Leaves,
        EffectKind::Starfall,
    ];

    /// Stable string id, used for settings and translation keys.
    pub fn id(self) -> &'static str {
        match self {
            EffectKind::Snow => "snow",
            EffectKind::HeavySnow => "heavy_snow",
            EffectKind::Rain => "rain",
            EffectKind::Fireflies => "fireflies",
            EffectKind::Sakura => "sakura",
            EffectKind::Leaves => "leaves",
            EffectKind::Starfall => "starfall",
        }
    }
}

/// What the user picked. `Season` and `Random` resolve to a concrete EffectKind at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectChoice {
    Off,
    Fixed(EffectKind),
    /// Spring: Sakura, Summer: Fireflies, Autumn: Leaves, Winter: Snow.
    Season,
    /// Picks a random effect at start and every `random_minutes`.
    Random,
    /// Follow the real weather outside (opt-in network use; see snowy-weather).
    Weather,
}

/// One sprite drawn by the GPU. Layout matches the HLSL input layout (see SHADER_CONTRACT).
/// Coordinates are in physical pixels, origin top-left of the monitor's overlay.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct Instance {
    /// Center position in pixels. HLSL semantic: POS (float2)
    pub pos: [f32; 2],
    /// Half-extent of the quad in pixels. HLSL semantic: SIZE (float)
    pub size: f32,
    /// Rotation in radians. HLSL semantic: ROT (float)
    pub rot: f32,
    /// Straight (non-premultiplied) RGBA 0..1. HLSL semantic: COLOR (float4)
    pub color: [f32; 4],
    /// Effect-specific parameters. HLSL semantic: PARAM (float4)
    pub param: [f32; 4],
}

/// Environment passed to effects every frame.
#[derive(Debug, Clone, Copy)]
pub struct Env {
    /// Horizontal wind, roughly -1..1 (negative = blowing left).
    pub wind: f32,
    /// Density multiplier 0.25..2.0 from settings.
    pub intensity: f32,
    /// Speed multiplier 0.5..2.0 from settings.
    pub speed: f32,
    /// Seconds since app start.
    pub time: f32,
}

/// A horizontal ledge things can land on: the visible part of a window's top edge, or the
/// taskbar's top edge. Overlay-local physical pixels (0,0 = top-left of this monitor).
/// Only the portions not covered by windows above it are reported, so one window may
/// yield several segments sharing the same `id`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// Window handle as u64. Stable while the window lives; use it to track what lies on it.
    pub id: u64,
    pub x0: f32,
    pub x1: f32,
    /// Y of the top edge. Things rest ABOVE it (y < this).
    pub y: f32,
}

/// Mouse state relative to this overlay. Present only on the monitor under the cursor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    /// Overlay-local physical pixels.
    pub pos: [f32; 2],
    /// Smoothed velocity in px/s.
    pub vel: [f32; 2],
    /// Left button held.
    pub down: bool,
    /// True when the cursor is over the bare desktop (wallpaper / icons area), i.e. not over
    /// any app window or the taskbar. Interactions like rolling snowballs require this.
    pub over_desktop: bool,
}

/// What the effect can interact with this frame. Surfaces refresh ~10 times per second;
/// the slice may be identical between frames.
#[derive(Debug, Clone, Copy)]
pub struct Scene<'a> {
    /// Ledges, sorted by `y` ascending. Empty when "Keep windows clear" is on or in
    /// behind-windows mode.
    pub surfaces: &'a [Surface],
    /// Floor y (bottom of the monitor) where things may pile, or None when piling is off.
    pub ground: Option<f32>,
    pub pointer: Option<Pointer>,
    /// User allows snowball play (only meaningful for snow effects).
    pub snowballs: bool,
}

impl Scene<'_> {
    pub const EMPTY: Scene<'static> = Scene { surfaces: &[], ground: None, pointer: None, snowballs: false };
}

/// A simulated weather effect for one monitor.
pub trait Effect {
    /// Called on creation and whenever the overlay size changes (physical pixels).
    fn resize(&mut self, width: f32, height: f32);
    /// Advance the simulation. `dt` is seconds, clamp-safe up to 0.1.
    /// Accumulated things (piles, resting petals) must react to `scene`: when a surface `id`
    /// disappears or its segment moves (window dragged/closed/minimized), whatever lay on it
    /// falls off as particles instead of floating in mid-air.
    fn update(&mut self, dt: f32, env: &Env, scene: &Scene);
    /// Remove all accumulated snow/petals/leaves and snowballs (e.g. "clear the snow").
    fn clear_accumulation(&mut self) {}
    /// Overlay-local rects `[x0, y0, x1, y1]` where a left-click on the bare desktop should be
    /// captured by Snowy (swallowed, not delivered to the desktop/apps), e.g. just above
    /// snow piles and around snowballs. Appended to `out`; default: none.
    fn grab_zones(&self, out: &mut Vec<[f32; 4]>) {
        let _ = out;
    }
    /// True when a press at `pos` (overlay-local) would pick up something of this effect that
    /// sits there, e.g. a snowball. It then wins over a decoration behind it. Default: false.
    fn grabs_at(&self, pos: [f32; 2]) -> bool {
        let _ = pos;
        false
    }
    /// A friend threw a snowball: it flies in from the top at `x` (0..1 across). Returns false
    /// when this effect has no snowballs (the caller then only shows a notice).
    fn incoming_snowball(&mut self, x: f32) -> bool {
        let _ = x;
        false
    }
    /// Maximum height of ground snow in pixels at 1080p (ledges get half). Default: ignored.
    fn set_pile_depth(&mut self, depth: f32) {
        let _ = depth;
    }
    /// Drain events produced since the last call. Default: none.
    fn take_events(&mut self, out: &mut Vec<WorldEvent>) {
        let _ = out;
    }
    /// Remove ground snow, resting snowballs and leaves between `x0..x1` (overlay-local),
    /// e.g. where the snowplough just drove. Default: nothing.
    fn clear_region(&mut self, x0: f32, x1: f32) {
        let _ = (x0, x1);
    }
    /// Current sprites to draw this frame.
    fn instances(&self) -> &[Instance];
    /// HLSL source for this effect. Must follow SHADER_CONTRACT. Must return the same
    /// `&'static str` every call (the renderer caches compiled shaders by pointer).
    fn shader(&self) -> &'static str;
}

/// Contract every effect shader follows. The renderer (snowy-gfx) builds the pipeline
/// from this; effect shaders (snowy-effects) implement it.
pub const SHADER_CONTRACT: &str = r#"
// Renderer draws a 4-vertex triangle strip per instance (DrawInstanced(4, n)).
// Vertex buffer slot 0 is the per-instance Instance buffer (D3D11_INPUT_PER_INSTANCE_DATA).
// Input layout (all slot 0, per-instance, step rate 1):
//   POS   float2 offset 0
//   SIZE  float  offset 8
//   ROT   float  offset 12
//   COLOR float4 offset 16
//   PARAM float4 offset 32
// Constant buffer b0:
//   cbuffer Frame : register(b0) { float2 viewport; float time; float _pad; };
// Entry points: VSMain (vs_5_0), PSMain (ps_5_0). VSMain also receives uint vid : SV_VertexID.
// Quad corners for vid 0..3 (strip): (-1,-1), (1,-1), (-1,1), (1,1).
// Pixel shader returns PREMULTIPLIED alpha. Blend state: ONE, INV_SRC_ALPHA.
// One shader per effect per frame: effects that draw several kinds of things (flakes,
// piles, snowballs) select the shape inside their shader, by convention via PARAM.w.
// Render target is cleared to transparent (0,0,0,0) every frame.
"#;

/// A physical monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    /// Stable-ish device name, e.g. "\\\\.\\DISPLAY1". Used as key in settings.
    pub id: String,
    /// Full monitor rect in virtual-screen physical pixels: x, y, width, height.
    pub rect: (i32, i32, i32, i32),
    /// Work area (excluding taskbar) in the same coordinates.
    pub work_rect: (i32, i32, i32, i32),
    pub primary: bool,
    pub dpi_scale: f32,
}

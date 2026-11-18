//! D3D11 + DirectComposition renderer for one overlay window.

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use snowy_core::{Effect, Instance};
use windows::core::{s, Interface, PCSTR};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

const INSTANCE_SIZE: usize = std::mem::size_of::<Instance>();
const MIN_INSTANCES: usize = 1024;

/// Minimal shader with the instance layout of `SHADER_CONTRACT`, compiled at startup to
/// check that the device can build our pipelines.
const PROBE_SHADER: &str = r"
struct VSIn { float2 pos : POS; float size : SIZE; float rot : ROT; float4 color : COLOR; float4 param : PARAM; uint vid : SV_VertexID; };
struct VSOut { float4 pos : SV_POSITION; float4 color : COLOR; };
VSOut VSMain(VSIn i) { VSOut o; o.pos = float4(i.pos * i.size + i.rot + i.param.xy, 0, 1); o.color = i.color; return o; }
float4 PSMain(VSOut i) : SV_TARGET { return i.color; }
";

struct Pipeline {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    layout: ID3D11InputLayout,
}

pub struct Renderer {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    swapchain: IDXGISwapChain1,
    rtv: Option<ID3D11RenderTargetView>,
    _dcomp: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    blend: ID3D11BlendState,
    raster: ID3D11RasterizerState,
    cbuf: ID3D11Buffer,
    ibuf: Option<ID3D11Buffer>,
    ibuf_cap: usize,
    pipelines: HashMap<usize, Pipeline>,
    /// Shader targets for this device's feature level.
    targets: (PCSTR, PCSTR),
    size: (u32, u32),
    cleared: bool,
}

/// Device plus the shader targets its feature level supports.
struct Device {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    /// (vertex, pixel) shader targets: SM 5.0 on feature level 11+, SM 4.0 on 10.x.
    targets: (PCSTR, PCSTR),
}

fn create_device(driver: D3D_DRIVER_TYPE) -> Result<Device> {
    let mut device = None;
    let mut ctx = None;
    let mut level = D3D_FEATURE_LEVEL::default();
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut level),
            Some(&mut ctx),
        )
        .context("D3D11CreateDevice")?;
    }
    let (Some(device), Some(ctx)) = (device, ctx) else {
        return Err(anyhow!("D3D11CreateDevice returned no device"));
    };
    let targets = if level.0 >= D3D_FEATURE_LEVEL_11_0.0 {
        (s!("vs_5_0"), s!("ps_5_0"))
    } else if level.0 >= D3D_FEATURE_LEVEL_10_0.0 {
        (s!("vs_4_0"), s!("ps_4_0"))
    } else {
        return Err(anyhow!("feature level {:#x} is below 10.0", level.0));
    };
    Ok(Device { device, ctx, targets })
}

/// "GPU name (feature level)" for the log.
fn describe(device: &ID3D11Device) -> String {
    unsafe {
        let level = device.GetFeatureLevel().0;
        let name = device
            .cast::<IDXGIDevice>()
            .and_then(|d| d.GetAdapter())
            .and_then(|a| a.GetDesc())
            .map(|d| {
                let n = d.Description.iter().position(|&c| c == 0).unwrap_or(d.Description.len());
                String::from_utf16_lossy(&d.Description[..n])
            })
            .unwrap_or_else(|_| "unknown adapter".into());
        format!("{name} (feature level {}.{})", level >> 12, (level >> 8) & 0xf)
    }
}

fn compile(src: &str, entry: PCSTR, target: PCSTR) -> Result<Vec<u8>> {
    let mut code = None;
    let mut errs = None;
    let r = unsafe {
        D3DCompile(
            src.as_ptr() as *const _,
            src.len(),
            s!("effect.hlsl"),
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            Some(&mut errs),
        )
    };
    if let Err(e) = r {
        let msg = errs
            .map(|b| unsafe {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    b.GetBufferPointer() as *const u8,
                    b.GetBufferSize(),
                ))
                .into_owned()
            })
            .unwrap_or_else(|| e.to_string());
        return Err(anyhow!("HLSL compile error: {msg}"));
    }
    let code = code.ok_or_else(|| anyhow!("D3DCompile returned no bytecode"))?;
    Ok(unsafe {
        std::slice::from_raw_parts(code.GetBufferPointer() as *const u8, code.GetBufferSize())
            .to_vec()
    })
}

fn elem(name: PCSTR, format: DXGI_FORMAT, offset: u32) -> D3D11_INPUT_ELEMENT_DESC {
    D3D11_INPUT_ELEMENT_DESC {
        SemanticName: name,
        SemanticIndex: 0,
        Format: format,
        InputSlot: 0,
        AlignedByteOffset: offset,
        InputSlotClass: D3D11_INPUT_PER_INSTANCE_DATA,
        InstanceDataStepRate: 1,
    }
}

impl Renderer {
    /// Sets up rendering into `hwnd`, on the GPU if possible, else on WARP (software).
    /// Every step is retried on WARP, not only device creation: some drivers create a
    /// device but then fail composition swap chains or shaders.
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self> {
        let mut errors = Vec::new();
        // SNOWY_RENDERER=warp skips the GPU (to test the fallback, or to dodge a bad driver).
        let warp_only = std::env::var("SNOWY_RENDERER").is_ok_and(|v| v.eq_ignore_ascii_case("warp"));
        let drivers = [(D3D_DRIVER_TYPE_HARDWARE, "GPU"), (D3D_DRIVER_TYPE_WARP, "WARP")];
        for (driver, name) in drivers.into_iter().skip(warp_only as usize) {
            match Self::with_driver(hwnd, width, height, driver) {
                Ok(r) => {
                    log::info!("renderer: {name}, {}", describe(&r.device));
                    return Ok(r);
                }
                Err(e) => {
                    log::warn!("renderer on {name} failed: {e:#}");
                    errors.push(format!("{name}: {e:#}"));
                }
            }
        }
        Err(anyhow!("no usable renderer ({})", errors.join("; ")))
    }

    fn with_driver(hwnd: HWND, width: u32, height: u32, driver: D3D_DRIVER_TYPE) -> Result<Self> {
        let Device { device, ctx, targets } = create_device(driver)?;
        unsafe {
            let dxgi_device: IDXGIDevice = device.cast()?;
            let factory: IDXGIFactory2 = dxgi_device.GetAdapter()?.GetParent()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swapchain = factory
                .CreateSwapChainForComposition(&device, &desc, None)
                .context("CreateSwapChainForComposition")?;

            let dcomp: IDCompositionDevice =
                DCompositionCreateDevice(&dxgi_device).context("DCompositionCreateDevice")?;
            let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = dcomp.CreateVisual()?;
            visual.SetContent(&swapchain)?;
            target.SetRoot(&visual)?;
            dcomp.Commit()?;

            let rt = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut bd = D3D11_BLEND_DESC::default();
            bd.RenderTarget[0] = rt;
            let mut blend = None;
            device.CreateBlendState(&bd, Some(&mut blend))?;

            let rd = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            };
            let mut raster = None;
            device.CreateRasterizerState(&rd, Some(&mut raster))?;

            let cd = D3D11_BUFFER_DESC {
                ByteWidth: 16,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            let mut cbuf = None;
            device.CreateBuffer(&cd, None, Some(&mut cbuf))?;

            let mut r = Renderer {
                device,
                ctx,
                targets,
                swapchain,
                rtv: None,
                _dcomp: dcomp,
                _target: target,
                _visual: visual,
                blend: blend.ok_or_else(|| anyhow!("no blend state"))?,
                raster: raster.ok_or_else(|| anyhow!("no rasterizer state"))?,
                cbuf: cbuf.ok_or_else(|| anyhow!("no constant buffer"))?,
                ibuf: None,
                ibuf_cap: 0,
                pipelines: HashMap::new(),
                size: (width, height),
                cleared: false,
            };
            r.create_rtv()?;
            // Prove the pipeline works now (with a shader every effect shares the contract
            // of), so a broken driver falls back to WARP instead of failing every frame.
            r.pipeline(PROBE_SHADER).context("test shader")?;
            r.clear().context("first present")?;
            Ok(r)
        }
    }

    fn create_rtv(&mut self) -> Result<()> {
        unsafe {
            let back: ID3D11Texture2D = self.swapchain.GetBuffer(0)?;
            let mut rtv = None;
            self.device.CreateRenderTargetView(&back, None, Some(&mut rtv))?;
            self.rtv = rtv;
        }
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if (width, height) == self.size {
            return Ok(());
        }
        unsafe {
            self.ctx.OMSetRenderTargets(None, None);
            self.rtv = None;
            self.swapchain
                .ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0))?;
        }
        self.size = (width, height);
        self.cleared = false;
        self.create_rtv()
    }

    fn pipeline(&mut self, src: &'static str) -> Result<()> {
        let key = src.as_ptr() as usize;
        if self.pipelines.contains_key(&key) {
            return Ok(());
        }
        let vs_code = compile(src, s!("VSMain"), self.targets.0)?;
        let ps_code = compile(src, s!("PSMain"), self.targets.1)?;
        let elems = [
            elem(s!("POS"), DXGI_FORMAT_R32G32_FLOAT, 0),
            elem(s!("SIZE"), DXGI_FORMAT_R32_FLOAT, 8),
            elem(s!("ROT"), DXGI_FORMAT_R32_FLOAT, 12),
            elem(s!("COLOR"), DXGI_FORMAT_R32G32B32A32_FLOAT, 16),
            elem(s!("PARAM"), DXGI_FORMAT_R32G32B32A32_FLOAT, 32),
        ];
        unsafe {
            let mut vs = None;
            self.device.CreateVertexShader(&vs_code, None, Some(&mut vs))?;
            let mut ps = None;
            self.device.CreatePixelShader(&ps_code, None, Some(&mut ps))?;
            let mut layout = None;
            self.device
                .CreateInputLayout(&elems, &vs_code, Some(&mut layout))
                .context("CreateInputLayout (does VSMain match SHADER_CONTRACT?)")?;
            self.pipelines.insert(
                key,
                Pipeline {
                    vs: vs.ok_or_else(|| anyhow!("no vertex shader"))?,
                    ps: ps.ok_or_else(|| anyhow!("no pixel shader"))?,
                    layout: layout.ok_or_else(|| anyhow!("no input layout"))?,
                },
            );
        }
        Ok(())
    }

    fn ensure_ibuf(&mut self, count: usize) -> Result<()> {
        if self.ibuf.is_some() && count <= self.ibuf_cap {
            return Ok(());
        }
        let mut cap = self.ibuf_cap.max(MIN_INSTANCES);
        while cap < count {
            cap *= 2;
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: (cap * INSTANCE_SIZE) as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut buf = None;
        unsafe { self.device.CreateBuffer(&desc, None, Some(&mut buf))? };
        self.ibuf = buf;
        self.ibuf_cap = cap;
        Ok(())
    }

    fn begin(&self) -> Result<()> {
        let rtv = self.rtv.clone().ok_or_else(|| anyhow!("no render target"))?;
        unsafe {
            self.ctx.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            self.ctx.ClearRenderTargetView(&rtv, &[0.0; 4]);
        }
        Ok(())
    }

    fn present(&self) -> Result<()> {
        // No vsync here: with several monitors, Present(1) per window would serialize to
        // one vblank each. The app syncs once per frame via DwmFlush (snowy_sys::wait_vblank).
        // DXGI_STATUS_OCCLUDED is a success code and is ignored by `.ok()`.
        unsafe { self.swapchain.Present(0, DXGI_PRESENT(0)).ok()? };
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        if self.cleared {
            return Ok(());
        }
        self.begin()?;
        self.present()?;
        self.cleared = true;
        Ok(())
    }

    pub fn render(&mut self, effect: &dyn Effect, time: f32) -> Result<()> {
        let instances = effect.instances();
        let shader = effect.shader();
        if !instances.is_empty() {
            self.pipeline(shader)?;
            self.ensure_ibuf(instances.len())?;
        }
        self.begin()?;
        if !instances.is_empty() {
            self.draw(shader, instances, time)?;
        }
        self.cleared = false;
        self.present()
    }

    fn draw(&self, shader: &'static str, instances: &[Instance], time: f32) -> Result<()> {
        let p = &self.pipelines[&(shader.as_ptr() as usize)];
        let ibuf = self.ibuf.as_ref().ok_or_else(|| anyhow!("no instance buffer"))?;
        let ctx = &self.ctx;
        unsafe {
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(ibuf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut m))?;
            let bytes: &[u8] = bytemuck::cast_slice(instances);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), m.pData as *mut u8, bytes.len());
            ctx.Unmap(ibuf, 0);

            let frame = [self.size.0 as f32, self.size.1 as f32, time, 0.0];
            ctx.Map(&self.cbuf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut m))?;
            std::ptr::copy_nonoverlapping(frame.as_ptr(), m.pData as *mut f32, 4);
            ctx.Unmap(&self.cbuf, 0);

            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.size.0 as f32,
                Height: self.size.1 as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            ctx.RSSetState(&self.raster);
            ctx.OMSetBlendState(&self.blend, Some(&[0.0; 4]), 0xFFFF_FFFF);
            ctx.IASetInputLayout(&p.layout);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            let stride = INSTANCE_SIZE as u32;
            let offset = 0u32;
            ctx.IASetVertexBuffers(0, 1, Some(&Some(ibuf.clone())), Some(&stride), Some(&offset));
            ctx.VSSetShader(&p.vs, None);
            ctx.VSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetShader(&p.ps, None);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.DrawInstanced(4, instances.len() as u32, 0, 0);
        }
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.ctx.ClearState();
            self.ctx.Flush();
        }
    }
}

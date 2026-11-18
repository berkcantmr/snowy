#![cfg(windows)]

use snowy_core::EffectKind;
use windows::core::{s, PCSTR};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::ID3DBlob;

fn compile(src: &str, entry: PCSTR, target: PCSTR, name: &str) {
    let mut code: Option<ID3DBlob> = None;
    let mut err: Option<ID3DBlob> = None;
    let r = unsafe {
        D3DCompile(
            src.as_ptr() as *const _,
            src.len(),
            None,
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            Some(&mut err),
        )
    };
    if r.is_err() {
        let msg = err
            .map(|b| unsafe {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    b.GetBufferPointer() as *const u8,
                    b.GetBufferSize(),
                ))
                .into_owned()
            })
            .unwrap_or_default();
        panic!("{name} failed to compile: {r:?}\n{msg}");
    }
}

#[test]
fn all_shaders_compile() {
    for kind in EffectKind::ALL {
        let e = snowy_effects::create(kind, 640.0, 480.0);
        let src = e.shader();
        compile(src, s!("VSMain"), s!("vs_5_0"), &format!("{kind:?} VS"));
        compile(src, s!("PSMain"), s!("ps_5_0"), &format!("{kind:?} PS"));
    }
}

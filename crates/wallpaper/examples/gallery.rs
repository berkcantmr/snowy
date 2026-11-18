//! Contact sheet of all themes x times of day, plus two full-size samples.
//! Usage: cargo run -p snowy-wallpaper --release --example gallery -- <out_dir> [full_theme:tod ...]

use snowy_wallpaper::{render, write_png, Theme, TimeOfDay};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(args.first().cloned().unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&out).unwrap();
    let (tw, th, gap) = (480usize, 270usize, 6usize);
    let cols = TimeOfDay::ALL.len();
    let rows = Theme::ALL.len();
    let (sw, sh) = (cols * (tw + gap) + gap, rows * (th + gap) + gap);
    let mut sheet = vec![24u8; sw * sh * 4];
    for px in sheet.chunks_mut(4) {
        px[3] = 255;
    }
    let t0 = Instant::now();
    for (r, theme) in Theme::ALL.into_iter().enumerate() {
        for (cidx, tod) in TimeOfDay::ALL.into_iter().enumerate() {
            let img = render(theme, tod, tw as u32, th as u32);
            let (ox, oy) = (gap + cidx * (tw + gap), gap + r * (th + gap));
            for y in 0..th {
                let dst = ((oy + y) * sw + ox) * 4;
                sheet[dst..dst + tw * 4].copy_from_slice(&img[y * tw * 4..(y + 1) * tw * 4]);
            }
        }
    }
    println!("sheet rendered in {:?}", t0.elapsed());
    write_png(&out.join("wallpapers_sheet.png"), &sheet, sw as u32, sh as u32).unwrap();

    let mut fulls: Vec<(Theme, TimeOfDay)> = args[1.min(args.len())..]
        .iter()
        .filter_map(|a| {
            let (t, d) = a.split_once(':')?;
            Some((Theme::from_id(t)?, TimeOfDay::from_id(d)?))
        })
        .collect();
    if fulls.is_empty() {
        fulls = vec![(Theme::SnowyHills, TimeOfDay::Golden), (Theme::Aurora, TimeOfDay::Night)];
    }
    for (theme, tod) in fulls {
        let t = Instant::now();
        let img = render(theme, tod, 1920, 1080);
        println!("{} {} 1920x1080 in {:?}", theme.id(), tod.id(), t.elapsed());
        write_png(&out.join(format!("full_{}_{}.png", theme.id(), tod.id())), &img, 1920, 1080).unwrap();
    }
}

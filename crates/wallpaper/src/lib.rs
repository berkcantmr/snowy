//! Original procedurally painted wallpapers (six themes, seven times of day) and
//! helpers to cache them as PNG and set them as the Windows desktop wallpaper.

mod paint;
mod palette;
mod scenes;
mod win;

pub use win::{apply, current_wallpaper, local_hour};

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    SnowyHills,
    PineForest,
    FrozenLake,
    Mountains,
    StarryVillage,
    Aurora,
}

impl Theme {
    pub const ALL: [Theme; 6] =
        [Theme::SnowyHills, Theme::PineForest, Theme::FrozenLake, Theme::Mountains, Theme::StarryVillage, Theme::Aurora];

    pub fn id(self) -> &'static str {
        match self {
            Theme::SnowyHills => "snowy_hills",
            Theme::PineForest => "pine_forest",
            Theme::FrozenLake => "frozen_lake",
            Theme::Mountains => "mountains",
            Theme::StarryVillage => "starry_village",
            Theme::Aurora => "aurora",
        }
    }

    pub fn from_id(id: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|t| t.id() == id)
    }

    /// Fixed per-theme seed: the same theme always paints the same picture.
    fn seed(self) -> u32 {
        0x51_0A_17 + self as u32 * 7919
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeOfDay {
    Dawn,
    Morning,
    Midday,
    Golden,
    Sunset,
    Dusk,
    Night,
}

impl TimeOfDay {
    pub const ALL: [TimeOfDay; 7] =
        [TimeOfDay::Dawn, TimeOfDay::Morning, TimeOfDay::Midday, TimeOfDay::Golden, TimeOfDay::Sunset, TimeOfDay::Dusk, TimeOfDay::Night];

    pub fn id(self) -> &'static str {
        match self {
            TimeOfDay::Dawn => "dawn",
            TimeOfDay::Morning => "morning",
            TimeOfDay::Midday => "midday",
            TimeOfDay::Golden => "golden",
            TimeOfDay::Sunset => "sunset",
            TimeOfDay::Dusk => "dusk",
            TimeOfDay::Night => "night",
        }
    }

    pub fn from_id(id: &str) -> Option<TimeOfDay> {
        TimeOfDay::ALL.into_iter().find(|t| t.id() == id)
    }

    /// dawn 5-7, morning 7-10, midday 10-15, golden 15-17, sunset 17-18:30, dusk 18:30-20, else night.
    pub fn from_local_time(hour: f32) -> TimeOfDay {
        let h = hour.rem_euclid(24.0);
        if (5.0..7.0).contains(&h) {
            TimeOfDay::Dawn
        } else if (7.0..10.0).contains(&h) {
            TimeOfDay::Morning
        } else if (10.0..15.0).contains(&h) {
            TimeOfDay::Midday
        } else if (15.0..17.0).contains(&h) {
            TimeOfDay::Golden
        } else if (17.0..18.5).contains(&h) {
            TimeOfDay::Sunset
        } else if (18.5..20.0).contains(&h) {
            TimeOfDay::Dusk
        } else {
            TimeOfDay::Night
        }
    }
}

/// Time of day to show: follows `hour` when `day_night`, else the fixed id (falls back to golden hour).
pub fn target_tod(day_night: bool, fixed_id: &str, hour: f32) -> TimeOfDay {
    if day_night {
        TimeOfDay::from_local_time(hour)
    } else {
        TimeOfDay::from_id(fixed_id).unwrap_or(TimeOfDay::Golden)
    }
}

/// Paints a wallpaper as tightly packed RGBA8 (alpha always 255). Deterministic.
pub fn render(theme: Theme, tod: TimeOfDay, width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let pal = palette::palette(tod);
    let mut canvas = paint::Canvas::new(w, h);
    scenes::paint(&mut canvas, theme, &pal, theme.seed());
    canvas.to_rgba(theme.seed())
}

/// %APPDATA%\Snowy\wallpapers
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Snowy").join("wallpapers")
}

/// File name used in the cache: `<theme>-<tod>-<w>x<h>.png`.
pub fn cache_file_name(theme: Theme, tod: TimeOfDay, width: u32, height: u32) -> String {
    format!("{}-{}-{}x{}.png", theme.id(), tod.id(), width, height)
}

pub fn cache_path(theme: Theme, tod: TimeOfDay, width: u32, height: u32) -> PathBuf {
    cache_dir().join(cache_file_name(theme, tod, width, height))
}

/// Encodes RGBA8 as PNG.
pub fn write_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(rgba).map_err(std::io::Error::other)?;
    Ok(())
}

/// Returns the cached PNG for this wallpaper, rendering and writing it first if missing.
pub fn ensure_cached(theme: Theme, tod: TimeOfDay, width: u32, height: u32) -> std::io::Result<PathBuf> {
    let path = cache_path(theme, tod, width, height);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(cache_dir())?;
    let rgba = render(theme, tod, width, height);
    let tmp = path.with_extension("png.tmp");
    write_png(&tmp, &rgba, width, height)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_of_day_boundaries() {
        use TimeOfDay::*;
        let cases = [
            (0.0, Night),
            (4.99, Night),
            (5.0, Dawn),
            (6.99, Dawn),
            (7.0, Morning),
            (9.99, Morning),
            (10.0, Midday),
            (14.99, Midday),
            (15.0, Golden),
            (16.99, Golden),
            (17.0, Sunset),
            (18.49, Sunset),
            (18.5, Dusk),
            (19.99, Dusk),
            (20.0, Night),
            (23.99, Night),
            (24.0, Night),
            (29.5, Dawn),
            (-1.0, Night),
        ];
        for (h, want) in cases {
            assert_eq!(TimeOfDay::from_local_time(h), want, "hour {h}");
        }
    }

    #[test]
    fn target_time_selection() {
        assert_eq!(target_tod(true, "dawn", 12.0), TimeOfDay::Midday);
        assert_eq!(target_tod(false, "dawn", 12.0), TimeOfDay::Dawn);
        assert_eq!(target_tod(false, "bogus", 3.0), TimeOfDay::Golden);
    }

    #[test]
    fn ids_round_trip() {
        for t in Theme::ALL {
            assert_eq!(Theme::from_id(t.id()), Some(t));
        }
        for t in TimeOfDay::ALL {
            assert_eq!(TimeOfDay::from_id(t.id()), Some(t));
        }
        assert_eq!(Theme::from_id("nope"), None);
    }

    #[test]
    fn cache_naming() {
        assert_eq!(cache_file_name(Theme::FrozenLake, TimeOfDay::Golden, 1920, 1080), "frozen_lake-golden-1920x1080.png");
        let p = cache_path(Theme::Aurora, TimeOfDay::Night, 2560, 1440);
        assert!(p.ends_with(std::path::Path::new("Snowy").join("wallpapers").join("aurora-night-2560x1440.png")));
    }

    #[test]
    fn render_dims_and_determinism() {
        for theme in Theme::ALL {
            let a = render(theme, TimeOfDay::Dusk, 96, 54);
            assert_eq!(a.len(), 96 * 54 * 4);
            assert!(a.chunks(4).all(|p| p[3] == 255));
            assert_eq!(a, render(theme, TimeOfDay::Dusk, 96, 54));
        }
        assert_ne!(render(Theme::Aurora, TimeOfDay::Night, 64, 36), render(Theme::Aurora, TimeOfDay::Midday, 64, 36));
        assert_ne!(render(Theme::Mountains, TimeOfDay::Night, 64, 36), render(Theme::PineForest, TimeOfDay::Night, 64, 36));
    }

    #[test]
    fn tiny_sizes_do_not_panic() {
        for theme in Theme::ALL {
            for tod in TimeOfDay::ALL {
                let _ = render(theme, tod, 1, 1);
                let _ = render(theme, tod, 7, 3);
            }
        }
    }
}

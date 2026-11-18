//! "Match my weather": rough location from IP, current conditions from Open-Meteo, mapped to
//! an effect. Runs on a background thread; never blocks the render loop.
//! FROZEN CONTRACT: WeatherWatch API and `effect_for` (plus `cached`).
//!
//! Privacy: the rounded location is only held in memory for the duration of a fetch; it is
//! never written to disk or logged.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use snowy_core::EffectKind;

/// Latest known conditions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conditions {
    /// WMO weather code (Open-Meteo `weather_code`).
    pub code: u16,
    pub is_day: bool,
    /// Degrees Celsius.
    pub temperature: f32,
    /// km/h
    pub wind_speed: f32,
}

fn season_default(month: u32) -> EffectKind {
    match month {
        3..=5 => EffectKind::Sakura,
        6..=8 => EffectKind::Fireflies,
        9..=11 => EffectKind::Leaves,
        _ => EffectKind::Snow,
    }
}

/// Pick an effect for the conditions and calendar month (1..=12).
///
/// | WMO code                    | Effect                                                    |
/// |-----------------------------|-----------------------------------------------------------|
/// | 71, 73, 77, 85              | Snow                                                      |
/// | 75, 86                      | HeavySnow                                                 |
/// | 51..=67, 80..=82, 95..=99   | Rain (drizzle, rain, showers, thunderstorm)               |
/// | 0..=2 (clear/partly cloudy) | night: Starfall; day: season default (Mar-May Sakura,     |
/// |                             | Jun-Aug Fireflies, Sep-Nov Leaves, else Snow)             |
/// | 3 (overcast)                | Sep-Nov: Leaves; otherwise None                           |
/// | 45, 48 (fog), others        | None                                                      |
///
/// Wind: if the wind is strong (> 35 km/h) and the result would be Leaves, it stays Leaves.
pub fn effect_for(c: &Conditions, month: u32) -> Option<EffectKind> {
    let eff = match c.code {
        71 | 73 | 77 | 85 => Some(EffectKind::Snow),
        75 | 86 => Some(EffectKind::HeavySnow),
        51..=67 | 80..=82 | 95..=99 => Some(EffectKind::Rain),
        0..=2 => Some(if c.is_day { season_default(month) } else { EffectKind::Starfall }),
        3 => (9..=11).contains(&month).then_some(EffectKind::Leaves),
        _ => None,
    };
    // Strong wind never removes falling leaves.
    if c.wind_speed > 35.0 && eff == Some(EffectKind::Leaves) {
        return Some(EffectKind::Leaves);
    }
    eff
}

// ---------------------------------------------------------------- cache

#[derive(Serialize, Deserialize)]
struct CacheFile {
    code: u16,
    is_day: bool,
    temperature: f32,
    wind_speed: f32,
    fetched_at: u64,
}

const CACHE_TTL_SECS: u64 = 3 * 3600;

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn cache_dir() -> PathBuf {
    snowy_core::config::Settings::dir()
}

fn write_cache_in(dir: &Path, c: &Conditions, fetched_at: u64) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let f = CacheFile { code: c.code, is_day: c.is_day, temperature: c.temperature, wind_speed: c.wind_speed, fetched_at };
    let json = serde_json::to_string(&f).map_err(std::io::Error::other)?;
    let tmp = dir.join("weather.json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, dir.join("weather.json"))
}

fn read_cache_in(dir: &Path) -> Option<(Conditions, u64)> {
    let text = std::fs::read_to_string(dir.join("weather.json")).ok()?;
    let f: CacheFile = serde_json::from_str(&text).ok()?;
    let c = Conditions { code: f.code, is_day: f.is_day, temperature: f.temperature, wind_speed: f.wind_speed };
    Some((c, f.fetched_at))
}

/// The on-disk cache (conditions, unix seconds of the fetch), if present. Used by the
/// settings window, which runs in a separate process.
pub fn cached() -> Option<(Conditions, u64)> {
    read_cache_in(&cache_dir())
}

// ---------------------------------------------------------------- parsing

#[derive(Deserialize)]
struct IpApi {
    status: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
}

#[derive(Deserialize)]
struct IpApiCo {
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Deserialize)]
struct Forecast {
    current: CurrentRaw,
}

#[derive(Deserialize)]
struct CurrentRaw {
    weather_code: u16,
    is_day: u8,
    temperature_2m: f32,
    wind_speed_10m: f32,
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn parse_ip_api(body: &str) -> Option<(f64, f64)> {
    let r: IpApi = serde_json::from_str(body).ok()?;
    if r.status.as_deref() != Some("success") {
        return None;
    }
    Some((round1(r.lat?), round1(r.lon?)))
}

fn parse_ipapi_co(body: &str) -> Option<(f64, f64)> {
    let r: IpApiCo = serde_json::from_str(body).ok()?;
    Some((round1(r.latitude?), round1(r.longitude?)))
}

fn parse_forecast(body: &str) -> Option<Conditions> {
    let f: Forecast = serde_json::from_str(body).ok()?;
    let c = f.current;
    Some(Conditions { code: c.weather_code, is_day: c.is_day != 0, temperature: c.temperature_2m, wind_speed: c.wind_speed_10m })
}

// ---------------------------------------------------------------- network

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("Snowy/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn get(agent: &ureq::Agent, url: &str) -> Option<String> {
    agent.get(url).call().ok()?.into_string().ok()
}

fn locate(agent: &ureq::Agent) -> Option<(f64, f64)> {
    get(agent, "http://ip-api.com/json/?fields=status,lat,lon")
        .and_then(|b| parse_ip_api(&b))
        .or_else(|| get(agent, "https://ipapi.co/json/").and_then(|b| parse_ipapi_co(&b)))
}

fn fetch() -> Option<Conditions> {
    let agent = agent();
    let (lat, lon) = locate(&agent)?;
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.1}&longitude={lon:.1}&current=weather_code,is_day,temperature_2m,wind_speed_10m"
    );
    parse_forecast(&get(&agent, &url)?)
}

// ---------------------------------------------------------------- watcher

const REFRESH: Duration = Duration::from_secs(3 * 3600);
const FIRST_DELAY: Duration = Duration::from_secs(2);
const BACKOFF_MIN: [u64; 4] = [2, 5, 15, 30];

/// Background poller. Starts its thread on `start`, refreshes every 3 hours (and soon after
/// start), stops on drop (the thread is detached; it exits as soon as it notices).
pub struct WeatherWatch {
    state: Arc<Mutex<Option<Conditions>>>,
    stop: Sender<()>,
}

impl WeatherWatch {
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(None));
        let (stop, rx) = mpsc::channel::<()>();
        let shared = Arc::clone(&state);
        let dir = cache_dir();
        let spawned = std::thread::Builder::new().name("snowy-weather".into()).spawn(move || {
            let mut wait = FIRST_DELAY;
            if let Some((c, at)) = read_cache_in(&dir) {
                let age = now_unix().saturating_sub(at);
                if age < CACHE_TTL_SECS {
                    *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
                    wait = Duration::from_secs(CACHE_TTL_SECS - age);
                }
            }
            let mut failures = 0usize;
            loop {
                match rx.recv_timeout(wait) {
                    Err(RecvTimeoutError::Timeout) => {}
                    _ => return,
                }
                match fetch() {
                    Some(c) => {
                        failures = 0;
                        wait = REFRESH;
                        // Dropped while the request was in flight: discard the result.
                        if !matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                            return;
                        }
                        *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
                        if let Err(e) = write_cache_in(&dir, &c, now_unix()) {
                            log::warn!("weather cache not written: {e}");
                        }
                    }
                    None => {
                        let m = BACKOFF_MIN[failures.min(BACKOFF_MIN.len() - 1)];
                        failures += 1;
                        wait = Duration::from_secs(m * 60);
                        log::warn!("weather fetch failed; retrying in {m} min");
                    }
                }
            }
        });
        if let Err(e) = spawned {
            log::warn!("weather thread not started: {e}");
        }
        Self { state, stop }
    }

    /// Latest conditions, if any fetch succeeded so far.
    pub fn current(&self) -> Option<Conditions> {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for WeatherWatch {
    fn drop(&mut self) {
        // Wakes the thread immediately; it is detached so an in-flight request cannot block us.
        let _ = self.stop.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(code: u16, is_day: bool, wind: f32) -> Conditions {
        Conditions { code, is_day, temperature: 10.0, wind_speed: wind }
    }

    #[test]
    fn snow_rain_families() {
        for code in [71, 73, 77, 85] {
            assert_eq!(effect_for(&c(code, true, 0.0), 7), Some(EffectKind::Snow));
        }
        for code in [75, 86] {
            assert_eq!(effect_for(&c(code, false, 0.0), 7), Some(EffectKind::HeavySnow));
        }
        for code in (51..=67).chain(80..=82).chain(95..=99) {
            assert_eq!(effect_for(&c(code, true, 0.0), 1), Some(EffectKind::Rain), "{code}");
        }
        for code in [45, 48, 68, 70, 83, 84, 100] {
            assert_eq!(effect_for(&c(code, true, 0.0), 10), None, "{code}");
        }
    }

    #[test]
    fn clear_day_night_seasons() {
        for code in 0..=2 {
            assert_eq!(effect_for(&c(code, false, 0.0), 1), Some(EffectKind::Starfall));
            assert_eq!(effect_for(&c(code, true, 0.0), 4), Some(EffectKind::Sakura));
            assert_eq!(effect_for(&c(code, true, 0.0), 7), Some(EffectKind::Fireflies));
            assert_eq!(effect_for(&c(code, true, 0.0), 10), Some(EffectKind::Leaves));
            assert_eq!(effect_for(&c(code, true, 0.0), 12), Some(EffectKind::Snow));
            assert_eq!(effect_for(&c(code, true, 0.0), 2), Some(EffectKind::Snow));
        }
    }

    #[test]
    fn overcast_and_wind() {
        assert_eq!(effect_for(&c(3, true, 5.0), 10), Some(EffectKind::Leaves));
        assert_eq!(effect_for(&c(3, true, 50.0), 10), Some(EffectKind::Leaves));
        assert_eq!(effect_for(&c(3, true, 50.0), 6), None);
        assert_eq!(effect_for(&c(3, false, 0.0), 1), None);
    }

    #[test]
    fn cache_roundtrip() {
        let dir = std::env::temp_dir().join(format!("snowy-weather-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read_cache_in(&dir).is_none());
        let cond = c(73, false, 12.5);
        write_cache_in(&dir, &cond, 1_700_000_000).unwrap();
        assert_eq!(read_cache_in(&dir), Some((cond, 1_700_000_000)));
        let text = std::fs::read_to_string(dir.join("weather.json")).unwrap();
        assert!(!text.contains("lat") && !text.contains("lon"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_samples() {
        let om = r#"{"latitude":52.5,"longitude":13.4,"current_units":{"time":"iso8601"},
            "current":{"time":"2026-11-18T10:00","interval":900,"weather_code":61,"is_day":1,"temperature_2m":7.3,"wind_speed_10m":18.2}}"#;
        let w = parse_forecast(om).unwrap();
        assert_eq!((w.code, w.is_day), (61, true));
        assert!((w.temperature - 7.3).abs() < 1e-4 && (w.wind_speed - 18.2).abs() < 1e-4);
        assert!(parse_forecast("{}").is_none());

        assert_eq!(parse_ip_api(r#"{"status":"success","lat":52.5167,"lon":13.3833}"#), Some((52.5, 13.4)));
        assert_eq!(parse_ip_api(r#"{"status":"fail"}"#), None);
        assert_eq!(parse_ipapi_co(r#"{"ip":"x","latitude":-33.87,"longitude":151.21,"city":"y"}"#), Some((-33.9, 151.2)));
        assert_eq!(parse_ipapi_co(r#"{"error":true}"#), None);
    }

    #[test]
    fn drop_is_prompt() {
        let t = std::time::Instant::now();
        let w = WeatherWatch::start();
        drop(w);
        assert!(t.elapsed() < Duration::from_millis(200));
    }

    #[test]
    #[ignore]
    fn live() {
        let agent = agent();
        println!("located: {}", locate(&agent).is_some());
        let w = fetch().expect("live fetch failed");
        println!("live conditions: {w:?}; effect(month 11) = {:?}", effect_for(&w, 11));
    }
}

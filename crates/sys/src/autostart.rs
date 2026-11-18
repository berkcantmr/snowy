use anyhow::{anyhow, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("Snowy");

fn open(access: REG_SAM_FLAGS) -> Result<HKEY> {
    let mut key = HKEY::default();
    let r = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
    };
    if r != ERROR_SUCCESS {
        return Err(anyhow!("RegCreateKeyExW failed: {}", r.0));
    }
    Ok(key)
}

/// Add/remove `current_exe()` to HKCU\...\Run as "Snowy".
pub fn set_autostart(enabled: bool) -> Result<()> {
    let key = open(KEY_SET_VALUE)?;
    let result = (|| {
        if enabled {
            let exe = std::env::current_exe()?;
            let text = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes =
                unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
            let r = unsafe { RegSetValueExW(key, VALUE, None, REG_SZ, Some(bytes)) };
            if r != ERROR_SUCCESS {
                return Err(anyhow!("RegSetValueExW failed: {}", r.0));
            }
        } else {
            let r = unsafe { RegDeleteValueW(key, VALUE) };
            if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
                return Err(anyhow!("RegDeleteValueW failed: {}", r.0));
            }
        }
        Ok(())
    })();
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::{RegQueryValueExW, KEY_READ};

    fn read_value() -> Option<String> {
        let key = open(KEY_READ).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        let r = unsafe {
            RegQueryValueExW(key, VALUE, None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len))
        };
        unsafe {
            let _ = RegCloseKey(key);
        }
        if r != ERROR_SUCCESS {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string())
    }

    #[test]
    fn roundtrip() {
        if read_value().is_some() {
            eprintln!("Snowy Run value already exists; skipping autostart test");
            return;
        }
        set_autostart(true).unwrap();
        let v = read_value();
        set_autostart(false).unwrap();
        let after = read_value();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(v, Some(format!("\"{}\"", exe.display())));
        assert!(after.is_none());
        set_autostart(false).unwrap(); // missing value is fine
    }
}

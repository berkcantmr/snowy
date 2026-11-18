use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// Holds a named mutex for the app's lifetime.
pub struct InstanceGuard {
    handle: HANDLE,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Returns None if another Snowy main instance is already running.
pub fn single_instance() -> Option<InstanceGuard> {
    unsafe {
        let handle = CreateMutexW(None, false, w!("Local\\SnowyMainInstance")).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return None;
        }
        Some(InstanceGuard { handle })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_instance_refused() {
        let first = single_instance();
        assert!(first.is_some());
        assert!(single_instance().is_none());
        drop(first);
    }
}

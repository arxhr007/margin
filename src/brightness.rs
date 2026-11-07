//! Built-in display brightness through WMI. WMI calls take tens of milliseconds, so they run on a
//! worker thread and writes are coalesced: only the latest target is applied.

use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use windows::core::{w, BSTR, VARIANT};
use windows::Win32::Foundation::RPC_E_TOO_LATE;
use windows::Win32::System::Com::*;
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Wmi::*;

struct Shared {
    current: i32, // last known value, -1 = unknown
    pending: i32, // value waiting to be written, -1 = none
    refresh_wanted: bool,
    stopping: bool,
}

pub struct Brightness {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl Brightness {
    pub fn new() -> Self {
        let shared = Arc::new((
            Mutex::new(Shared { current: -1, pending: -1, refresh_wanted: true, stopping: false }),
            Condvar::new(),
        ));
        let for_worker = shared.clone();
        let worker = std::thread::Builder::new()
            .name("Margin brightness".into())
            .spawn(move || run(for_worker))
            .ok();
        Brightness { shared, worker }
    }

    /// Re-reads the hardware value (it may have changed through the keyboard keys).
    pub fn refresh(&self) {
        let (lock, cv) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).refresh_wanted = true;
        cv.notify_one();
    }

    /// Moves brightness by `delta` percent and returns the new target (or -1 if unknown/unavailable).
    pub fn nudge(&self, delta: i32) -> i32 {
        let (lock, cv) = &*self.shared;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        if s.current < 0 {
            return -1;
        }
        s.current = (s.current + delta).clamp(0, 100);
        s.pending = s.current;
        let value = s.current;
        drop(s);
        cv.notify_one();
        value
    }
}

impl Drop for Brightness {
    fn drop(&mut self) {
        let (lock, cv) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopping = true;
        cv.notify_one();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

fn run(shared: Arc<(Mutex<Shared>, Condvar)>) {
    let (lock, cv) = &*shared;
    let mut wmi: Option<Wmi> = None;
    let mut com_ready = false;
    loop {
        let (refresh, write) = {
            let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
            while !s.stopping && s.pending < 0 && !s.refresh_wanted {
                s = cv.wait(s).unwrap_or_else(|e| e.into_inner());
            }
            if s.stopping {
                break;
            }
            let refresh = s.refresh_wanted && s.pending < 0;
            s.refresh_wanted = false;
            let write = s.pending;
            s.pending = -1;
            (refresh, write)
        };

        if !com_ready {
            // COM must be initialised on this worker thread before any WMI call.
            com_ready = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        }
        if wmi.is_none() {
            match Wmi::connect() {
                Ok(w) => wmi = Some(w),
                Err(e) => {
                    crate::log::error(format!("brightness: WMI connect failed: {e}"));
                    continue;
                }
            }
        }
        let Some(w) = &wmi else { continue };

        if write >= 0 {
            if let Err(e) = w.write(write) {
                crate::log::error(format!("brightness: {e}"));
                wmi = None; // reconnect next time
            }
        } else if refresh {
            match w.read() {
                Ok(value) => {
                    let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if s.pending < 0 {
                        s.current = value;
                    }
                }
                Err(e) => {
                    crate::log::error(format!("brightness: {e}"));
                    wmi = None;
                }
            }
        }
    }
    drop(wmi);
    if com_ready {
        unsafe { CoUninitialize() };
    }
}

struct Wmi {
    services: IWbemServices,
}

impl Wmi {
    fn connect() -> windows::core::Result<Wmi> {
        unsafe {
            // Process-wide security setup; fails harmlessly when something already did it.
            if let Err(e) = CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_DEFAULT,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
                None,
            ) {
                if e.code() != RPC_E_TOO_LATE {
                    crate::log::debug(format!("CoInitializeSecurity: {e}"));
                }
            }
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let services = locator.ConnectServer(
                &BSTR::from("ROOT\\WMI"),
                &BSTR::new(),
                &BSTR::new(),
                &BSTR::new(),
                0,
                &BSTR::new(),
                None,
            )?;
            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )?;
            Ok(Wmi { services })
        }
    }

    fn first(&self, query: &str) -> windows::core::Result<Option<IWbemClassObject>> {
        unsafe {
            let enumerator = self.services.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(query),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )?;
            let mut objects: [Option<IWbemClassObject>; 1] = [None];
            let mut returned = 0u32;
            enumerator.Next(WBEM_INFINITE, &mut objects, &mut returned).ok()?;
            Ok(if returned == 0 { None } else { objects[0].take() })
        }
    }

    /// Current brightness of the active built-in panel in percent, or -1 when there is none.
    fn read(&self) -> windows::core::Result<i32> {
        let Some(obj) = self.first("SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE")? else {
            return Ok(-1);
        };
        unsafe {
            let mut value = VARIANT::default();
            obj.Get(w!("CurrentBrightness"), 0, &mut value, None, None)?;
            Ok(i32::try_from(&value).unwrap_or(-1))
        }
    }

    fn write(&self, value: i32) -> windows::core::Result<()> {
        let Some(instance) = self.first("SELECT * FROM WmiMonitorBrightnessMethods WHERE Active=TRUE")? else {
            return Ok(());
        };
        unsafe {
            let mut path = VARIANT::default();
            instance.Get(w!("__PATH"), 0, &mut path, None, None)?;
            let path = BSTR::try_from(&path)?;

            let mut class: Option<IWbemClassObject> = None;
            self.services.GetObject(
                &BSTR::from("WmiMonitorBrightnessMethods"),
                WBEM_FLAG_RETURN_WBEM_COMPLETE,
                None,
                Some(&mut class),
                None,
            )?;
            let class = class.ok_or_else(windows::core::Error::empty)?;
            let mut signature: Option<IWbemClassObject> = None;
            class.GetMethod(w!("WmiSetBrightness"), 0, &mut signature, std::ptr::null_mut())?;
            let params = signature.ok_or_else(windows::core::Error::empty)?.SpawnInstance(0)?;
            params.Put(w!("Timeout"), 0, &VARIANT::from(1i32), 0)?;
            params.Put(w!("Brightness"), 0, &VARIANT::from(value.clamp(0, 100)), 0)?;
            self.services.ExecMethod(
                &path,
                &BSTR::from("WmiSetBrightness"),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                &params,
                None,
                None,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to the real display through WMI, so it only runs on request:
    /// `cargo test brightness_round_trip -- --ignored --nocapture`. It rewrites the current value (no change).
    #[test]
    #[ignore]
    fn brightness_round_trip() {
        let b = Brightness::new();
        // the worker reads the value shortly after start
        let mut value = -1;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            value = b.nudge(0);
            if value >= 0 {
                break;
            }
        }
        println!("current brightness: {value}");
        assert!(value >= 0, "WMI could not read the built-in panel brightness");
        std::thread::sleep(std::time::Duration::from_millis(800)); // let the write finish
    }

    /// Changes the panel by one percent, reads it back from WMI, then restores it.
    #[test]
    #[ignore]
    fn brightness_write_is_applied_by_hardware() {
        let b = Brightness::new();
        let wait_for_value = |b: &Brightness| {
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let v = b.nudge(0);
                if v >= 0 {
                    return v;
                }
            }
            -1
        };
        let start = wait_for_value(&b);
        assert!(start >= 1 && start <= 99, "need a mid-range brightness to test, got {start}");
        let target = b.nudge(1);
        std::thread::sleep(std::time::Duration::from_millis(1200)); // let the write finish
        b.refresh();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let read_back = b.nudge(0);
        println!("start {start} -> asked {target} -> hardware says {read_back}");
        b.nudge(-1); // restore
        std::thread::sleep(std::time::Duration::from_millis(1200));
        b.refresh();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let restored = b.nudge(0);
        println!("restored: {restored}");
        assert_eq!(read_back, target, "the brightness write did not reach the hardware");
        assert_eq!(restored, start);
    }
}

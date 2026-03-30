//! 电池状态模块

use crate::types::BatteryStatus;

#[cfg(windows)]
use windows::Win32::System::Power::GetSystemPowerStatus;
#[cfg(windows)]
use windows::Win32::System::Power::SYSTEM_POWER_STATUS;

#[tauri::command]
pub fn get_battery_status() -> BatteryStatus {
    #[cfg(windows)]
    {
        let mut status = SYSTEM_POWER_STATUS::default();
        let ok = unsafe { GetSystemPowerStatus(&mut status) };
        if ok.is_ok() {
            let percentage = if status.BatteryLifePercent <= 100 {
                status.BatteryLifePercent as i32
            } else {
                -1
            };
            let is_charging =
                status.ACLineStatus == 1 || status.BatteryFlag & 8 != 0;
            let has_battery = status.BatteryFlag != 128;
            let remaining_seconds = if status.BatteryLifeTime != 0xFFFFFFFF {
                status.BatteryLifeTime as i32
            } else {
                -1
            };

            return BatteryStatus {
                has_battery,
                is_charging,
                percentage,
                remaining_seconds,
            };
        }
    }

    BatteryStatus {
        has_battery: false,
        is_charging: false,
        percentage: -1,
        remaining_seconds: -1,
    }
}

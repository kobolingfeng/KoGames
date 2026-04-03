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
            let battery_percent = if status.BatteryLifePercent <= 100 {
                status.BatteryLifePercent as i32
            } else {
                -1
            };
            let is_charging =
                status.ACLineStatus == 1 || status.BatteryFlag & 8 != 0;
            let is_ac_connected = status.ACLineStatus == 1;
            let has_battery = status.BatteryFlag != 128;
            let battery_life_time = if status.BatteryLifeTime != 0xFFFFFFFF {
                Some(status.BatteryLifeTime as i32)
            } else {
                None
            };
            let power_status = if is_charging { "Charging" } else if has_battery { "Battery" } else { "AC" };

            return BatteryStatus {
                has_battery,
                is_charging,
                is_ac_connected,
                battery_percent,
                battery_life_time,
                power_status: power_status.to_string(),
            };
        }
    }

    BatteryStatus {
        has_battery: false,
        is_charging: false,
        is_ac_connected: false,
        battery_percent: -1,
        battery_life_time: None,
        power_status: "Unknown".to_string(),
    }
}

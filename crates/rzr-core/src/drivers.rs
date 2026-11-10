//! Which driver serves each Razer device (Diagnostics → Drivers).
//!
//! open-rzr needs no driver of its own: it talks through the standard HID driver of the OS.
//! Razer Synapse installs driver packages (and on some models filter drivers) that stay after
//! Synapse is removed; this list shows them so the user can remove them in Device Manager.
//! Nothing is changed here.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DriverInfo {
    pub device: String,
    pub instance: String,
    pub provider: String,
    pub version: String,
    pub inf: String,
    pub service: String,
    /// upper and lower filter drivers, comma separated
    pub filters: String,
    /// a Razer package or a Razer filter driver
    pub razer: bool,
}

fn is_razer(provider: &str, service: &str, filters: &str) -> bool {
    let l = |s: &str| s.to_ascii_lowercase();
    l(provider).contains("razer")
        || filters.split(',').chain(std::iter::once(service)).any(|f| {
            let f = l(f.trim());
            f.starts_with("rz") || f.contains("razer")
        })
}

/// "name\tinstance\tprovider\tversion\tinf\tservice\tupper\tlower" lines (PowerShell output).
pub fn parse_lines(text: &str) -> Vec<DriverInfo> {
    let mut v = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        if f.len() < 8 || f[1].is_empty() {
            continue;
        }
        let filters: Vec<&str> = [f[6], f[7]].iter().flat_map(|x| x.split(',')).map(str::trim).filter(|x| !x.is_empty()).collect();
        let filters = filters.join(", ");
        v.push(DriverInfo {
            device: f[0].to_string(),
            instance: f[1].to_string(),
            provider: f[2].to_string(),
            version: f[3].to_string(),
            inf: f[4].to_string(),
            service: f[5].to_string(),
            razer: is_razer(f[2], f[5], &filters),
            filters,
        });
    }
    v
}

#[cfg(windows)]
pub fn list() -> Vec<DriverInfo> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let ps = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
        $keys = 'DEVPKEY_Device_DriverProvider','DEVPKEY_Device_DriverVersion','DEVPKEY_Device_DriverInfPath','DEVPKEY_Device_Service','DEVPKEY_Device_UpperFilters','DEVPKEY_Device_LowerFilters'; \
        Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match 'VID_1532|VID&021532' } | ForEach-Object { \
          $v = @{}; foreach ($x in (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName $keys -ErrorAction SilentlyContinue)) { $v[$x.KeyName] = ($x.Data -join ',') }; \
          \"$($_.FriendlyName)`t$($_.InstanceId)`t$($v[$keys[0]])`t$($v[$keys[1]])`t$($v[$keys[2]])`t$($v[$keys[3]])`t$($v[$keys[4]])`t$($v[$keys[5]])\" }";
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => parse_lines(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => {
            log_warn!("app", "driver list: {e}");
            Vec::new()
        }
    }
}

/// Linux: the kernel driver bound to each Razer HID device (hid-generic, razermouse, ...).
#[cfg(not(windows))]
pub fn list() -> Vec<DriverInfo> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/bus/hid/devices") else { return v };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.to_ascii_uppercase().contains(":1532:") {
            continue;
        }
        let uevent = std::fs::read_to_string(e.path().join("uevent")).unwrap_or_default();
        let device = uevent.lines().find_map(|l| l.strip_prefix("HID_NAME=")).unwrap_or("").to_string();
        let driver = std::fs::read_link(e.path().join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "—".into());
        v.push(DriverInfo {
            device,
            instance: name,
            provider: "Linux kernel".into(),
            razer: false,
            service: driver,
            ..Default::default()
        });
    }
    v.sort_by(|a, b| a.instance.cmp(&b.instance));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_powershell_lines() {
        let v = parse_lines(
            "Razer Naga Pro\tHID\\VID_1532&PID_008F&MI_00\\8&1\tRazer Inc\t10.0.15063.0\toem42.inf\tmouhid\t\t\r\n\
             HID-compliant mouse\tHID\\VID_1532&PID_008F&MI_01\\8&2\tMicrosoft\t10.0.26100.1\tmsmouse.inf\tmouhid\tRzFilter\t\r\n\
             USB Input Device\tUSB\\VID_1532&PID_008F&MI_02\\7&3\tMicrosoft\t10.0.26100.1\tinput.inf\tHidUsb\t\t\r\nbroken\n",
        );
        assert_eq!(v.len(), 3);
        assert!(v[0].razer);
        assert!(v[1].razer);
        assert_eq!(v[1].filters, "RzFilter");
        assert!(!v[2].razer);
    }
}

//! Linux: install the udev rule that lets the desktop user talk to the mouse.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::devices::{self, RAZER_VID};

pub const RULE_PATH: &str = "/etc/udev/rules.d/99-open-rzr.rules";

pub fn rule_text() -> String {
    let mut s = String::from(
        "# open-rzr: access to Razer mouse HID control for the logged-in user;\n\
         # input lines: read the mouse's own key events (F13-F24 triggers of macro program actions)\n",
    );
    for d in devices::all() {
        s += &format!("# {}\n", d.name);
        if d.bluetooth {
            // Bluetooth devices have no USB attributes: match the HID device name (bus 0005)
            s += &format!(
                "KERNEL==\"hidraw*\", SUBSYSTEM==\"hidraw\", KERNELS==\"0005:{:04X}:{:04X}.*\", TAG+=\"uaccess\"\n",
                RAZER_VID, d.pid
            );
            continue;
        }
        s += &format!(
            "KERNEL==\"hidraw*\", SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{:04x}\", ATTRS{{idProduct}}==\"{:04x}\", TAG+=\"uaccess\"\n",
            RAZER_VID, d.pid
        );
        s += &format!(
            "KERNEL==\"event*\", SUBSYSTEM==\"input\", ATTRS{{idVendor}}==\"{:04x}\", ATTRS{{idProduct}}==\"{:04x}\", TAG+=\"uaccess\"\n",
            RAZER_VID, d.pid
        );
    }
    s
}

pub fn rule_installed() -> bool {
    std::fs::read_to_string(RULE_PATH).map(|s| s == rule_text()).unwrap_or(false)
}

fn which(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

/// pkexec for the GUI, sudo in a terminal
fn elevator(graphical: bool) -> Option<&'static str> {
    if graphical && which("pkexec") {
        Some("pkexec")
    } else if which("sudo") {
        Some("sudo")
    } else {
        None
    }
}

/// Run `script` with `sh -c` as root, `input` on its stdin.
fn as_root(graphical: bool, script: &str, input: &[u8]) -> Result<(), String> {
    let elev = elevator(graphical).ok_or("no pkexec/sudo")?;
    crate::log_info!("app", "udev: running as root via {elev}");
    let mut child = Command::new(elev)
        .args(["sh", "-c", script])
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{elev}: {e}"))?;
    child.stdin.take().ok_or("stdin")?.write_all(input).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { "cancelled?".into() } else { err })
    }
}

/// Write the rule as root (pkexec for the GUI, sudo in a terminal).
pub fn install(graphical: bool) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("udev exists only on Linux".into());
    }
    if elevator(graphical).is_none() {
        return Err(format!("No pkexec/sudo. Create {RULE_PATH} manually:\n{}", rule_text()));
    }
    let script = format!(
        "cat > '{RULE_PATH}' && udevadm control --reload-rules && udevadm trigger --subsystem-match=hidraw --action=change"
    );
    match as_root(graphical, &script, rule_text().as_bytes()) {
        Ok(()) => {
            crate::log_info!("app", "udev rule installed: {RULE_PATH}");
            Ok(())
        }
        Err(e) => {
            crate::log_error!("app", "udev rule not installed: {e}");
            Err(format!("Could not install the udev rule: {e}"))
        }
    }
}

/// A rule file written by [`install`] exists (the .deb package keeps its own in /usr/lib and
/// removes it itself).
pub fn rule_present() -> bool {
    std::path::Path::new(RULE_PATH).exists()
}

/// Remove the rule written by [`install`] (as root).
pub fn uninstall(graphical: bool) -> Result<(), String> {
    if !cfg!(target_os = "linux") || !rule_present() {
        return Ok(());
    }
    if elevator(graphical).is_none() {
        return Err(format!("No pkexec/sudo. Remove {RULE_PATH} manually."));
    }
    let script = format!("rm -f '{RULE_PATH}' && udevadm control --reload-rules");
    match as_root(graphical, &script, b"") {
        Ok(()) => {
            crate::log_info!("app", "udev rule removed: {RULE_PATH}");
            Ok(())
        }
        Err(e) => {
            crate::log_error!("app", "udev rule not removed: {e}");
            Err(format!("Could not remove the udev rule: {e}"))
        }
    }
}

#[cfg(test)]
mod packaged_rule {
    /// packaging/linux/99-open-rzr.rules must match the generated rule (regenerate: open-rzr-cli install-udev --print)
    #[test]
    fn matches_generated() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/linux/99-open-rzr.rules");
        if let Ok(s) = std::fs::read_to_string(p) {
            // a Windows checkout may have turned the line ends into CRLF
            assert_eq!(s.replace("\r\n", "\n"), super::rule_text(), "regenerate packaging/linux/99-open-rzr.rules");
        }
    }
}

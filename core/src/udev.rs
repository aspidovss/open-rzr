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

/// Write the rule as root (pkexec for the GUI, sudo in a terminal).
pub fn install(graphical: bool) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("udev exists only on Linux".into());
    }
    let script = format!(
        "cat > '{RULE_PATH}' && udevadm control --reload-rules && udevadm trigger --subsystem-match=hidraw --action=change"
    );
    let elev = if graphical && which("pkexec") {
        "pkexec"
    } else if which("sudo") {
        "sudo"
    } else {
        return Err(format!("No pkexec/sudo. Create {RULE_PATH} manually:\n{}", rule_text()));
    };
    crate::log_info!("app", "installing udev rule via {elev}");
    let mut child = Command::new(elev)
        .args(["sh", "-c", &script])
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{elev}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(rule_text().as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        crate::log_info!("app", "udev rule installed: {RULE_PATH}");
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        crate::log_error!("app", "udev rule not installed: {err}");
        Err(format!("Could not install the udev rule{}", if err.is_empty() { " (cancelled?)".into() } else { format!(": {err}") }))
    }
}

#[cfg(test)]
mod packaged_rule {
    /// packaging/linux/99-open-rzr.rules must match the generated rule (regenerate: open-rzr-cli install-udev --print)
    #[test]
    fn matches_generated() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/linux/99-open-rzr.rules");
        if let Ok(s) = std::fs::read_to_string(p) {
            assert_eq!(s, super::rule_text(), "regenerate packaging/linux/99-open-rzr.rules");
        }
    }
}

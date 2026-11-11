//! Commands: lighting.

use crate::*;

#[derive(Serialize)]
pub(crate) struct LightingInfo {
    pub(crate) saved: HashMap<String, ZoneState>,
    pub(crate) brightness: HashMap<String, u8>,
}

#[tauri::command]
pub(crate) async fn get_lighting(st: State<'_, AppState>) -> R<LightingInfo> {
    let serial = lock(&st.serial).clone();
    let saved = lock(&st.store).lighting.get(&serial).cloned().unwrap_or_default();
    let brightness = with_mouse(&st, "brightness", |m| {
        let zones: Vec<&str> = m.zones.iter().map(|z| z.id).collect();
        let mut b = HashMap::new();
        for z in zones {
            if let Ok(v) = m.get_brightness(z) {
                b.insert(z.to_string(), v);
            }
        }
        Ok(b)
    })?;
    Ok(LightingInfo { saved, brightness })
}

#[tauri::command]
pub(crate) async fn set_lighting(st: State<'_, AppState>, targets: Vec<(String, ZoneState)>) -> R<()> {
    let serial = lock(&st.serial).clone();
    for (z, s) in &targets {
        log_info!("ui", "lighting {z}: {} {} brightness {}%", s.effect, lighting::hex_color(s.color1), s.brightness);
    }
    with_mouse(&st, "lighting", |m| {
        for (z, s) in &targets {
            m.set_lighting(z, s)?;
            m.set_brightness(z, s.brightness)?;
        }
        Ok(())
    })?;
    let mut store = lock(&st.store);
    // lighting belongs to the selected profile too
    if let Some(id) = store.current_profile.get(&serial).cloned() {
        if let Ok(mut p) = library::load(&id) {
            for (z, s) in &targets {
                p.lighting.insert(z.clone(), s.clone());
            }
            let _ = library::save(&p);
        }
    }
    let e = store.lighting.entry(serial).or_default();
    for (z, s) in targets {
        e.insert(z, s);
    }
    store.save();
    Ok(())
}

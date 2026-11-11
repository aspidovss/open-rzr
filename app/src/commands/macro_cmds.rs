//! Commands: macros.

use crate::*;

#[derive(Serialize)]
pub(crate) struct MouseMacro {
    pub(crate) id: u16,
    pub(crate) steps: Vec<MacroStep>,
    pub(crate) text: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct MacrosOnMouse {
    pub(crate) macros: Vec<MouseMacro>,
    pub(crate) flash: Option<(u32, u32, u32)>,
}

#[tauri::command]
pub(crate) async fn macros_on_mouse(st: State<'_, AppState>) -> R<MacrosOnMouse> {
    with_mouse(&st, "read macros", |m| {
        let mut out = Vec::new();
        for id in m.macro_ids()? {
            let steps = m.read_macro(id)?;
            let text = steps.iter().map(|s| s.describe()).collect();
            out.push(MouseMacro { id, steps, text });
        }
        Ok(MacrosOnMouse { macros: out, flash: m.flash_usage().ok() })
    })
}

/// Build the program-actions table from the library (trigger key -> actions).
pub(crate) fn program_table(store: &Store) -> HashMap<u8, (String, Vec<SoftAction>)> {
    store
        .macros
        .iter()
        .filter_map(|(n, m)| {
            let acts: Vec<SoftAction> = m.software.iter().filter(|a| a.on).cloned().collect();
            match (m.trigger, acts.is_empty()) {
                (Some(k), false) => Some((k, (n.clone(), acts))),
                _ => None,
            }
        })
        .collect()
}

pub(crate) fn refresh_program_table(st: &AppState) {
    let t = program_table(&lock(&st.store));
    *lock(&st.actions) = t;
    program_actions::refresh();
}

/// Compile an editor document; other library macros can be nested by name.
/// A trigger key is chosen when there are program actions: the macro's own, else a free F13–F24.
pub(crate) fn compile_doc(store: &Store, name: &str, items: &[Item], software: &[SoftAction]) -> Result<Compiled, String> {
    let trigger = if software.iter().any(|a| a.on) {
        let own = store.macros.get(name).and_then(|m| m.trigger);
        let used: Vec<u8> = store.macros.iter().filter(|(n, _)| n.as_str() != name).filter_map(|(_, m)| m.trigger).collect();
        match own.filter(|k| !used.contains(k)).or_else(|| macro_doc::free_trigger(&used)) {
            Some(k) => Some(k),
            None => return Err("[triggers|] all trigger keys F13–F24 are taken by other macros with program actions".into()),
        }
    } else {
        None
    };
    let lookup = |n: &str| store.macros.get(n).map(|m| m.editor_items());
    macro_doc::compile(name, items, trigger, &lookup)
}

#[derive(Serialize)]
pub(crate) struct MacroCheck {
    pub(crate) ok: bool,
    pub(crate) error: String,
    pub(crate) steps: usize,
    pub(crate) size: usize,
    pub(crate) ms: i64,
    pub(crate) trigger: String,
    pub(crate) lines: Vec<String>,
}

/// Live check for the editor: does it fit into the mouse, how long it takes, trigger key.
#[tauri::command]
pub(crate) async fn check_macro(st: State<'_, AppState>, name: String, items: Vec<Item>, software: Vec<SoftAction>) -> R<MacroCheck> {
    let s = lock(&st.store);
    let lines = items.iter().map(macro_doc::describe).collect();
    Ok(match compile_doc(&s, &name, &items, &software) {
        Ok(c) => MacroCheck {
            ok: true,
            error: String::new(),
            steps: c.steps.len(),
            size: c.size,
            ms: c.ms,
            trigger: c.trigger.map(program_actions::trigger_name).unwrap_or_default(),
            lines,
        },
        Err(e) => MacroCheck { ok: false, error: e, steps: 0, size: 0, ms: 0, trigger: String::new(), lines },
    })
}

pub(crate) fn store_doc(st: &AppState, name: &str, id: u16, items: Vec<Item>, software: Vec<SoftAction>) -> Result<Compiled, String> {
    let mut s = lock(&st.store);
    let compiled = compile_doc(&s, name, &items, &software);
    let (steps, trigger) = match &compiled {
        Ok(c) => (c.steps.clone(), c.trigger),
        Err(_) => (s.macros.get(name).map(|m| m.steps.clone()).unwrap_or_default(), s.macros.get(name).and_then(|m| m.trigger)),
    };
    s.macros.insert(name.to_string(), SavedMacro { id, steps, items, software, trigger });
    s.save();
    compiled
}

#[derive(Serialize)]
pub(crate) struct Written {
    pub(crate) bytes: usize,
    pub(crate) trigger: String,
}

#[tauri::command]
pub(crate) async fn write_macro(st: State<'_, AppState>, id: u16, name: String, items: Vec<Item>, software: Vec<SoftAction>) -> R<Written> {
    log_info!("ui", "write macro #{id} \"{name}\" ({} steps, {} program actions)", items.len(), software.len());
    let c = store_doc(&st, &name, id, items, software).map_err(|e| format!("[invalid] {e}"))?;
    refresh_program_table(&st);
    let bytes = with_mouse(&st, "write macro", |m| m.write_macro(id, &c.steps))?;
    Ok(Written { bytes, trigger: c.trigger.map(program_actions::trigger_name).unwrap_or_default() })
}

#[tauri::command]
pub(crate) async fn delete_macro(st: State<'_, AppState>, id: u16) -> R<()> {
    log_info!("ui", "delete macro #{id} from the mouse");
    with_mouse(&st, "delete macro", |m| m.delete_macro(id))
}

#[derive(Serialize)]
pub(crate) struct LibMacro {
    pub(crate) name: String,
    pub(crate) id: u16,
    pub(crate) items: Vec<Item>,
    pub(crate) software: Vec<SoftAction>,
    pub(crate) trigger: String,
}

#[tauri::command]
pub(crate) async fn macro_library(st: State<'_, AppState>) -> R<Vec<LibMacro>> {
    Ok(lock(&st.store)
        .macros
        .iter()
        .map(|(n, m)| LibMacro {
            name: n.clone(),
            id: m.id,
            items: m.editor_items(),
            software: m.software.clone(),
            trigger: m.trigger.map(program_actions::trigger_name).unwrap_or_default(),
        })
        .collect())
}

/// Save to the library. The document is kept even if it does not compile; the error is returned.
#[tauri::command]
pub(crate) async fn save_macro_lib(st: State<'_, AppState>, name: String, id: u16, items: Vec<Item>, software: Vec<SoftAction>) -> R<String> {
    log_info!("ui", "library: saved \"{name}\" (ID {id})");
    let r = store_doc(&st, &name, id, items, software);
    refresh_program_table(&st);
    Ok(r.err().unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn rename_macro_lib(st: State<'_, AppState>, old: String, name: String) -> R<()> {
    let name = name.trim().to_string();
    let mut s = lock(&st.store);
    if name.is_empty() || s.macros.contains_key(&name) {
        return Err("[invalid] name is empty or taken".into());
    }
    let m = s.macros.remove(&old).ok_or("[not_found] macro")?;
    s.macros.insert(name.clone(), m);
    // nested references follow the new name
    for m in s.macros.values_mut() {
        for it in m.items.iter_mut().filter(|i| i.kind == "macro" && i.text == old) {
            it.text = name.clone();
        }
    }
    s.save();
    log_info!("ui", "library: \"{old}\" renamed to \"{name}\"");
    drop(s);
    refresh_program_table(&st);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_macro_lib(st: State<'_, AppState>, name: String) -> R<()> {
    log_info!("ui", "library: removed \"{name}\"");
    {
        let mut s = lock(&st.store);
        s.macros.remove(&name);
        s.save();
    }
    refresh_program_table(&st);
    Ok(())
}

/// Try the program actions now (from the editor).
#[tauri::command]
pub(crate) async fn test_program_actions(software: Vec<SoftAction>) -> R<()> {
    std::thread::spawn(move || {
        if let Err(e) = program_actions::run(&software) {
            log_warn!("app", "program actions test: {e}");
        }
    });
    Ok(())
}

#[derive(Serialize)]
pub(crate) struct ActionsStatus {
    pub(crate) status: program_actions::Status,
    /// trigger key -> macro name
    pub(crate) bound: Vec<(String, String)>,
}

#[tauri::command]
pub(crate) async fn program_actions_status(st: State<'_, AppState>) -> R<ActionsStatus> {
    let mut bound: Vec<(String, String)> =
        lock(&st.actions).iter().map(|(k, (n, _))| (program_actions::trigger_name(*k), n.clone())).collect();
    bound.sort();
    Ok(ActionsStatus { status: program_actions::status(), bound })
}

#[tauri::command]
pub(crate) async fn describe_steps(steps: Vec<MacroStep>) -> R<(Vec<String>, usize)> {
    let size = macros::encode(&steps).map(|d| d.len()).unwrap_or(0);
    Ok((steps.iter().map(|s| s.describe()).collect(), size))
}

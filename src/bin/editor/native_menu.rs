//! Native menu for the city editor.
use super::*;

pub(super) struct NativeFileMenu {
    pub(super) _menu: muda::Menu,
    pub(super) items: [(muda::MenuId, FileAction); 4],
}

#[cfg(target_os = "macos")]
impl NativeFileMenu {
    pub(super) fn install() -> Option<Self> {
        use muda::{
            accelerator::{Accelerator, Code, Modifiers},
            Menu, MenuItem, PredefinedMenuItem, Submenu,
        };
        let menu = Menu::new();
        let app = Submenu::new("OpenCity", true);
        app.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .ok()?;
        let file = Submenu::new("File", true);
        let new = MenuItem::new(
            "Clear",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyN)),
        );
        let open = MenuItem::new(
            "Open…",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyO)),
        );
        let save = MenuItem::new(
            "Save",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyS)),
        );
        let save_as = MenuItem::new(
            "Save As…",
            true,
            Some(Accelerator::new(
                Some(Modifiers::META | Modifiers::SHIFT),
                Code::KeyS,
            )),
        );
        file.append_items(&[
            &open,
            &save,
            &save_as,
            &PredefinedMenuItem::separator(),
            &new,
        ])
        .ok()?;
        menu.append_items(&[&app, &file]).ok()?;
        menu.init_for_nsapp();
        Some(Self {
            _menu: menu,
            items: [
                (new.id().clone(), FileAction::New),
                (open.id().clone(), FileAction::Open),
                (save.id().clone(), FileAction::Save),
                (save_as.id().clone(), FileAction::SaveAs),
            ],
        })
    }

    pub(super) fn drain(&self) -> Vec<FileAction> {
        let mut actions = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some((_, action)) = self.items.iter().find(|(id, _)| *id == event.id) {
                actions.push(*action);
            }
        }
        actions
    }
}

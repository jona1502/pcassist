use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct Tray {
    pub _icon: TrayIcon,
    pub open_id: tray_icon::menu::MenuId,
    pub settings_id: tray_icon::menu::MenuId,
    pub quit_id: tray_icon::menu::MenuId,
}

fn circle_icon() -> Icon {
    const N: u32 = 32;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let inside = dx * dx + dy * dy <= 15.0 * 15.0;
            rgba.extend_from_slice(if inside {
                &[0x2d, 0x8c, 0xf0, 0xff]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    Icon::from_rgba(rgba, N, N).expect("valid icon")
}

pub fn create() -> Result<Tray, Box<dyn std::error::Error>> {
    let open = MenuItem::new("Panel öffnen", true, None);
    let settings = MenuItem::new("Einstellungen", true, None);
    let quit = MenuItem::new("Beenden", true, None);
    let menu = Menu::new();
    menu.append_items(&[&open, &settings, &quit])?;
    let icon = TrayIconBuilder::new()
        .with_icon(circle_icon())
        .with_tooltip("PC Assistant")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;
    Ok(Tray {
        _icon: icon,
        open_id: open.id().clone(),
        settings_id: settings.id().clone(),
        quit_id: quit.id().clone(),
    })
}

//! The app's icons: Lucide SVGs (ISC; license and credits in `assets/icons/LICENSE` and the
//! readme), embedded unmodified. Rune draws an `<svg>` root as a group, so each file's stroke
//! attributes apply and `currentColor` takes the element's color.

use rune::Icon;

const fn lucide(name: &'static str, body: &'static str) -> Icon {
    Icon {
        name,
        view: 24.0,
        body,
    }
}

pub static SIDEBAR: Icon = lucide(
    "panel-left",
    include_str!("../../assets/icons/panel-left.svg"),
);
pub static CONNECT: Icon = lucide("plug", include_str!("../../assets/icons/plug.svg"));
pub static DISCONNECT: Icon = lucide("unplug", include_str!("../../assets/icons/unplug.svg"));
pub static ANTENNA: Icon = lucide(
    "radio-tower",
    include_str!("../../assets/icons/radio-tower.svg"),
);
pub static ARCHIVE: Icon = lucide("archive", include_str!("../../assets/icons/archive.svg"));
pub static FOLDER: Icon = lucide(
    "folder-open",
    include_str!("../../assets/icons/folder-open.svg"),
);

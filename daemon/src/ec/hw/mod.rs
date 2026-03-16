pub use super::EcDevice;

mod getters;
mod power_profile;
mod kbd_backlight;
mod led;
mod fan;
mod flexicharger;

pub use power_profile::*;
pub use getters::*;
pub use kbd_backlight::*;
pub use led::*;
pub use fan::*;
pub use flexicharger::*;

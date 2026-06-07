pub use super::EcDevice;

mod fan;
mod flexicharger;
mod getters;
mod kbd_backlight;
mod led;
mod power_profile;

pub use fan::*;
pub use flexicharger::*;
pub use getters::*;
pub use kbd_backlight::*;
pub use led::*;
pub use power_profile::*;

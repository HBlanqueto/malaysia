use std::sync::LazyLock;

use ctappearance::AppearanceConfig;

pub static APPEARANCE: LazyLock<AppearanceConfig> = LazyLock::new(ctappearance::load_appearance);

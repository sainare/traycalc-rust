use std::{env, fs, path::PathBuf};

pub const DEFAULT_HOTKEY: u32 = 0x0002_0000 | 0x0004_0000 | 0x43;

#[derive(Clone, Debug)]
pub struct Settings {
    pub hotkey: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub opacity: u8,
    pub theme: u8,
    pub accent: u32,
    pub font_scale: u16,
    pub on_top: bool,
    pub show_at: u8,
    pub hide_on_blur: bool,
    pub show_tip: bool,
    pub block_key: bool,
    pub in_taskbar: bool,
    pub auto_start: bool,
    pub decimal_comma: bool,
    pub expression: String,
    pub history_line: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY,
            x: -1,
            y: -1,
            width: 340,
            height: 500,
            opacity: 100,
            theme: 0,
            accent: 0x00f58b4c,
            font_scale: 100,
            on_top: true,
            show_at: 0,
            hide_on_blur: false,
            show_tip: true,
            block_key: true,
            in_taskbar: true,
            auto_start: true,
            decimal_comma: false,
            expression: String::new(),
            history_line: String::new(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut value = Self::default();
        let Ok(text) = fs::read_to_string(settings_path()) else { return value };
        for line in text.lines() {
            let Some((key, raw)) = line.split_once('=') else { continue };
            let int = raw.parse::<i64>().unwrap_or_default();
            match key {
                "hotkey" => value.hotkey = int as u32,
                "x" => value.x = int as i32,
                "y" => value.y = int as i32,
                "w" if int >= 240 => value.width = int as i32,
                "h" if int >= 320 => value.height = int as i32,
                "opacity" if (30..=100).contains(&int) => value.opacity = int as u8,
                "theme" => value.theme = (int == 1) as u8,
                "accent" => value.accent = int as u32 & 0x00ff_ffff,
                "fontscale" if (70..=150).contains(&int) => value.font_scale = int as u16,
                "ontop" => value.on_top = int != 0,
                "showat" if (0..=3).contains(&int) => value.show_at = int as u8,
                "hideonblur" => value.hide_on_blur = int != 0,
                "showtip" => value.show_tip = int != 0,
                "blockkey" => value.block_key = int != 0,
                "taskbar" => value.in_taskbar = int != 0,
                "autostart" => value.auto_start = int != 0,
                "decimal" => value.decimal_comma = int == 1,
                "expr" => value.expression = raw.to_owned(),
                "history" => value.history_line = raw.to_owned(),
                _ => {}
            }
        }
        value
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let clean = |s: &str| s.replace(['\r', '\n'], "");
        let text = format!(
            "hotkey={}\nx={}\ny={}\nw={}\nh={}\nopacity={}\ntheme={}\naccent={}\nfontscale={}\nontop={}\nshowat={}\nhideonblur={}\nshowtip={}\nblockkey={}\ntaskbar={}\nautostart={}\ndecimal={}\nexpr={}\nhistory={}\n",
            self.hotkey, self.x, self.y, self.width, self.height, self.opacity,
            self.theme, self.accent as i32, self.font_scale, self.on_top as u8,
            self.show_at, self.hide_on_blur as u8, self.show_tip as u8,
            self.block_key as u8, self.in_taskbar as u8, self.auto_start as u8,
            self.decimal_comma as u8,
            clean(&self.expression), clean(&self.history_line),
        );
        let _ = fs::write(path, text);
    }
}

pub fn app_dir() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("TrayCalc")
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.ini")
}


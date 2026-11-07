//! Settings model. The JSON written here is the same format the C# app uses
//! (`%APPDATA%\Margin\settings.json`: camelCase keys, string enums), so both apps share one file.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

/// The eight touchpad regions a user can assign.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Section {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Section {
    pub const ALL: [Section; 8] = [
        Section::Left,
        Section::Right,
        Section::Top,
        Section::Bottom,
        Section::TopLeft,
        Section::TopRight,
        Section::BottomLeft,
        Section::BottomRight,
    ];

    /// The key used in settings.json.
    pub fn key(self) -> &'static str {
        match self {
            Section::Left => "left",
            Section::Right => "right",
            Section::Top => "top",
            Section::Bottom => "bottom",
            Section::TopLeft => "topLeft",
            Section::TopRight => "topRight",
            Section::BottomLeft => "bottomLeft",
            Section::BottomRight => "bottomRight",
        }
    }

    pub fn is_corner(self) -> bool {
        matches!(
            self,
            Section::TopLeft | Section::TopRight | Section::BottomLeft | Section::BottomRight
        )
    }

    /// Left and right edges are slid up/down; top and bottom left/right.
    pub fn is_vertical(self) -> bool {
        matches!(self, Section::Left | Section::Right)
    }

    pub fn name(self) -> &'static str {
        match self {
            Section::Left => "Left edge",
            Section::Right => "Right edge",
            Section::Top => "Top edge",
            Section::Bottom => "Bottom edge",
            Section::TopLeft => "Top-left corner",
            Section::TopRight => "Top-right corner",
            Section::BottomLeft => "Bottom-left corner",
            Section::BottomRight => "Bottom-right corner",
        }
    }
}

/// What sliding along an edge controls. The order is the order of the settings drop-down
/// and must match the C# enum so saved files mean the same thing in both apps.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum SlideAction {
    #[default]
    Off,
    Volume,
    Brightness,
    Seek,
    Zoom,
    Tabs,
    Tracks,
    Desktops,
    Custom,
}

impl SlideAction {
    pub const ALL: [SlideAction; 9] = [
        SlideAction::Off,
        SlideAction::Volume,
        SlideAction::Brightness,
        SlideAction::Seek,
        SlideAction::Zoom,
        SlideAction::Tabs,
        SlideAction::Tracks,
        SlideAction::Desktops,
        SlideAction::Custom,
    ];
}

/// What clicking a corner does.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum ClickAction {
    #[default]
    None,
    Mute,
    PlayPause,
    NextTrack,
    PrevTrack,
    Screenshot,
    TaskView,
    ShowDesktop,
    Lock,
    Notifications,
    App,
    Url,
    Keys,
    Command,
}

impl ClickAction {
    pub const ALL: [ClickAction; 14] = [
        ClickAction::None,
        ClickAction::Mute,
        ClickAction::PlayPause,
        ClickAction::NextTrack,
        ClickAction::PrevTrack,
        ClickAction::Screenshot,
        ClickAction::TaskView,
        ClickAction::ShowDesktop,
        ClickAction::Lock,
        ClickAction::Notifications,
        ClickAction::App,
        ClickAction::Url,
        ClickAction::Keys,
        ClickAction::Command,
    ];
}

/// How an edge slide begins.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Activation {
    /// Press the edge until the touchpad clicks, then slide while holding.
    Press,
    /// Slide a finger in from the laptop body onto the edge, then along it. No click needed.
    #[default]
    SlideIn,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Speed {
    Slow,
    #[default]
    Medium,
    Fast,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct ClickSpec {
    #[serde(rename = "do")]
    pub action: ClickAction,
    /// App path, URL, key combo ("Ctrl+Shift+Esc") or command line, depending on the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SectionSettings {
    /// Used by the four edges.
    pub slide: SlideAction,
    /// Used by the four corners: index 0 = single click, 1 = double, 2 = triple.
    pub clicks: Vec<ClickSpec>,
    /// Custom keys: shortcut sent per step when sliding right / up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward: Option<String>,
    /// Custom keys: shortcut sent per step when sliding left / down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back: Option<String>,
    /// Custom keys: keep the modifiers both shortcuts share (e.g. Alt) held for the whole slide.
    pub hold_modifiers: bool,
}

impl SectionSettings {
    const EMPTY: SectionSettings = SectionSettings {
        slide: SlideAction::Off,
        clicks: Vec::new(),
        forward: None,
        back: None,
        hold_modifiers: false,
    };

    fn slide(action: SlideAction) -> Self {
        SectionSettings { slide: action, ..Self::EMPTY }
    }

    fn clicks(list: &[ClickAction]) -> Self {
        SectionSettings {
            clicks: list
                .iter()
                .map(|&a| ClickSpec { action: a, value: None })
                .collect(),
            ..Self::EMPTY
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub enabled: bool,
    pub activation: Activation,
    pub speed: Speed,
    /// Depth of the edge strips, as a fraction of the touchpad.
    pub edge_depth: f64,
    /// Side of each corner square.
    pub corner_size: f64,
    /// Slide-in mode: a finger must first appear this close to the edge.
    pub entry_strip: f64,
    pub sections: IndexMap<String, SectionSettings>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            activation: Activation::SlideIn,
            speed: Speed::Medium,
            edge_depth: 0.10,
            corner_size: 0.15,
            entry_strip: 0.04,
            sections: defaults(),
        }
    }
}

fn defaults() -> IndexMap<String, SectionSettings> {
    use ClickAction as C;
    let mut m = IndexMap::new();
    m.insert(Section::Left.key().into(), SectionSettings::slide(SlideAction::Volume));
    m.insert(Section::Right.key().into(), SectionSettings::slide(SlideAction::Brightness));
    m.insert(Section::Top.key().into(), SectionSettings::slide(SlideAction::Tabs));
    m.insert(Section::Bottom.key().into(), SectionSettings::slide(SlideAction::Desktops));
    m.insert(Section::TopLeft.key().into(), SectionSettings::clicks(&[C::ShowDesktop]));
    m.insert(
        Section::TopRight.key().into(),
        SectionSettings::clicks(&[C::TaskView, C::Notifications]),
    );
    m.insert(Section::BottomLeft.key().into(), SectionSettings::clicks(&[C::Lock]));
    m.insert(
        Section::BottomRight.key().into(),
        SectionSettings::clicks(&[C::PlayPause, C::NextTrack, C::PrevTrack]),
    );
    m
}

impl Settings {
    /// Settings of one section (an unassigned default when the file doesn't list it).
    pub fn section(&self, s: Section) -> &SectionSettings {
        static EMPTY: OnceLock<SectionSettings> = OnceLock::new();
        self.sections
            .get(s.key())
            .unwrap_or_else(|| EMPTY.get_or_init(|| SectionSettings::EMPTY))
    }

    /// Mutable access, creating the section when missing.
    pub fn section_mut(&mut self, s: Section) -> &mut SectionSettings {
        self.sections.entry(s.key().to_string()).or_default()
    }

    /// Clamps values and makes sure every section exists. Call after loading or editing raw JSON.
    pub fn normalize(&mut self) {
        self.edge_depth = self.edge_depth.clamp(0.05, 0.25);
        self.corner_size = self.corner_size.clamp(0.08, 0.30);
        self.entry_strip = self.entry_strip.clamp(0.01, 0.10);
        for s in Section::ALL {
            let v = self.section_mut(s);
            v.clicks.truncate(3);
        }
    }
}

// ---------------------------------------------------------------- storage

pub fn directory() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Margin")
}

pub fn file_path() -> PathBuf {
    directory().join("settings.json")
}

pub fn exists() -> bool {
    file_path().exists()
}

pub fn load() -> Settings {
    let mut settings = match std::fs::read_to_string(file_path()) {
        Ok(text) => parse(&text).unwrap_or_else(|e| {
            crate::log::error(format!("settings.json unreadable, using defaults: {e}"));
            Settings::default()
        }),
        Err(_) => Settings::default(),
    };
    settings.normalize();
    settings
}

pub fn save(settings: &Settings) {
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(directory())?;
        let path = file_path();
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)
    })();
    if let Err(e) = result {
        crate::log::error(format!("could not save settings: {e}"));
    }
}

/// Parses settings text. Like the C# app this accepts `//` and `/* */` comments and trailing commas.
pub fn parse(text: &str) -> Result<Settings, serde_json::Error> {
    serde_json::from_str(&strip_jsonc(text.trim_start_matches('\u{feff}')))
}

/// Removes comments and trailing commas from JSON-with-comments, leaving strings untouched.
fn strip_jsonc(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == ',' {
            // a comma directly before } or ] (ignoring whitespace and comments) is a trailing comma
            let mut j = i + 1;
            loop {
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if chars.get(j) == Some(&'/') && chars.get(j + 1) == Some(&'/') {
                    while j < chars.len() && chars[j] != '\n' {
                        j += 1;
                    }
                } else if chars.get(j) == Some(&'/') && chars.get(j + 1) == Some(&'*') {
                    j += 2;
                    while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '/') {
                        j += 1;
                    }
                    j += 2;
                } else {
                    break;
                }
            }
            if !matches!(chars.get(j), Some('}') | Some(']')) {
                out.push(c);
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file exactly as the C# app writes it (the user's real settings with a custom-keys top edge).
    const CSHARP_FILE: &str = r#"{
  "enabled": true,
  "activation": "slideIn",
  "speed": "fast",
  "edgeDepth": 0.13,
  "cornerSize": 0.15,
  "entryStrip": 0.04,
  "sections": {
    "left": { "slide": "volume", "clicks": [], "holdModifiers": false },
    "right": { "slide": "brightness", "clicks": [], "holdModifiers": false },
    "top": {
      "slide": "custom",
      "clicks": [],
      "forward": "Alt+Tab",
      "back": "Alt+Shift+Tab",
      "holdModifiers": true
    },
    "bottom": { "slide": "seek", "clicks": [], "holdModifiers": false },
    "topLeft": { "slide": "off", "clicks": [ { "do": "command" } ], "holdModifiers": false },
    "topRight": {
      "slide": "off",
      "clicks": [ { "do": "lock" }, { "do": "taskView" } ],
      "holdModifiers": false
    },
    "bottomLeft": { "slide": "off", "clicks": [], "holdModifiers": false },
    "bottomRight": { "slide": "off", "clicks": [], "holdModifiers": false }
  }
}"#;

    #[test]
    fn reads_the_csharp_file() {
        let s = parse(CSHARP_FILE).unwrap();
        assert_eq!(s.speed, Speed::Fast);
        assert_eq!(s.activation, Activation::SlideIn);
        assert!((s.edge_depth - 0.13).abs() < 1e-9);
        let top = s.section(Section::Top);
        assert_eq!(top.slide, SlideAction::Custom);
        assert_eq!(top.forward.as_deref(), Some("Alt+Tab"));
        assert_eq!(top.back.as_deref(), Some("Alt+Shift+Tab"));
        assert!(top.hold_modifiers);
        let tr = s.section(Section::TopRight);
        assert_eq!(tr.clicks[0].action, ClickAction::Lock);
        assert_eq!(tr.clicks[1].action, ClickAction::TaskView);
        assert_eq!(s.section(Section::TopLeft).clicks[0].action, ClickAction::Command);
    }

    #[test]
    fn round_trips_and_keeps_json_shape() {
        let s = parse(CSHARP_FILE).unwrap();
        let text = serde_json::to_string_pretty(&s).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["activation"], "slideIn");
        assert_eq!(v["sections"]["top"]["slide"], "custom");
        assert_eq!(v["sections"]["top"]["holdModifiers"], true);
        assert_eq!(v["sections"]["topRight"]["clicks"][1]["do"], "taskView");
        // null values are skipped, like the C# WhenWritingNull option
        assert!(v["sections"]["left"].get("forward").is_none());
        assert!(v["sections"]["topRight"]["clicks"][0].get("value").is_none());
        let again = parse(&text).unwrap();
        assert_eq!(s, again);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s = parse("{}").unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.section(Section::Top).slide, SlideAction::Tabs);
        assert_eq!(s.section(Section::BottomRight).clicks.len(), 3);
    }

    #[test]
    fn accepts_comments_and_trailing_commas() {
        let text = r#"{
            // speed up
            "speed": "slow", /* inline */
            "entryStrip": 0.02,
            "sections": { "top": { "slide": "zoom", }, },
        }"#;
        let s = parse(text).unwrap();
        assert_eq!(s.speed, Speed::Slow);
        assert_eq!(s.section(Section::Top).slide, SlideAction::Zoom);
    }

    #[test]
    fn comment_markers_inside_strings_are_kept() {
        let text = r#"{ "sections": { "topLeft": { "clicks": [ { "do": "url", "value": "https://a.b/c,]" } ] } } }"#;
        let s = parse(text).unwrap();
        assert_eq!(
            s.section(Section::TopLeft).clicks[0].value.as_deref(),
            Some("https://a.b/c,]")
        );
    }

    #[test]
    fn normalize_clamps_and_limits_clicks() {
        let mut s = Settings { edge_depth: 9.0, corner_size: 0.0, entry_strip: -1.0, ..Settings::default() };
        s.section_mut(Section::TopLeft).clicks = vec![ClickSpec::default(); 5];
        s.normalize();
        assert_eq!(s.edge_depth, 0.25);
        assert_eq!(s.corner_size, 0.08);
        assert_eq!(s.entry_strip, 0.01);
        assert_eq!(s.section(Section::TopLeft).clicks.len(), 3);
    }

    #[test]
    fn enum_order_matches_csharp() {
        // The drop-down index is the enum index, so the order is part of the file format contract.
        assert_eq!(SlideAction::Custom as usize, 8);
        assert_eq!(SlideAction::Tabs as usize, 5);
        assert_eq!(ClickAction::Command as usize, 13);
        for (i, a) in SlideAction::ALL.iter().enumerate() {
            assert_eq!(*a as usize, i);
        }
        for (i, a) in ClickAction::ALL.iter().enumerate() {
            assert_eq!(*a as usize, i);
        }
    }
}

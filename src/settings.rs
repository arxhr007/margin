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
    Scroll,
}

impl SlideAction {
    pub const ALL: [SlideAction; 10] = [
        SlideAction::Off,
        SlideAction::Volume,
        SlideAction::Brightness,
        SlideAction::Seek,
        SlideAction::Zoom,
        SlideAction::Tabs,
        SlideAction::Tracks,
        SlideAction::Desktops,
        SlideAction::Custom,
        SlideAction::Scroll,
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
    /// Used by the four corners: pressing and holding the corner.
    #[serde(skip_serializing_if = "ClickSpec::is_unset")]
    pub long_press: ClickSpec,
    /// Whether the touchpad must click to use this section, or a touch is enough. Unset: edges follow
    /// `activation`, corners need a click. See [`Settings::needs_click`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_click: Option<bool>,
}

impl ClickSpec {
    pub const NONE: ClickSpec = ClickSpec { action: ClickAction::None, value: None };

    pub fn is_unset(&self) -> bool {
        self.action == ClickAction::None
    }
}

/// How many actions a corner editor shows: 1, 2 and 3 clicks, then press and hold.
pub const CORNER_SLOTS: usize = 4;
pub const LONG_PRESS_SLOT: usize = 3;

impl SectionSettings {
    const EMPTY: SectionSettings = SectionSettings {
        slide: SlideAction::Off,
        clicks: Vec::new(),
        forward: None,
        back: None,
        hold_modifiers: false,
        long_press: ClickSpec::NONE,
        needs_click: None,
    };

    /// A corner action by editor slot: 0-2 are 1-3 clicks, [`LONG_PRESS_SLOT`] is press and hold.
    pub fn corner_slot(&self, slot: usize) -> ClickSpec {
        if slot == LONG_PRESS_SLOT {
            self.long_press.clone()
        } else {
            self.clicks.get(slot).cloned().unwrap_or_default()
        }
    }

    /// The same, for editing; missing click slots are created.
    pub fn corner_slot_mut(&mut self, slot: usize) -> &mut ClickSpec {
        if slot == LONG_PRESS_SLOT {
            return &mut self.long_press;
        }
        while self.clicks.len() <= slot {
            self.clicks.push(ClickSpec::default());
        }
        &mut self.clicks[slot]
    }

    /// Every corner action that is set, clicks first.
    pub fn corner_actions(&self) -> impl Iterator<Item = &ClickSpec> {
        self.clicks.iter().chain(std::iter::once(&self.long_press)).filter(|c| !c.is_unset())
    }

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
    /// Per-app sections that replace the ones above while that app is in front (Windows).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<AppProfile>,
}

/// Sections that apply while one app is the window in front. Sections it doesn't list follow the
/// settings for all apps.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AppProfile {
    /// The program's name without folder or ".exe", e.g. "vlc" or "chrome".
    pub app: String,
    pub sections: IndexMap<String, SectionSettings>,
}

/// The name a profile is matched by: "C:\Apps\VLC.exe" and "vlc" both give "vlc".
pub fn app_name(exe_or_path: &str) -> String {
    let trimmed = exe_or_path.trim().trim_matches('"');
    let file = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    let stem = match file.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("exe") => stem,
        _ => file,
    };
    stem.to_lowercase()
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
            apps: Vec::new(),
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

    /// Whether this section needs the touchpad to click. Edges: slide while pressing, or slide in
    /// from the laptop body with no click. Corners: press until it clicks, or just tap.
    pub fn needs_click(&self, s: Section) -> bool {
        self.section(s).needs_click.unwrap_or(if s.is_corner() { true } else { self.activation == Activation::Press })
    }

    /// `Some(true)` / `Some(false)` when every section (in every app profile too) needs a click / is
    /// used by touch, `None` when they differ.
    pub fn all_need_click(&self) -> Option<bool> {
        let mut values = Section::ALL.iter().map(|&s| self.needs_click(s)).chain(
            self.apps.iter().flat_map(|p| {
                Section::ALL
                    .iter()
                    .filter(|s| p.sections.contains_key(s.key()))
                    .map(|&s| self.for_app(Some(&p.app)).needs_click(s))
                    .collect::<Vec<_>>()
            }),
        );
        let first = values.next()?;
        values.all(|v| v == first).then_some(first)
    }

    /// Makes every section, in every app profile too, need a click or work by touch.
    pub fn set_all_need_click(&mut self, click: bool) {
        self.activation = if click { Activation::Press } else { Activation::SlideIn };
        // Edges follow `activation`; corners need a click unless told otherwise.
        let value = |s: Section| if s.is_corner() && !click { Some(false) } else { None };
        for s in Section::ALL {
            self.section_mut(s).needs_click = value(s);
        }
        for p in &mut self.apps {
            for s in Section::ALL {
                if let Some(v) = p.sections.get_mut(s.key()) {
                    v.needs_click = value(s);
                }
            }
        }
    }

    pub fn profile(&self, app: &str) -> Option<&AppProfile> {
        let name = app_name(app);
        self.apps.iter().find(|p| app_name(&p.app) == name)
    }

    pub fn profile_mut(&mut self, app: &str) -> Option<&mut AppProfile> {
        let name = app_name(app);
        self.apps.iter_mut().find(|p| app_name(&p.app) == name)
    }

    /// The settings in effect while `app` is in front: its profile's sections over the ones for all
    /// apps. `None`, or an app without a profile, gives the settings for all apps.
    pub fn for_app(&self, app: Option<&str>) -> Settings {
        let mut out = Settings { apps: Vec::new(), ..self.clone() };
        if let Some(profile) = app.and_then(|a| self.profile(a)) {
            for (key, v) in &profile.sections {
                out.sections.insert(key.clone(), v.clone());
            }
        }
        out
    }

    /// Whether `app`'s profile has its own settings for this section.
    pub fn overrides(&self, app: &str, s: Section) -> bool {
        self.profile(app).is_some_and(|p| p.sections.contains_key(s.key()))
    }

    /// The section to edit: the profile's own copy when `app` has one, else the one for all apps.
    pub fn section_for_edit(&mut self, s: Section, app: Option<&str>) -> &mut SectionSettings {
        let name = app.map(app_name);
        let Some(i) = name.and_then(|n| self.apps.iter().position(|p| app_name(&p.app) == n)) else {
            return self.section_mut(s);
        };
        let global = self.section(s).clone();
        self.apps[i].sections.entry(s.key().to_string()).or_insert(global)
    }

    /// Gives `app` its own copy of a section (`true`), or makes it follow all apps again (`false`).
    pub fn set_override(&mut self, app: &str, s: Section, own: bool) {
        let global = self.section(s).clone();
        if let Some(p) = self.profile_mut(app) {
            if own {
                p.sections.entry(s.key().to_string()).or_insert(global);
            } else {
                p.sections.shift_remove(s.key());
            }
        }
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
        for p in &mut self.apps {
            for v in p.sections.values_mut() {
                v.clicks.truncate(3);
            }
        }
        // a profile without a name could never match an app
        self.apps.retain(|p| !app_name(&p.app).is_empty());
    }
}

// ---------------------------------------------------------------- storage

/// Windows: `%APPDATA%\Margin`. Elsewhere: `$XDG_CONFIG_HOME/margin` (default `~/.config/margin`).
pub fn directory() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Margin")
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("margin")
    }
}

pub fn file_path() -> PathBuf {
    directory().join("settings.json")
}

pub fn exists() -> bool {
    file_path().exists()
}

pub fn load() -> Settings {
    match read() {
        Ok(Some(settings)) => settings,
        Ok(None) => {
            let mut settings = Settings::default();
            settings.normalize();
            settings
        }
        Err(e) => {
            crate::log::error(format!("{e}; using the defaults until it is fixed"));
            let mut settings = Settings::default();
            settings.normalize();
            settings
        }
    }
}

/// Reads settings.json: `Ok(None)` when there is no file yet. When the file can't be parsed, a copy is
/// kept next to it as `settings.json.broken`, so a later save can't lose what the user wrote.
pub fn read() -> Result<Option<Settings>, String> {
    read_from(&file_path())
}

fn read_from(path: &std::path::Path) -> Result<Option<Settings>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("could not read {}: {e}", path.display())),
    };
    match parse(&text) {
        Ok(mut settings) => {
            settings.normalize();
            Ok(Some(settings))
        }
        Err(e) => {
            let copy = broken_copy_path(path);
            let kept = match std::fs::write(&copy, &text) {
                Ok(()) => format!(" (a copy is kept as {})", copy.display()),
                Err(_) => String::new(),
            };
            Err(format!("{} has an error: {e}{kept}", path.display()))
        }
    }
}

fn broken_copy_path(path: &std::path::Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".broken");
    path.with_file_name(name)
}

/// When settings.json last changed, in milliseconds since 1970; 0 if it doesn't exist.
pub fn modified_ms() -> u64 {
    std::fs::metadata(file_path())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

/// Saves the settings, reporting any error to the caller.
pub fn try_save(settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(directory())?;
    let path = file_path();
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)
}

/// Saves the settings; a failure is logged, not fatal.
pub fn save(settings: &Settings) {
    if let Err(e) = try_save(settings) {
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
            "sections": { "top": { "slide": "zoom", }, "right": { "slide": "scroll" } },
        }"#;
        let s = parse(text).unwrap();
        assert_eq!(s.speed, Speed::Slow);
        assert_eq!(s.section(Section::Top).slide, SlideAction::Zoom);
        assert_eq!(s.section(Section::Right).slide, SlideAction::Scroll);
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

    #[test]
    fn needs_click_defaults_follow_activation_for_edges_and_click_for_corners() {
        let mut st = Settings::default(); // slideIn
        assert!(!st.needs_click(Section::Left));
        assert!(st.needs_click(Section::TopLeft));
        st.activation = Activation::Press;
        assert!(st.needs_click(Section::Left));
        st.section_mut(Section::TopLeft).needs_click = Some(false);
        assert!(!st.needs_click(Section::TopLeft));
        let text = serde_json::to_string(&st).unwrap();
        assert_eq!(text.matches("needsClick").count(), 1, "only the corner that was set");
        assert!(!parse(&text).unwrap().needs_click(Section::TopLeft));
    }

    #[test]
    fn set_all_need_click_reaches_every_section_and_profile() {
        let mut st = Settings::default();
        assert_eq!(st.all_need_click(), None, "default: edges by touch, corners by click");
        st.apps.push(AppProfile { app: "vlc".into(), ..Default::default() });
        st.set_override("vlc", Section::TopLeft, true);
        st.set_all_need_click(false);
        assert_eq!(st.all_need_click(), Some(false));
        assert!(!st.for_app(Some("vlc")).needs_click(Section::TopLeft));
        st.section_for_edit(Section::Top, None).needs_click = Some(true);
        assert_eq!(st.all_need_click(), None);
        st.set_all_need_click(true);
        assert_eq!(st.all_need_click(), Some(true));
        assert_eq!(st.activation, Activation::Press);
        assert!(st.sections.values().all(|v| v.needs_click.is_none()), "nothing left to write");
    }

    #[test]
    fn long_press_is_read_written_and_left_out_when_unset() {
        let mut st = Settings::default();
        assert!(!serde_json::to_string(&st).unwrap().contains("longPress"), "unset: not written");
        st.section_mut(Section::BottomLeft).long_press = ClickSpec { action: ClickAction::Url, value: Some("example.com".into()) };
        let text = serde_json::to_string_pretty(&st).unwrap();
        assert!(text.contains("\"longPress\""));
        let back = parse(&text).unwrap();
        assert_eq!(back.section(Section::BottomLeft).long_press.action, ClickAction::Url);
        assert_eq!(back.section(Section::BottomLeft).long_press.value.as_deref(), Some("example.com"));
    }

    #[test]
    fn corner_slots_map_clicks_and_long_press() {
        let mut v = SectionSettings::default();
        v.corner_slot_mut(2).action = ClickAction::Mute; // grows the click list
        v.corner_slot_mut(LONG_PRESS_SLOT).action = ClickAction::Lock;
        assert_eq!(v.clicks.len(), 3);
        assert_eq!(v.corner_slot(2).action, ClickAction::Mute);
        assert_eq!(v.corner_slot(LONG_PRESS_SLOT).action, ClickAction::Lock);
        let set: Vec<_> = v.corner_actions().map(|c| c.action).collect();
        assert_eq!(set, vec![ClickAction::Mute, ClickAction::Lock]);
    }

    #[test]
    fn app_names_ignore_folder_extension_and_case() {
        assert_eq!(app_name(r"C:\Program Files\VideoLAN\VLC\VLC.EXE"), "vlc");
        assert_eq!(app_name("\"C:/Apps/chrome.exe\""), "chrome");
        assert_eq!(app_name("Notepad"), "notepad");
        assert_eq!(app_name("my.app.exe"), "my.app");
    }

    fn with_vlc_profile() -> Settings {
        let mut st = Settings::default();
        st.apps.push(AppProfile { app: "VLC".into(), sections: IndexMap::new() });
        st.set_override("vlc", Section::Bottom, true);
        st.section_for_edit(Section::Bottom, Some("vlc")).slide = SlideAction::Seek;
        st
    }

    #[test]
    fn a_profile_replaces_only_its_own_sections() {
        let st = with_vlc_profile();
        let vlc = st.for_app(Some("vlc.exe"));
        assert_eq!(vlc.section(Section::Bottom).slide, SlideAction::Seek);
        assert_eq!(vlc.section(Section::Left).slide, SlideAction::Volume, "not in the profile: follows all apps");
        assert!(vlc.apps.is_empty(), "the result is flat");
        assert_eq!(st.for_app(Some("chrome")).section(Section::Bottom).slide, SlideAction::Desktops);
        assert_eq!(st.for_app(None).section(Section::Bottom).slide, SlideAction::Desktops);
        assert_eq!(st.section(Section::Bottom).slide, SlideAction::Desktops, "editing the profile left all apps alone");
    }

    #[test]
    fn editing_without_a_profile_changes_all_apps() {
        let mut st = with_vlc_profile();
        st.section_for_edit(Section::Left, Some("chrome")).slide = SlideAction::Zoom; // no chrome profile
        assert_eq!(st.section(Section::Left).slide, SlideAction::Zoom);
        st.section_for_edit(Section::Left, None).slide = SlideAction::Tracks;
        assert_eq!(st.section(Section::Left).slide, SlideAction::Tracks);
    }

    #[test]
    fn an_override_starts_as_a_copy_and_can_be_dropped() {
        let mut st = with_vlc_profile();
        assert!(!st.overrides("vlc", Section::TopLeft));
        st.set_override("vlc", Section::TopLeft, true);
        assert!(st.overrides("vlc", Section::TopLeft));
        assert_eq!(st.profile("vlc").unwrap().sections["topLeft"], *st.section(Section::TopLeft));
        st.set_override("vlc", Section::Bottom, false);
        assert_eq!(st.for_app(Some("vlc")).section(Section::Bottom).slide, SlideAction::Desktops);
    }

    #[test]
    fn profiles_round_trip_and_are_left_out_when_there_are_none() {
        assert!(!serde_json::to_string(&Settings::default()).unwrap().contains("\"apps\""));
        let st = with_vlc_profile();
        let back = parse(&serde_json::to_string_pretty(&st).unwrap()).unwrap();
        assert_eq!(back, st);
        let mut nameless = st.clone();
        nameless.apps.push(AppProfile { app: "  ".into(), sections: IndexMap::new() });
        nameless.normalize();
        assert_eq!(nameless.apps.len(), 1, "a profile without a name is dropped");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("margin-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = temp_dir("missing");
        assert!(matches!(read_from(&dir.join("settings.json")), Ok(None)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_file_is_reported_and_kept() {
        let dir = temp_dir("broken");
        let path = dir.join("settings.json");
        let text = "{ \"enabled\": true, \"speed\": \"medium\" \"oops\" }";
        std::fs::write(&path, text).unwrap();
        let err = read_from(&path).unwrap_err();
        assert!(err.contains("has an error"), "{err}");
        assert_eq!(std::fs::read_to_string(dir.join("settings.json.broken")).unwrap(), text);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "the file itself is left alone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_good_file_is_read_and_normalized() {
        let dir = temp_dir("good");
        let path = dir.join("settings.json");
        std::fs::write(&path, CSHARP_FILE).unwrap();
        let s = read_from(&path).unwrap().unwrap();
        assert_eq!(s.activation, Activation::SlideIn);
        assert!(!dir.join("settings.json.broken").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

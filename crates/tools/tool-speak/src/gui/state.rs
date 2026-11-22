//! What the speak window holds, and how its inputs change it.

use super::services::Services;
use crate::config::{DEFAULT_REACTIONS, SpeakConfig};
use common_audio::Clip;
use common_gui_rune::Ds;
use common_gui_rune::parts::Tone;
use rune::TextEdit;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What Compose says while no speech server is saved.
pub const FIRST_RUN_STATUS: &str = "Set the speech server in Settings first.";

/// The window's two areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// The text, the reaction buttons, Generate, and Play.
    Compose,
    /// The settings form and Save.
    Settings,
}

impl Area {
    /// Both areas, in the order the switch shows them.
    pub const ALL: [Self; 2] = [Self::Compose, Self::Settings];
    /// The switch's labels, in [`Area::ALL`]'s order.
    pub const LABELS: [&'static str; 2] = ["Compose", "Settings"];

    /// The area's place in [`Area::ALL`].
    pub fn index(self) -> usize {
        match self {
            Self::Compose => 0,
            Self::Settings => 1,
        }
    }

    /// The area's name in a log line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compose => "compose",
            Self::Settings => "settings",
        }
    }
}

/// The audio Compose holds for the text.
#[derive(Debug, Clone)]
pub enum ComposeAudio {
    /// Nothing was generated yet.
    Empty,
    /// A request is on its way; its answer has not arrived.
    Generating,
    /// The last generated audio. `play_issue` is why the last attempt to play did not work,
    /// whichever clip that attempt played.
    Ready {
        clip: Clip,
        play_issue: Option<String>,
    },
    /// The last request failed with this text.
    Failed(String),
}

/// How the last Save went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveStatus {
    /// Nothing to report.
    Idle,
    /// A save is on its way; `edited` is whether a field changed since it started.
    Saving { edited: bool },
    /// The form on screen is what the file holds.
    Saved,
    /// The save failed with this text.
    Failed(String),
}

/// The request method the Settings form offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Post,
    Put,
}

impl Method {
    /// The picker's options.
    pub const LABELS: [&'static str; 2] = ["POST", "PUT"];

    /// The method as the settings file and the picker spell it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Post => Self::LABELS[0],
            Self::Put => Self::LABELS[1],
        }
    }

    /// The method `label` spells, when it is one of [`Method::LABELS`].
    pub fn from_label(label: &str) -> Option<Self> {
        [Self::Post, Self::Put]
            .into_iter()
            .find(|method| method.label() == label)
    }
}

/// A field of the Settings form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Url,
    Method,
    Model,
    Voice,
    Reactions,
}

/// What is wrong with each Settings field that can be wrong, as of the last Save.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldIssues {
    pub url: Option<String>,
    pub model: Option<String>,
    pub voice: Option<String>,
    pub reactions: Option<String>,
}

impl FieldIssues {
    /// Whether no field has an issue.
    pub fn is_empty(&self) -> bool {
        self.first().is_none()
    }

    /// The first field with an issue, in the form's order.
    pub fn first(&self) -> Option<Field> {
        [
            (Field::Url, &self.url),
            (Field::Model, &self.model),
            (Field::Voice, &self.voice),
            (Field::Reactions, &self.reactions),
        ]
        .into_iter()
        .find_map(|(field, issue)| issue.as_ref().map(|_| field))
    }

    /// Drops `field`'s issue; true when it had one.
    fn clear(&mut self, field: Field) -> bool {
        match field {
            Field::Url => self.url.take().is_some(),
            Field::Model => self.model.take().is_some(),
            Field::Voice => self.voice.take().is_some(),
            Field::Reactions => self.reactions.take().is_some(),
            Field::Method => false,
        }
    }
}

/// The Settings form as typed, saved or not.
pub struct SettingsForm {
    pub url: TextEdit,
    pub method: Method,
    /// Whether the method picker's list is open.
    pub method_open: bool,
    pub model: TextEdit,
    pub voice: TextEdit,
    /// The reaction tags, one per line.
    pub reactions: TextEdit,
}

impl SettingsForm {
    /// The form showing `config`. A method the file spells some other way shows as `POST`,
    /// which the next Save writes.
    fn from_config(config: &SpeakConfig) -> Self {
        Self {
            url: TextEdit::new(config.url.as_str()),
            method: Method::from_label(&config.request_method).unwrap_or(Method::Post),
            method_open: false,
            model: TextEdit::new(config.model().unwrap_or_default()),
            voice: TextEdit::new(config.voice().unwrap_or_default()),
            reactions: TextEdit::new(config.supported_vocal_reactions.join("\n")),
        }
    }
}

/// Something that happened: a press, an edit, or a service's answer.
#[derive(Debug, Clone)]
pub enum Input {
    /// The area switch, or its shortcut.
    AreaPicked(Area),
    /// Generate, or `mod+enter` on Compose.
    GeneratePressed,
    /// The answer to a generate request: the audio, or why there is none.
    Generated(Result<Clip, String>),
    /// Play.
    PlayPressed,
    /// A clip could not be played.
    PlayFailed(String),
    /// Save.
    SavePressed,
    /// The user changed a Settings field.
    FieldEdited(Field),
    /// The answer to a save request: the settings now in the file, or why they are not.
    ConfigSaved(Result<SpeakConfig, String>),
}

/// Work an input asks the services for.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Ask `config`'s server to say `text`.
    Generate { config: SpeakConfig, text: String },
    /// Play the clip, stopping one that still sounds.
    Play(Clip),
    /// Write `config` to `path`.
    Save { config: SpeakConfig, path: PathBuf },
}

/// What applying an input did.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Whether anything the view reads changed.
    pub changed: bool,
    pub effects: Vec<Effect>,
}

impl Outcome {
    fn unchanged() -> Self {
        Self::default()
    }

    fn changed() -> Self {
        Self {
            changed: true,
            effects: Vec::new(),
        }
    }

    fn with(effect: Effect) -> Self {
        Self {
            changed: true,
            effects: vec![effect],
        }
    }
}

/// The speak window's state.
pub struct SpeakState {
    pub(super) ds: Ds,
    pub(super) system_dark: Option<bool>,
    area: Area,
    pub(super) text: TextEdit,
    audio: ComposeAudio,
    pub(super) form: SettingsForm,
    issues: FieldIssues,
    save: SaveStatus,
    /// The settings the file holds: `None` until a file was loaded or saved. Never the unsaved
    /// form.
    config: Option<SpeakConfig>,
    config_path: PathBuf,
    pub(super) services: Option<Rc<dyn Services>>,
}

impl SpeakState {
    /// The window over the settings loaded from `config_path` (`None` on a first run, which
    /// opens on Settings with the defaults and an empty URL).
    pub fn new(
        config: Option<SpeakConfig>,
        config_path: PathBuf,
        services: Option<Rc<dyn Services>>,
    ) -> Self {
        let (area, form) = match &config {
            Some(config) => (Area::Compose, SettingsForm::from_config(config)),
            None => (
                Area::Settings,
                SettingsForm::from_config(&SpeakConfig::default()),
            ),
        };
        Self {
            ds: Ds::new(),
            system_dark: None,
            area,
            text: TextEdit::new(""),
            audio: ComposeAudio::Empty,
            form,
            issues: FieldIssues::default(),
            save: SaveStatus::Idle,
            config,
            config_path,
            services,
        }
    }

    /// A first-run window with no services and no settings path.
    pub fn detached() -> Self {
        Self::new(None, PathBuf::new(), None)
    }

    pub fn area(&self) -> Area {
        self.area
    }

    pub fn audio(&self) -> &ComposeAudio {
        &self.audio
    }

    pub fn save_status(&self) -> &SaveStatus {
        &self.save
    }

    pub fn issues(&self) -> &FieldIssues {
        &self.issues
    }

    /// The saved settings, when there are any.
    pub fn config(&self) -> Option<&SpeakConfig> {
        self.config.as_ref()
    }

    /// Where Save writes.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// The reaction tags Compose offers: the saved settings' list, or the defaults while
    /// nothing is saved.
    pub fn reactions(&self) -> Vec<String> {
        match &self.config {
            Some(config) => config.supported_vocal_reactions.clone(),
            None => DEFAULT_REACTIONS.map(str::to_owned).to_vec(),
        }
    }

    /// Whether Generate can be pressed: a speech server is saved.
    pub fn can_generate(&self) -> bool {
        self.config.is_some()
    }

    /// Whether a generate request is on its way.
    pub fn is_generating(&self) -> bool {
        match self.audio {
            ComposeAudio::Generating => true,
            ComposeAudio::Empty | ComposeAudio::Ready { .. } | ComposeAudio::Failed(_) => false,
        }
    }

    /// Whether there is audio to play.
    pub fn can_play(&self) -> bool {
        match self.audio {
            ComposeAudio::Ready { .. } => true,
            ComposeAudio::Empty | ComposeAudio::Generating | ComposeAudio::Failed(_) => false,
        }
    }

    /// Whether a save is on its way.
    pub fn is_saving(&self) -> bool {
        match self.save {
            SaveStatus::Saving { .. } => true,
            SaveStatus::Idle | SaveStatus::Saved | SaveStatus::Failed(_) => false,
        }
    }

    /// What Compose's status line says, when it says anything. With no server saved and
    /// nothing generated it is [`FIRST_RUN_STATUS`].
    pub fn compose_status(&self) -> Option<(Tone, String)> {
        match &self.audio {
            ComposeAudio::Empty => match self.config {
                Some(_) => None,
                None => Some((Tone::Muted, FIRST_RUN_STATUS.to_owned())),
            },
            ComposeAudio::Generating => Some((Tone::Muted, "Generating…".to_owned())),
            ComposeAudio::Ready {
                play_issue: None, ..
            } => Some((Tone::Muted, "Ready to play".to_owned())),
            ComposeAudio::Ready {
                play_issue: Some(why),
                ..
            } => Some((Tone::Danger, format!("Couldn't play: {why}"))),
            ComposeAudio::Failed(why) => Some((Tone::Danger, format!("Couldn't generate: {why}"))),
        }
    }

    /// What the Settings footer's status line says, when it says anything.
    pub fn save_words(&self) -> Option<(Tone, String)> {
        match &self.save {
            SaveStatus::Idle | SaveStatus::Saving { .. } => None,
            SaveStatus::Saved => Some((Tone::Muted, "Saved".to_owned())),
            SaveStatus::Failed(why) => Some((Tone::Danger, format!("Couldn't save: {why}"))),
        }
    }

    /// Types `tag` into the text at the caret, replacing a selection, with one space before
    /// it when the caret follows a word. The caret ends after the tag.
    pub fn insert_reaction(&mut self, tag: &str) {
        let (start, _) = self.text.range();
        let insert = reaction_insert(&self.text.text[..start], tag);
        self.text.insert(&insert);
    }

    /// Applies `input` and says what changed and what work it asks for. Each machine takes one
    /// request at a time, so an answer always belongs to the request on its way; an answer
    /// that arrives with none on its way changes nothing.
    pub fn apply(&mut self, input: Input) -> Outcome {
        match input {
            Input::AreaPicked(area) => self.pick_area(area),
            Input::GeneratePressed => self.generate(),
            Input::Generated(result) => self.generated(result),
            Input::PlayPressed => self.play(),
            Input::PlayFailed(why) => self.play_failed(why),
            Input::SavePressed => self.save(),
            Input::FieldEdited(field) => self.field_edited(field),
            Input::ConfigSaved(result) => self.config_saved(result),
        }
    }

    fn pick_area(&mut self, area: Area) -> Outcome {
        if self.area == area {
            return Outcome::unchanged();
        }
        self.area = area;
        self.form.method_open = false;
        Outcome::changed()
    }

    fn generate(&mut self) -> Outcome {
        match self.audio {
            ComposeAudio::Generating => Outcome::unchanged(),
            ComposeAudio::Empty | ComposeAudio::Ready { .. } | ComposeAudio::Failed(_) => {
                let text = self.text.text.trim();
                let Some(config) = &self.config else {
                    return Outcome::unchanged();
                };
                if text.is_empty() {
                    return Outcome::unchanged();
                }
                let effect = Effect::Generate {
                    config: config.clone(),
                    text: text.to_owned(),
                };
                self.audio = ComposeAudio::Generating;
                Outcome::with(effect)
            }
        }
    }

    fn generated(&mut self, result: Result<Clip, String>) -> Outcome {
        match self.audio {
            ComposeAudio::Generating => {
                self.audio = match result {
                    Ok(clip) => ComposeAudio::Ready {
                        clip,
                        play_issue: None,
                    },
                    Err(why) => ComposeAudio::Failed(why),
                };
                Outcome::changed()
            }
            ComposeAudio::Empty | ComposeAudio::Ready { .. } | ComposeAudio::Failed(_) => {
                Outcome::unchanged()
            }
        }
    }

    fn play(&mut self) -> Outcome {
        match &mut self.audio {
            ComposeAudio::Ready { clip, play_issue } => {
                let changed = play_issue.take().is_some();
                Outcome {
                    changed,
                    effects: vec![Effect::Play(clip.clone())],
                }
            }
            ComposeAudio::Empty | ComposeAudio::Generating | ComposeAudio::Failed(_) => {
                Outcome::unchanged()
            }
        }
    }

    fn play_failed(&mut self, why: String) -> Outcome {
        match &mut self.audio {
            ComposeAudio::Ready { play_issue, .. } => {
                *play_issue = Some(why);
                Outcome::changed()
            }
            ComposeAudio::Empty | ComposeAudio::Generating | ComposeAudio::Failed(_) => {
                Outcome::unchanged()
            }
        }
    }

    fn save(&mut self) -> Outcome {
        match self.save {
            SaveStatus::Saving { .. } => Outcome::unchanged(),
            SaveStatus::Idle | SaveStatus::Saved | SaveStatus::Failed(_) => {
                match self.form_config() {
                    Ok(config) => {
                        self.issues = FieldIssues::default();
                        self.save = SaveStatus::Saving { edited: false };
                        Outcome::with(Effect::Save {
                            config,
                            path: self.config_path.clone(),
                        })
                    }
                    Err(issues) => {
                        self.issues = issues;
                        self.save = SaveStatus::Idle;
                        Outcome::changed()
                    }
                }
            }
        }
    }

    fn field_edited(&mut self, field: Field) -> Outcome {
        let cleared = self.issues.clear(field);
        let moved = match self.save {
            SaveStatus::Saving { edited: false } => {
                self.save = SaveStatus::Saving { edited: true };
                true
            }
            SaveStatus::Saved | SaveStatus::Failed(_) => {
                self.save = SaveStatus::Idle;
                true
            }
            SaveStatus::Idle | SaveStatus::Saving { edited: true } => false,
        };
        Outcome {
            changed: cleared || moved,
            effects: Vec::new(),
        }
    }

    fn config_saved(&mut self, result: Result<SpeakConfig, String>) -> Outcome {
        match self.save {
            SaveStatus::Saving { edited } => {
                self.save = match result {
                    Ok(config) => {
                        self.config = Some(config);
                        if edited {
                            SaveStatus::Idle
                        } else {
                            SaveStatus::Saved
                        }
                    }
                    Err(why) => SaveStatus::Failed(why),
                };
                Outcome::changed()
            }
            SaveStatus::Idle | SaveStatus::Saved | SaveStatus::Failed(_) => Outcome::unchanged(),
        }
    }

    /// The settings the form spells: the saved ones (so payload fields the form does not show
    /// are kept) with the form's values over them, or each field's issue.
    fn form_config(&self) -> Result<SpeakConfig, FieldIssues> {
        let mut config = self.config.clone().unwrap_or_default();
        config.url = self.form.url.text.trim().to_owned();
        config.request_method = self.form.method.label().to_owned();
        config.set_model(self.form.model.text.trim());
        config.set_voice(self.form.voice.text.trim());
        let why = |error: anyhow::Error| format!("{error:#}");
        let mut issues = FieldIssues {
            url: config.endpoint().err().map(why),
            model: config.check_model().err().map(why),
            voice: config.check_voice().err().map(why),
            reactions: None,
        };
        match parse_reactions(&self.form.reactions.text) {
            Ok(reactions) => config.supported_vocal_reactions = reactions,
            Err(issue) => issues.reactions = Some(issue),
        }
        if issues.is_empty() {
            Ok(config)
        } else {
            Err(issues)
        }
    }
}

/// What a reaction button types for `tag`, given the text before the caret: the tag, after one
/// space when that text ends in something other than whitespace.
pub fn reaction_insert(text_before_caret: &str, tag: &str) -> String {
    match text_before_caret.chars().next_back() {
        Some(before) if !before.is_whitespace() => format!(" {tag}"),
        Some(_) | None => tag.to_owned(),
    }
}

/// The reaction tags the Settings box lists, one per line; blank lines are dropped.
///
/// # Errors
/// The issue naming the first line that is not words in one pair of square brackets.
pub fn parse_reactions(lines: &str) -> Result<Vec<String>, String> {
    let mut reactions = Vec::new();
    for (index, line) in lines.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !is_reaction_tag(line) {
            return Err(format!(
                "Line {} is not a tag in square brackets, like [laugh]",
                index + 1
            ));
        }
        reactions.push(line.to_owned());
    }
    Ok(reactions)
}

fn is_reaction_tag(line: &str) -> bool {
    let Some(inner) = line
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return false;
    };
    !inner.trim().is_empty() && !inner.contains(['[', ']'])
}

/// The words of `tag` without its brackets, for a reaction button's accessible name.
pub fn reaction_word(tag: &str) -> &str {
    tag.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim()
}

#[cfg(test)]
mod tests;

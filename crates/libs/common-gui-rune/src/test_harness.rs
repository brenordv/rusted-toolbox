//! Headless test helpers over Rune's `Runtime`, for any app state. It is the one place tests
//! reach into Rune's runtime API, so a Rune bump edits this module.
//!
//! Test support only (`cfg(test)` or the dev-only `test-harness` feature): helpers panic on a
//! failed expectation, which fails the test that called them.

use rune::clipboard::MemoryClipboard;
use rune::{Cx, El, ElementInfo, Env, FileDialogKind, Key, KeyEvent, Mods, Runtime, Tx};
use serde_json::Value;
use std::path::Path;

/// Frames a settle runs: half a second of virtual time, enough for every transition the motion
/// roles give. A spinner loops forever, so "until nothing moves" would never end.
pub const SETTLE_FRAMES: usize = 30;
/// One frame of virtual time.
pub const FRAME: f64 = 1.0 / 60.0;

/// A runtime over `state` drawn by `view` at `size`, in the dark appearance at scale 1, settled.
pub fn runtime_at<S: 'static>(
    state: S,
    view: fn(&S, &mut Cx<S>) -> El,
    size: (f32, f32),
) -> Runtime<S> {
    let mut rt = Runtime::new(state, view, size, Env::new(true, 1.0));
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// [`runtime_at`] with `on_key` as the window's key handler, so window shortcuts reach the
/// state.
pub fn runtime_with_keys<S: 'static>(
    state: S,
    view: fn(&S, &mut Cx<S>) -> El,
    on_key: fn(&mut S, &KeyEvent, &mut Tx) -> bool,
    size: (f32, f32),
) -> Runtime<S> {
    let mut rt = runtime_at(state, view, size);
    rt.on_key = Some(on_key);
    rt
}

/// Runs [`SETTLE_FRAMES`] frames.
pub fn settle<S: 'static>(rt: &mut Runtime<S>) {
    for _ in 0..SETTLE_FRAMES {
        rt.frame(FRAME);
    }
}

/// What a background thread posts: `f` applied on the UI thread as
/// `AppHandle::update_if` would apply it, then a settle.
pub fn deliver<S: 'static>(rt: &mut Runtime<S>, f: impl FnOnce(&mut S, &mut Tx) -> bool + 'static) {
    rt.update_if(f);
    settle(rt);
}

/// Whether focus is on the element keyed `key` or inside it (the key may sit on a wrapper
/// around the control that takes focus).
///
/// # Panics
/// When nothing is keyed `key`.
pub fn focus_is_inside<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    let target = element(rt, key).path;
    let inside = format!("{target}/");
    rt.elements().into_iter().any(|element| {
        element.focused && (element.path == target || element.path.starts_with(&inside))
    })
}

/// Presses the field keyed `key`, checks that focus landed in it, selects its text, and pastes
/// `text` over it: typing through the event loop, as a person would.
///
/// # Panics
/// When nothing is keyed `key`, or the press leaves focus outside it.
pub fn type_into<S: 'static>(rt: &mut Runtime<S>, key: &str, text: &str) {
    press(rt, key);
    assert!(
        focus_is_inside(rt, key),
        "{key} did not take focus: {:?}",
        focused_key(rt)
    );
    command(rt, Key::Command('a'));
    rt.paste(text);
    settle(rt);
}

/// Where the element keyed `key` ends, top to bottom.
///
/// # Panics
/// When nothing is keyed `key`.
pub fn bottom<S: 'static>(rt: &Runtime<S>, key: &str) -> f32 {
    let [_, y, _, height] = rect(rt, key);
    y + height
}

/// Whether the accessibility node keyed `key` reads as disabled.
///
/// # Panics
/// When no accessibility node is keyed `key`.
pub fn is_disabled<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    let tree = rt.accessibility_json();
    let node = a11y_node(&tree, key).unwrap_or_else(|| panic!("no accessibility node keyed {key}"));
    node["states"]
        .as_array()
        .is_some_and(|states| states.iter().any(|state| state == "disabled"))
}

/// Gives `rt` an in-memory clipboard and returns it, so a test reads what the app copied.
pub fn use_memory_clipboard<S: 'static>(rt: &mut Runtime<S>) -> MemoryClipboard {
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    clipboard
}

/// The text `clipboard` holds, when it holds text.
pub fn clipboard_text(clipboard: &MemoryClipboard) -> Option<String> {
    clipboard.text().map(|text| text.to_string())
}

/// Answers the `kind` dialog that shows with `path`, or cancels it with `None`, then settles.
///
/// # Panics
/// When no file dialog shows, or the one that shows is another kind.
pub fn answer_dialog<S: 'static>(rt: &mut Runtime<S>, kind: FileDialogKind, path: Option<&Path>) {
    let showing = rt.file_dialog().map(|(showing, _)| showing);
    assert_eq!(showing, Some(kind), "the dialog showing is not {kind:?}");
    if let Err(error) = rt.resolve_file_dialog(path.map(Path::to_path_buf)) {
        panic!("answering the dialog failed: {error}");
    }
    settle(rt);
}

/// The element keyed `key` that is not on its way out.
///
/// # Panics
/// When nothing is keyed `key`.
pub fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .unwrap_or_else(|| panic!("no element keyed {key}"))
}

/// `[x, y, width, height]` of the element keyed `key`.
pub fn rect<S: 'static>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    element(rt, key).rect
}

/// Presses `key` with no modifiers, then settles.
pub fn press_key<S: 'static>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    settle(rt);
}

/// The platform's command modifier: Cmd on macOS, Ctrl elsewhere.
pub fn command_mods() -> Mods {
    Mods {
        cmd: true,
        ctrl: !cfg!(target_os = "macos"),
        logo: cfg!(target_os = "macos"),
        ..Mods::default()
    }
}

/// Presses `key` with the command modifier (`mod+<key>`), then settles.
pub fn command<S: 'static>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: command_mods(),
    });
    settle(rt);
}

/// A pointer press and release at the centre of the element keyed `key`, then a settle.
///
/// # Panics
/// When nothing is keyed `key`.
pub fn press<S: 'static>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Whether an element keyed `key` is mounted.
pub fn has_key<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.key.as_deref() == Some(key))
}

/// Whether an element's text holds `needle`.
pub fn has_text<S: 'static>(rt: &Runtime<S>, needle: &str) -> bool {
    rt.elements().iter().any(|element| {
        element
            .text
            .as_deref()
            .is_some_and(|text| text.contains(needle))
    })
}

/// Whether an element holding `needle` shows on screen.
pub fn text_visible<S: 'static>(rt: &Runtime<S>, needle: &str) -> bool {
    rt.elements().iter().any(|element| {
        element
            .text
            .as_deref()
            .is_some_and(|text| text.contains(needle))
            && element.visible[3] > 0.0
    })
}

/// The key of the focused element, when it has one.
pub fn focused_key<S: 'static>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .filter(|element| element.focused)
        .find_map(|element| element.key)
}

/// The label of the first element keyed `key`.
pub fn label_of<S: 'static>(rt: &Runtime<S>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .and_then(|element| element.label)
}

/// Every node of an accessibility tree from [`Runtime::accessibility_json`], depth first.
pub fn a11y_nodes(tree: &Value) -> Vec<&Value> {
    let mut nodes = vec![tree];
    let mut at = 0;
    while at < nodes.len() {
        if let Some(children) = nodes[at]["children"].as_array() {
            nodes.extend(children);
        }
        at += 1;
    }
    nodes
}

/// The accessibility node keyed `key`.
pub fn a11y_node<'a>(tree: &'a Value, key: &str) -> Option<&'a Value> {
    a11y_nodes(tree)
        .into_iter()
        .find(|node| node["key"].as_str() == Some(key))
}

#[cfg(test)]
mod tests;

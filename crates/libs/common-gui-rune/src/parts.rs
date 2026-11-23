//! Parts every Rune window shares: the styled pieces the kit lacks. Views compose these and
//! kit components. A tool's own parts live in that tool.

use crate::tokens::{
    HAIRLINE, RADIUS_CONTROL, SPACE_SM, SPACE_XS, TYPE_CAPTION, TYPE_LABEL, TYPE_MONO,
};
use common_utils::string_utils::{display_line, error_preview};
use rune::{
    El, KeyEvent, LiveRegion, ScrollSpec, TextEdit, TextStyle, Tx, col, el, text, text_input,
};
use rune_kit::{Kit, ScrollBar, TextInput};

/// A caption label over a selectable monospace value, for paths and ids.
pub fn detail_row(k: &Kit, label: &str, value: &str) -> El {
    col()
        .gap(SPACE_XS)
        .w_full()
        .child(text(label).font(TYPE_CAPTION).color(k.c.text_muted))
        .child(
            text(value)
                .font(TYPE_MONO)
                .color(k.c.text_base)
                .wrap_anywhere()
                .selectable(true),
        )
}

/// A form label over a control that draws none of its own, such as a mode picker.
pub fn field_label(k: &Kit, label: &str) -> El {
    text(label).font(TYPE_LABEL).color(k.c.text_base)
}

/// A muted caption line, for form notes.
pub fn caption(k: &Kit, line: &str) -> El {
    text(line).font(TYPE_CAPTION).color(k.c.text_muted).wrap()
}

/// A field's issue under it, in the danger color.
pub fn issue(k: &Kit, line: &str) -> El {
    text(line).font(TYPE_CAPTION).color(k.c.danger.fg).wrap()
}

/// A caption asking the user to look before going on, in the warning color.
pub fn warning(k: &Kit, line: &str) -> El {
    text(line).font(TYPE_CAPTION).color(k.c.warning.fg).wrap()
}

/// A full-width hairline, as under a header or over a footer.
pub fn hairline(k: &Kit) -> El {
    el().w_full().h(HAIRLINE).fill(k.c.border_weak)
}

/// What a [`status_line`] says with its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Progress and outcomes that need no action.
    Muted,
    /// A failure.
    Danger,
    /// Something to look at that stopped nothing.
    Warning,
}

/// How a [`status_line`] fits its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLines {
    /// Wraps onto as many lines as the text needs.
    Wrap,
    /// One line, cut with an ellipsis at the width.
    One,
}

/// A caption keyed `key` that reports a status: `message` on one line in the tone's color, with
/// control and bidirectional characters as spaces, cut at 140 characters with "…". Assistive
/// technology gets it as a `status` in a polite live region named by what shows; when the
/// message was cut, the whole of it (cleaned the same way) is its description.
pub fn status_line(k: &Kit, key: &str, tone: Tone, lines: StatusLines, message: &str) -> El {
    let preview = error_preview(message);
    let color = match tone {
        Tone::Muted => k.c.text_muted,
        Tone::Danger => k.c.danger.fg,
        Tone::Warning => k.c.warning.fg,
    };
    let line = text(preview.shown.as_str()).font(TYPE_CAPTION).color(color);
    let line = match lines {
        StatusLines::Wrap => line.wrap(),
        StatusLines::One => line.truncate(),
    };
    let mut status = col()
        .key(key)
        .role("status")
        .label(preview.shown.as_str())
        .live_region(LiveRegion::Polite)
        .w_full()
        .min_w(0.0)
        .child(line.a11y_hidden());
    if preview.cut {
        status = status.description(display_line(message.chars()));
    }
    status
}

/// What a [`line_field`] shows beyond its text.
#[derive(Clone, Copy, Debug)]
pub struct LineFieldSpec<'a> {
    /// The field's key; focus moves there. Its issue line is keyed `{key}-issue`.
    pub key: &'a str,
    /// The visible label and the accessible name.
    pub label: &'a str,
    /// The faint text an empty field shows; empty for none.
    pub placeholder: &'a str,
    /// What is wrong with the field's value; it shows under the field as a danger status.
    pub issue: Option<&'a str>,
}

/// A label over a single-line field. `on_key` sees every key before the field does, so a
/// window's shortcuts work while the field has focus; `on_change` runs after every edit the
/// field makes. The field carries the name, so the visible label stays out of the
/// accessibility tree.
///
/// The kit's `TextField` has no key hook: a focused one takes a shortcut such as `mod+enter`
/// for itself and passes nothing on.
pub fn line_field<S: 'static>(
    k: &Kit,
    spec: LineFieldSpec<'_>,
    edit: &TextEdit,
    access: fn(&mut S) -> &mut TextEdit,
    on_key: impl Fn(&mut S, &KeyEvent, &mut Tx) -> bool + 'static,
    on_change: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let mut input = TextInput::new(edit)
        .label(spec.label)
        .width(None)
        .invalid(spec.issue.is_some())
        .on_edit(access)
        .on_key(on_key)
        .on_change(on_change);
    if !spec.placeholder.is_empty() {
        input = input.placeholder(spec.placeholder);
    }
    let mut field = col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, spec.label).a11y_hidden())
        .child(input.build(k).key(spec.key));
    if let Some(issue) = spec.issue {
        field = field.child(status_line(
            k,
            &format!("{}-issue", spec.key),
            Tone::Danger,
            StatusLines::Wrap,
            issue,
        ));
    }
    field
}

/// How tall a text editor is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorHeight {
    /// At least `min_lines` lines of the editor's type role tall, growing to the space it is
    /// given.
    Fill { min_lines: f32 },
    /// Exactly `lines` lines of the editor's type role tall, never growing.
    Fixed { lines: f32 },
}

/// What a text editor shows beyond its text.
#[derive(Clone, Copy, Debug)]
pub struct EditorSpec<'a> {
    /// The shell's key; focus moves there.
    pub key: &'a str,
    /// The accessible name.
    pub label: &'a str,
    /// The faint text an empty editor shows.
    pub placeholder: &'a str,
    /// The accessible description, when there is one.
    pub description: Option<&'a str>,
    pub invalid: bool,
    pub height: EditorHeight,
    /// The type role of the text; a line of it is the unit of `height`.
    pub font: TextStyle,
}

/// A multi-line editor for short lines such as `key=value`: the look of [`text_editor`], a
/// fixed `spec.height` that never grows, long lines broken anywhere (values such as ids and
/// URLs have no spaces), and a viewport that scrolls only once there is text, so an empty box
/// never takes the wheel from a scrolling pane around it. `on_key` sees every key first, as in
/// [`text_editor`].
pub fn lines_editor<S: 'static>(
    k: &Kit,
    spec: EditorSpec<'_>,
    edit: &TextEdit,
    access: fn(&mut S) -> &mut TextEdit,
    on_key: impl Fn(&mut S, &KeyEvent, &mut Tx) -> bool + 'static,
) -> El {
    text_editor(k, spec, edit, access, on_key, |_: &mut S, _: &mut Tx| {})
}

/// A multi-line editor in `spec.font` with the kit field's look, scrolling inside its box. A
/// `Fill` editor grows to the space it is given; a `Fixed` one breaks long lines anywhere and
/// scrolls only once it holds text. The box never springs as lines are typed. There is no
/// `on_submit`, and `on_key` sees every key first: a plain Return reaches it untyped, so
/// `on_key` inserts the newline. `on_change` runs after every edit the field itself makes
/// (typing, paste, undo), not after `on_key`'s.
///
/// The kit's `TextArea` sets its own UI type with no way to change it and springs its height
/// on every new line.
pub fn text_editor<S: 'static>(
    k: &Kit,
    spec: EditorSpec<'_>,
    edit: &TextEdit,
    access: fn(&mut S) -> &mut TextEdit,
    on_key: impl Fn(&mut S, &KeyEvent, &mut Tx) -> bool + 'static,
    on_change: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let c = &k.c;
    let mut field = text_input(edit, spec.font)
        .key("input")
        .w_full()
        .color(c.text_base)
        .placeholder(spec.placeholder, c.text_faint)
        .caret(c.text_base, k.selection())
        .label(spec.label)
        .invalid(spec.invalid)
        .on_edit(access)
        .on_input_key(on_key)
        .on_input_change(on_change);
    if let Some(description) = spec.description {
        field = field.description(description);
    }
    let line = spec.font.line_px();
    // (viewport, whether it scrolls, whether the shell grows to the space it is given)
    let (viewport, scrolls, fills) = match spec.height {
        EditorHeight::Fill { min_lines } => (
            col().grow(1.0).min_h(min_lines * line).child(field),
            true,
            true,
        ),
        EditorHeight::Fixed { lines } => (
            col().h(lines * line).child(field.wrap_anywhere()),
            !edit.text.is_empty(),
            false,
        ),
    };
    let mut viewport = viewport.key("scroll").w_full();
    if scrolls {
        viewport = viewport
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .scroll_bar(k);
    }
    let mut shell = col()
        .key(spec.key)
        .role("textbox")
        .w_full()
        .padding(SPACE_SM)
        .radius(RADIUS_CONTROL)
        .fill(c.bg_base)
        .child(viewport);
    if fills {
        shell = shell.grow(1.0).min_h(0.0);
    }
    if spec.invalid {
        shell = shell.outline(HAIRLINE, -HAIRLINE, c.danger.fg);
    } else if edit.focused {
        shell = shell.outline(HAIRLINE, -HAIRLINE, c.border_focus);
    } else {
        for shadow in k.shadow_layers(k.elevation.input()) {
            shell = shell.box_shadow(shadow);
        }
    }
    shell
}

#[cfg(test)]
mod tests;

use super::builders::state;
use super::worst_cases::long_text;
use super::{Harness, a11y_node, a11y_nodes, element, focused_key, lists, rect};
use crate::design::Ds;
use crate::design::toast::{toast_of_control, toast_size};
use crate::state::{AppState, Input, ToastKind, error_preview};
use crate::views::details::DONE_KEY;
use chrono::Utc;
use rune::accesskit::Action;
use rune::{Key, KeyEvent, Mods};
use rune_kit::toast_stack::GAP;

const SIZE: (f32, f32) = crate::design::tokens::WINDOW_SIZE;

/// Toasts of `kinds` with these texts, oldest first, in a window of `size`.
pub(super) fn toasted(size: (f32, f32), toasts: &[(ToastKind, String)]) -> Harness {
    let mut s = state();
    for (kind, text) in toasts {
        s.toast(*kind, text.clone());
    }
    Harness::new(s, size)
}

/// Three toasts, oldest first: a cut danger, an info, a cut danger.
pub(super) fn three(size: (f32, f32)) -> Harness {
    toasted(
        size,
        &[
            (ToastKind::Danger, format!("first: {}", long_text())),
            (ToastKind::Info, "Exported 3 messages".to_owned()),
            (ToastKind::Danger, format!("third: {}", long_text())),
        ],
    )
}

fn ids(s: &AppState) -> Vec<u64> {
    s.toasts().entries().iter().map(|entry| entry.id).collect()
}

/// Whether the control keyed `key` takes a press and focus.
fn live(h: &Harness, key: &str) -> bool {
    let control = element(&h.rt, key);
    !control.inert && control.focusable
}

fn alt_t(h: &mut Harness) {
    h.rt.key(KeyEvent {
        key: Key::Text("t".into()),
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
    });
    h.settle();
}

fn focus(h: &mut Harness, key: &str) {
    assert!(
        h.rt.accessibility_action(key, Action::Focus, None),
        "{key} takes no focus"
    );
    h.settle();
}

/// Space on the focused control: a keyboard press.
///
/// # Panics
/// When the press opened no details modal.
fn press_focused(h: &mut Harness) {
    h.key(Key::Text(" ".into()));
    assert!(h.state().details().is_some(), "the press opened nothing");
}

fn danger(h: &mut Harness, text: &str) {
    h.deliver(Input::ConfigSaved(Err(format!("{text}: {}", long_text()))));
}

#[test]
fn a_folded_stack_offers_only_the_front_cards_controls_and_reads_every_card() {
    let h = three(SIZE);
    let [first, second, third] = ids(h.state())[..] else {
        panic!("three toasts");
    };

    let tree = h.a11y();
    let region = a11y_node(&tree, "toast-stack").unwrap();
    let statuses: Vec<_> = a11y_nodes(region)
        .into_iter()
        .filter(|node| node["role"] == "status")
        .collect();

    assert!(live(&h, &format!("toast-{third}-details")));
    assert!(live(&h, &format!("toast-{third}-close")));
    assert!(element(&h.rt, &format!("toast-{first}-details")).inert);
    assert!(element(&h.rt, &format!("toast-{second}-close")).inert);
    assert!(a11y_node(&tree, &format!("toast-{first}-details")).is_none());
    assert_eq!(region["name"], "Notifications");
    let keys: Vec<_> = statuses.iter().map(|node| node["key"].clone()).collect();
    assert_eq!(
        keys,
        [
            format!("toast-{third}"),
            format!("toast-{second}"),
            format!("toast-{first}")
        ]
    );
    assert!(
        statuses
            .iter()
            .all(|node| !lists(node, "states", "disabled"))
    );
    assert_eq!(statuses[1]["name"], "Done");
    assert_eq!(statuses[1]["description"], "Exported 3 messages");
    assert_eq!(statuses[0]["name"], "Something failed");
}

#[test]
fn the_pointer_fans_the_stack_out_and_each_card_sits_where_the_stack_places_it() {
    let mut h = three(SIZE);
    let [first, second, third] = ids(h.state())[..] else {
        panic!("three toasts");
    };

    h.hover("toast-stack");
    let stack = rect(&h.rt, "toast-stack");
    let bottoms: Vec<f32> = [third, second, first]
        .iter()
        .map(|id| {
            let [_, y, _, height] = rect(&h.rt, &format!("toast-{id}"));
            y + height
        })
        .collect();
    let heights: Vec<f32> = [third, second]
        .iter()
        .map(|id| rect(&h.rt, &format!("toast-{id}"))[3])
        .collect();

    assert!(h.state().toasts().stack().expanded());
    assert!(live(&h, &format!("toast-{first}-details")));
    assert!(live(&h, &format!("toast-{second}-close")));
    let foot = stack[1] + stack[3];
    assert!((bottoms[0] - foot).abs() < 0.5, "{bottoms:?} {stack:?}");
    assert!((bottoms[1] - (foot - heights[0] - GAP)).abs() < 0.5);
    assert!((bottoms[2] - (foot - heights[0] - heights[1] - 2.0 * GAP)).abs() < 0.5);
}

#[test]
fn a_fanned_out_cards_details_return_focus_to_it_and_leaving_folds_onto_the_front() {
    let mut h = three(SIZE);
    let [first, _, third] = ids(h.state())[..] else {
        panic!("three toasts");
    };
    h.hover("toast-stack");

    h.press(&format!("toast-{first}-details"));
    let text = h.state().details().map(|details| details.text.clone());
    h.key(Key::Escape);
    let returned = focused_key(&h.rt);
    let still_fanned = h.state().toasts().stack().expanded();
    focus(&mut h, &format!("toast-{first}-close"));
    h.pointer_away();

    assert_eq!(text, Some(format!("first: {}", long_text())));
    assert_eq!(returned, Some(format!("toast-{first}-details")));
    assert!(still_fanned);
    assert!(!h.state().toasts().stack().expanded());
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{third}-details")));
}

#[test]
fn alt_t_fans_out_onto_the_front_card_tab_walks_up_and_escape_folds() {
    let mut h = three(SIZE);
    let [first, second, third] = ids(h.state())[..] else {
        panic!("three toasts");
    };

    alt_t(&mut h);
    let on_front = focused_key(&h.rt);
    h.key(Key::Tab);
    let next = focused_key(&h.rt);
    h.key(Key::Tab);
    h.key(Key::Tab);
    let oldest = focused_key(&h.rt);
    h.key(Key::Escape);

    assert_eq!(on_front, Some(format!("toast-{third}-details")));
    assert_eq!(next, Some(format!("toast-{third}-close")));
    assert_eq!(oldest, Some(format!("toast-{first}-details")));
    assert!(!h.state().toasts().stack().expanded());
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{third}-details")));
    assert!(has_close(&h, second));
}

fn has_close(h: &Harness, id: u64) -> bool {
    h.has_key(&format!("toast-{id}-close"))
}

#[test]
fn escape_folds_the_stack_before_it_closes_the_connect_sheet() {
    let mut s = state();
    s.toast(ToastKind::Danger, format!("a: {}", long_text()));
    s.toast(ToastKind::Danger, format!("b: {}", long_text()));
    s.sheet = Some(crate::state::SheetPurpose::Connect);
    let mut h = Harness::new(s, SIZE);
    alt_t(&mut h);

    h.key(Key::Escape);
    let folded = !h.state().toasts().stack().expanded() && h.state().sheet.is_some();
    h.key(Key::Escape);

    assert!(folded);
    assert!(h.state().sheet.is_none());
}

#[test]
fn escape_with_nothing_focused_folds_a_stack_the_pointer_fanned_out() {
    let mut h = three(SIZE);
    h.hover("toast-stack");
    let fanned = h.state().toasts().stack().expanded();

    h.key(Key::Escape);

    assert!(fanned);
    assert!(!h.state().toasts().stack().expanded());
    assert_eq!(focused_key(&h.rt), None);
}

#[test]
fn toasts_arriving_and_closing_leave_focus_outside_the_stack_alone() {
    let mut h = toasted(SIZE, &[(ToastKind::Info, "done".to_owned())]);
    focus(&mut h, "pill");

    danger(&mut h, "newer");
    for _ in 0..6 {
        h.tick();
    }

    assert_eq!(focused_key(&h.rt).as_deref(), Some("pill"));
}

#[test]
fn closing_the_focused_front_card_moves_focus_to_the_next_one() {
    let mut h = three(SIZE);
    let [_, second, third] = ids(h.state())[..] else {
        panic!("three toasts");
    };
    alt_t(&mut h);
    focus(&mut h, &format!("toast-{third}-close"));

    h.key(Key::Text(" ".into()));

    assert!(!h.has_key(&format!("toast-{third}")));
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{second}-close")));
}

#[test]
fn closing_the_last_card_leaves_nothing_focused() {
    let mut h = toasted(SIZE, &[(ToastKind::Info, "done".to_owned())]);
    let id = ids(h.state())[0];
    focus(&mut h, &format!("toast-{id}-close"));

    h.key(Key::Text(" ".into()));

    assert!(h.state().toasts().entries().is_empty());
    assert_eq!(focused_key(&h.rt), None);
}

#[test]
fn a_new_toast_folding_the_focused_front_card_moves_focus_to_the_new_front() {
    let mut h = three(SIZE);
    let third = ids(h.state())[2];
    focus(&mut h, &format!("toast-{third}-close"));

    danger(&mut h, "newer");
    let front = h.state().toasts().front().unwrap().id;

    assert_eq!(focused_key(&h.rt), Some(format!("toast-{front}-details")));
}

#[test]
fn toasts_that_arrive_under_a_cards_details_wait_and_focus_finds_the_front_card() {
    let mut h = three(SIZE);
    let first = ids(h.state())[0];
    h.hover("toast-stack");
    h.press(&format!("toast-{first}-details"));

    for text in ["x", "y", "z"] {
        danger(&mut h, text);
    }
    let held = (ids(h.state()), h.has_key(&format!("toast-{first}")));
    h.press(DONE_KEY);
    let front = h.state().toasts().front().unwrap().id;

    assert_eq!(held.0.len(), 3);
    assert!(held.1);
    assert!(!h.has_key(&format!("toast-{first}")));
    assert_eq!(h.state().toasts().entries().len(), 3);
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{front}-details")));
}

#[test]
fn a_toast_held_under_the_front_cards_details_takes_focus_when_they_close() {
    let mut h = three(SIZE);
    let third = ids(h.state())[2];
    focus(&mut h, &format!("toast-{third}-details"));
    press_focused(&mut h);

    danger(&mut h, "newer");
    h.key(Key::Escape);
    let front = h.state().toasts().front().unwrap().id;

    assert_ne!(front, third);
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{front}-details")));
}

#[test]
fn a_fanned_out_opener_still_shown_after_the_release_gets_focus_back() {
    let mut h = toasted(
        SIZE,
        &[
            (ToastKind::Danger, format!("a: {}", long_text())),
            (ToastKind::Danger, format!("b: {}", long_text())),
        ],
    );
    let older = ids(h.state())[0];
    h.hover("toast-stack");
    h.press(&format!("toast-{older}-details"));

    danger(&mut h, "newer");
    h.key(Key::Escape);

    assert_eq!(h.state().toasts().entries().len(), 3);
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{older}-details")));
}

#[test]
fn closing_a_cards_details_with_nothing_held_returns_focus_to_it() {
    let mut h = three(SIZE);
    let third = ids(h.state())[2];
    focus(&mut h, &format!("toast-{third}-details"));
    press_focused(&mut h);

    h.key(Key::Escape);

    assert_eq!(focused_key(&h.rt), Some(format!("toast-{third}-details")));
}

/// Focus is on a control of a card that shows its controls, or on no toast control at all.
pub(super) fn focus_on_a_shown_card(h: &Harness) -> bool {
    let Some(id) = focused_key(&h.rt).as_deref().and_then(toast_of_control) else {
        return true;
    };
    let stack = h.state().toasts();
    stack.entries().iter().any(|entry| entry.id == id)
        && (stack.stack().expanded() || stack.front().map(|entry| entry.id) == Some(id))
}

#[test]
fn a_window_closed_under_a_cards_details_never_leaves_focus_on_a_folded_card() {
    let mut h = three(SIZE);
    let third = ids(h.state())[2];
    focus(&mut h, &format!("toast-{third}-details"));
    press_focused(&mut h);
    danger(&mut h, "newer");

    h.deliver(Input::WindowClosed(Utc::now()));
    let front = h.state().toasts().front().unwrap().id;

    assert!(h.state().details().is_none());
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{front}-details")));
}

#[test]
fn the_windows_close_handler_moves_focus_off_a_folded_opener() {
    let mut h = three(SIZE);
    let third = ids(h.state())[2];
    focus(&mut h, &format!("toast-{third}-details"));
    press_focused(&mut h);
    danger(&mut h, "newer");

    h.rt.update_if(|s, tx| {
        crate::app::on_close(s, tx);
        true
    });
    h.settle();
    let front = h.state().toasts().front().unwrap().id;

    assert!(h.state().details().is_none());
    assert_eq!(focused_key(&h.rt), Some(format!("toast-{front}-details")));
}

#[test]
fn every_toast_control_names_its_card_in_its_description() {
    let h = three(SIZE);
    let third = ids(h.state())[2];
    let preview = error_preview(&format!("third: {}", long_text())).shown;

    let tree = h.a11y();

    for key in [
        format!("toast-{third}-details"),
        format!("toast-{third}-close"),
    ] {
        let node = a11y_node(&tree, &key).unwrap();
        assert_eq!(node["description"], preview.as_str(), "{key}");
    }
}

#[test]
fn an_info_toast_waits_while_the_stack_is_fanned_out() {
    let mut h = toasted(SIZE, &[(ToastKind::Info, "done".to_owned())]);
    h.hover("toast-stack");

    for _ in 0..10 {
        h.tick();
    }
    let survived = h.state().toasts().entries().len();
    h.pointer_away();
    for _ in 0..5 {
        h.tick();
    }
    let before_sixth = h.state().toasts().entries().len();
    h.tick();

    assert_eq!((survived, before_sixth), (1, 1));
    assert!(h.state().toasts().entries().is_empty());
}

#[rstest::rstest]
#[case::one_line("Exported 3 messages".to_owned())]
#[case::two_lines("the hub refused the batch because the namespace is throttling".to_owned())]
#[case::three_lines("the hub refused the batch because the namespace is throttling ".repeat(2))]
#[case::four_lines(long_text())]
#[case::wide_glyphs("W".repeat(140))]
#[case::long_token(format!("sb://{}", "a".repeat(135)))]
fn a_cards_laid_out_height_is_the_measured_one(#[case] text: String) {
    let h = toasted(SIZE, &[(ToastKind::Danger, text.clone())]);
    let id = ids(h.state())[0];
    let expected = toast_size(Ds::new().kit(true), &error_preview(&text).shown).0;

    let card = rect(&h.rt, &format!("toast-{id}"));
    let card_path = h.path_of(&format!("toast-{id}"));
    let description =
        h.rt.elements()
            .into_iter()
            .find(|element| {
                element.key.as_deref() == Some("description")
                    && element.path.starts_with(&format!("{card_path}/"))
            })
            .unwrap();

    assert!(
        (card[3] - expected).abs() < 0.5,
        "{} vs {expected}",
        card[3]
    );
    let [_, y, _, height] = description.rect;
    assert!(y + height <= card[1] + card[3] + 0.5);
}

#[test]
fn the_same_failure_twice_rings_one_card_with_a_status_of_its_own() {
    let mut h = toasted(SIZE, &[]);
    danger(&mut h, "again");

    danger(&mut h, "again");
    let entry = h.state().toasts().front().unwrap().clone();
    let pulse = entry.options.pulse.unwrap();
    let card_path = h.path_of(&format!("toast-{}", entry.id));
    let ring_path = h.path_of(&format!("pulse-{pulse}"));
    let tree = h.a11y();
    let ring = a11y_node(&tree, &format!("pulse-{pulse}")).unwrap();

    assert_eq!(h.state().toasts().entries().len(), 1);
    assert_eq!(ring_path, format!("{card_path}/pulse-{pulse}"));
    assert_eq!(ring["role"], "status");
    assert_eq!(ring["name"], "Again: Something failed");
}

#[test]
fn no_repeat_mounts_no_ring() {
    let h = three(SIZE);

    let rings =
        h.rt.elements()
            .into_iter()
            .filter(|element| {
                element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("pulse-"))
            })
            .count();

    assert_eq!(rings, 0);
}

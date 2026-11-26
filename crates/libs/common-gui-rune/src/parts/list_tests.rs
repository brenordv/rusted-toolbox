use super::*;
use crate::Ds;
use crate::test_harness::{a11y_node, a11y_nodes, rect, runtime_at};
use rstest::rstest;
use rune::{Cx, Runtime};

const LIST_KEY: &str = "items";
const LABEL: &str = "Items";
const EMPTY: &str = "Nothing here yet.";
const ROOMY: (f32, f32) = (480.0, 400.0);
/// Shorter than three rows, so a `Fill` box shows its minimum.
const SHORT: (f32, f32) = (480.0, 80.0);

#[derive(Debug, Clone, Copy)]
enum Shown {
    EmptyBuilt,
    EmptyVirtual,
    ThreeBuilt,
}

struct Form {
    ds: Ds,
    shown: Shown,
    height: ListHeight,
}

fn view(form: &Form, _: &mut Cx<Form>) -> El {
    let k = form.ds.kit(true);
    let rows = match form.shown {
        Shown::EmptyBuilt => ListRows::Built(Vec::new()),
        Shown::EmptyVirtual => ListRows::Virtual {
            count: 0,
            column: col(),
        },
        Shown::ThreeBuilt => ListRows::Built(
            (0..3)
                .map(|index| {
                    row()
                        .key(format!("row-{index}"))
                        .role("group")
                        .label(format!("Row {index}"))
                        .h(CONTROL_HEIGHT)
                        .child(text(format!("row {index}")).font(TYPE_BODY))
                })
                .collect(),
        ),
    };
    col().w_full().h_full().padding(SPACE_LG).child(list_box(
        k,
        (LIST_KEY, LABEL),
        rows,
        EMPTY,
        form.height,
    ))
}

fn runtime(shown: Shown, height: ListHeight, size: (f32, f32)) -> Runtime<Form> {
    let form = Form {
        ds: Ds::new(),
        shown,
        height,
    };
    runtime_at(form, view, size)
}

fn empty_text_rect(rt: &Runtime<Form>) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|element| element.text.as_deref() == Some(EMPTY))
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("no empty text"))
}

#[rstest]
#[case::no_built_rows(Shown::EmptyBuilt)]
#[case::no_virtual_rows(Shown::EmptyVirtual)]
fn an_empty_list_centres_its_text_and_keeps_its_rows_height(#[case] shown: Shown) {
    let rt = runtime(shown, ListHeight::Rows(3.0), ROOMY);

    let [box_x, box_y, box_w, box_h] = rect(&rt, LIST_KEY);
    let [text_x, text_y, text_w, text_h] = empty_text_rect(&rt);

    assert!(
        (box_h - list_height(3.0)).abs() < 0.5,
        "the box is {box_h} tall"
    );
    assert!(
        ((text_y + text_h / 2.0) - (box_y + box_h / 2.0)).abs() < 1.0,
        "the text at {text_y}+{text_h}, the box at {box_y}+{box_h}"
    );
    assert!(
        ((text_x + text_w / 2.0) - (box_x + box_w / 2.0)).abs() < 1.0,
        "the text at {text_x}+{text_w}, the box at {box_x}+{box_w}"
    );
}

#[rstest]
#[case::no_built_rows(Shown::EmptyBuilt)]
#[case::no_virtual_rows(Shown::EmptyVirtual)]
fn an_empty_fill_list_keeps_its_minimum_rows_in_a_short_column(#[case] shown: Shown) {
    let rt = runtime(shown, ListHeight::Fill { min_rows: 3.0 }, SHORT);

    let box_h = rect(&rt, LIST_KEY)[3];

    assert!(
        box_h >= list_height(3.0) - 0.5,
        "the box is {box_h} tall, three rows are {}",
        list_height(3.0)
    );
}

#[test]
fn a_fill_list_takes_the_height_its_column_gives_it() {
    let rt = runtime(Shown::EmptyBuilt, ListHeight::Fill { min_rows: 3.0 }, ROOMY);

    let box_h = rect(&rt, LIST_KEY)[3];

    assert!(
        (box_h - (ROOMY.1 - 2.0 * SPACE_LG)).abs() < 0.5,
        "the box is {box_h} tall"
    );
}

#[test]
fn built_rows_sit_in_a_group_named_by_the_label() {
    let rt = runtime(Shown::ThreeBuilt, ListHeight::Rows(3.0), ROOMY);

    let tree = rt.accessibility_json();
    let group = a11y_node(&tree, LIST_KEY).unwrap_or_else(|| panic!("no list in {tree}"));
    let inside = a11y_nodes(group);

    assert_eq!(group["role"], "group");
    assert_eq!(group["name"], LABEL);
    for index in 0..3 {
        assert!(
            inside
                .iter()
                .any(|node| node["name"] == format!("Row {index}")),
            "row {index} is not in the list: {group}"
        );
    }
    assert!(
        !rt.elements()
            .iter()
            .any(|element| element.text.as_deref() == Some(EMPTY))
    );
}

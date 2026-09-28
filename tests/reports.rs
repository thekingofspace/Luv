use luv::packager::{remember, remembered};

#[test]
fn the_error_popup_groups_repeats_and_puts_the_most_common_first() {
    remember("first, once");
    for _ in 0..3 {
        remember("flood");
    }
    remember("second, once");
    for index in 0..70 {
        remember(&format!("distinct {index}"));
    }
    remember("flood");
    let lines = remembered();
    assert_eq!(lines[0], "4 times: flood");
    assert_eq!(lines[1], "first, once");
    assert_eq!(lines[2], "second, once");
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[5], "and 68 other errors");
}

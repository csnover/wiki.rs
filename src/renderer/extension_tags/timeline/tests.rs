use super::timeline_to_svg as render;
use axum::http::Uri;

#[track_caller]
fn run_test(input: &str) {
    let svg = render(input, &Uri::from_static("http://example.com")).unwrap();
    panic!("{svg}");
}

#[test]
fn basic() {
    run_test(include_str!("./tests/basic.txt"));
}

#[test]
fn eras() {
    run_test(include_str!("./tests/eras.txt"));
}

#[test]
fn font_sizes() {
    run_test(include_str!("./tests/font_sizes.txt"));
}

#[test]
fn fractional_major_scale() {
    run_test(include_str!("./tests/fractional_major_scale.txt"));
}

#[test]
fn history_of_computing() {
    run_test(include_str!("./tests/history_of_computing.txt"));
}

#[test]
fn mcdonnell_douglas_md_11() {
    run_test(include_str!("./tests/mcdonnell_douglas_md_11.txt"));
}

#[test]
fn tabs() {
    run_test(include_str!("./tests/tabs.txt"));
}

#[test]
fn vertical() {
    run_test(include_str!("./tests/vertical.txt"));
}

#[test]
fn wikimedia_growth() {
    run_test(include_str!("./tests/wikimedia_growth.txt"));
}

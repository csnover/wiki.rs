use super::timeline_to_svg as render;
use axum::http::Uri;

const BASE_DIR: &str = "./src/renderer/extension_tags/timeline/tests";

macro_rules! run_tests {
    ($($name:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run_test(
                stringify!($name),
                include_str!(concat!("./tests/", stringify!($name), ".txt"))
            );
        })*
    }
}

#[track_caller]
fn run_test(test_name: &str, input: &str) {
    use std::io::Write as _;

    let mut mint = goldenfile::Mint::new(format!("{BASE_DIR}/goldenfiles"));
    let mut file = mint.new_goldenfile(format!("{test_name}.svg")).unwrap();
    let result = render(input, &Uri::from_static("http://example.com")).unwrap();
    let _ = writeln!(file, "{result}");
}

run_tests! {
    basic,
    eras,
    font_sizes,
    fractional_major_scale,
    history_of_computing,
    mcdonnell_douglas_md_11,
    secretary_of_state_for_defence,
    tabs,
    vertical,
    wikimedia_growth,
}

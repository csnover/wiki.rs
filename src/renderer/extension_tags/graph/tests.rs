use super::spec_to_svg as render;
use crate::php::{DateTime, DateTimeZone};

const BASE_DIR: &str = "./src/renderer/extension_tags/graph/tests";

macro_rules! run_tests {
    ($($name:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run_test(
                stringify!($name),
                include_str!(concat!("./tests/", stringify!($name), ".json"))
            );
        })*
    }
}

#[track_caller]
fn run_test(test_name: &str, input: &str) {
    use std::io::Write as _;

    let now = DateTime::from_parts(
        2000,
        Some(1),
        Some(1),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(&DateTimeZone::UTC),
    )
    .unwrap();

    let mut mint = goldenfile::Mint::new(format!("{BASE_DIR}/goldenfiles"));
    let mut file = mint.new_goldenfile(format!("{test_name}.svg")).unwrap();
    let result = render(input, now).unwrap();
    let _ = writeln!(file, "{result}");
}

run_tests! {
    arc,
    area,
    bar,
    barley,
    driving,
    error,
    falkensee,
    // force,
    grouped_bar,
    image,
    // jobs,
    lifelines,
    map,
    parallel_coords,
    // playfair,
    population,
    // scatter_matrix,
    stacked_area,
    stacked_bar,
    // treemap,
    weather,
    wordcloud,
}

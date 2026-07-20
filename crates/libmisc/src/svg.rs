//! Common utility traits and functions for writing SVG strings.

/// The SVG namespace.
pub const NS_SVG: &str = "http://www.w3.org/2000/svg";

/// Shorthand for XML attribute names.
#[macro_export]
macro_rules! n {
    ($name:literal) => {
        #[expect(
            clippy::transmute_ptr_to_ptr,
            reason = "unwanted upstream lints leak in through the macro expansion"
        )]
        {
            ::rxml::xml_ncname!($name).into()
        }
    };
}

pub use n;

/// A trait for formatting floating point values with limited precision.
pub trait ValueDisplay {
    /// Formats the value as a floating point string with default precision.
    #[inline]
    #[must_use]
    fn v(self) -> String
    where
        Self: Sized,
    {
        self.v_precision(3)
    }

    /// Formats the value as a floating point string with *maximum* precision
    /// `precision`.
    #[must_use]
    fn v_precision(self, precision: u8) -> String;
}

impl ValueDisplay for f32 {
    fn v_precision(self, precision: u8) -> String {
        let precision = 10.0_f32.powi(precision.into());
        let v = (self * precision).round() / precision;
        if v == -0.0 {
            "0".to_owned()
        } else {
            format!("{v}")
        }
    }
}

impl ValueDisplay for f64 {
    fn v_precision(self, precision: u8) -> String {
        let precision = 10.0_f64.powi(precision.into());
        let v = (self * precision).round() / precision;
        if v == -0.0 {
            "0".to_owned()
        } else {
            format!("{v}")
        }
    }
}

//! Lua 5.1-compatible OS standard library.

use crate::{
    lua::{LanguageLibrary, prelude::*},
    php::{DateTime, DateTimeZone},
};
use gc_arena::Rootable;
use std::time::Instant;

/// Loads the OS library.
#[allow(clippy::too_many_lines)]
pub fn load_os(ctx: Context<'_>) {
    let os = Table::new(&ctx);
    let epoch = Instant::now();
    os.set_field(
        ctx,
        "clock",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            stack.replace(ctx, epoch.elapsed().as_secs_f64());
            Ok(CallbackReturn::Return)
        }),
    );
    os.set_field(
        ctx,
        "date",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (format, time) = stack.consume::<(VmString<'_>, Option<i64>)>(ctx)?;

            let (start, offset) = if format.first() == Some(&b'!') {
                (1, DateTimeZone::UTC)
            } else {
                (0, DateTimeZone::local()?)
            };

            let time = time
                .map_or_else(
                    || {
                        // TODO: This is silly and going in the opposite
                        // direction from what it should be
                        Ok(ctx.singleton::<Rootable![LanguageLibrary]>().date())
                    },
                    DateTime::from_unix_timestamp,
                )?
                .into_offset(offset)?;

            if format == "*t" || format == "!*t" {
                let table = Table::new(&ctx);
                table.set_field(ctx, "year", time.year());
                table.set_field(ctx, "month", u8::from(time.month()));
                table.set_field(ctx, "day", time.day());
                table.set_field(ctx, "yday", time.ordinal());
                table.set_field(ctx, "wday", time.weekday().number_from_sunday());
                table.set_field(ctx, "hour", time.hour());
                table.set_field(ctx, "min", time.minute());
                table.set_field(ctx, "sec", time.second());
                table.set_field(ctx, "isdst", false);
                stack.replace(ctx, table);
                return Ok(CallbackReturn::Return);
            }

            let format = format.as_bytes()[start..].iter().copied();
            let out = crate::common::format_date_strftime(time, format)?;

            stack.replace(ctx, ctx.intern(&out));
            Ok(CallbackReturn::Return)
        }),
    );
    os.set_field(
        ctx,
        "time",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let time = if let Some(options) = stack.consume::<Option<Table<'_>>>(ctx)? {
                if os_time::can_go_fast(ctx, options) {
                    os_time::time_fast(ctx, options)?
                } else {
                    return Ok(os_time::time_slow(ctx, options));
                }
            } else {
                // TODO: This is silly and going in the opposite
                // direction from what it should be
                ctx.singleton::<Rootable![LanguageLibrary]>()
                    .date()
                    .unix_timestamp()
            };
            stack.replace(ctx, time);
            Ok(CallbackReturn::Return)
        }),
    );

    ctx.set_global("os", os);
}

mod os_time {
    //! Support functions for `os.time`.

    use super::super::extras::index_helper;
    use piccolo::{CallbackReturn, Context, IntoValue as _, SequenceReturn, Table, async_sequence};

    /// The slow (metacall) version of `os.time`.
    pub(crate) fn time_slow<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> CallbackReturn<'gc> {
        let s = async_sequence(&ctx, |locals, mut seq| {
            let table = locals.stash(&ctx, options);
            let keys = [
                locals.stash(&ctx, "year".into_value(ctx)),
                locals.stash(&ctx, "month".into_value(ctx)),
                locals.stash(&ctx, "day".into_value(ctx)),
                locals.stash(&ctx, "hour".into_value(ctx)),
                locals.stash(&ctx, "min".into_value(ctx)),
                locals.stash(&ctx, "sec".into_value(ctx)),
                locals.stash(&ctx, "isdst".into_value(ctx)),
            ];
            async move {
                for (index, key) in keys.iter().enumerate() {
                    index_helper(&mut seq, &table, key, index).await?;
                }
                seq.try_enter(|ctx, _, _, mut stack| {
                    let (year, month, day, hour, min, sec, is_dst) =
                        stack.consume::<(
                            i64,
                            i64,
                            i64,
                            Option<i64>,
                            Option<i64>,
                            Option<i64>,
                            Option<bool>,
                        )>(ctx)?;
                    stack.replace(ctx, do_time(year, month, day, hour, min, sec, is_dst)?);
                    Ok(())
                })?;
                Ok(SequenceReturn::Return)
            }
        });
        CallbackReturn::Sequence(s)
    }

    /// Returns true if the fast path can be used.
    pub(crate) fn can_go_fast<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> bool {
        options.metatable().is_none_or(|_| {
            for key in ["year", "month", "day", "hour", "min", "sec", "isdst"] {
                if options.get::<_, i64>(ctx, key).is_err() {
                    return false;
                }
            }
            true
        })
    }

    /// Converts values from a Lua time object into a Unix timestamp.
    pub(crate) fn do_time<'gc>(
        year: i64,
        month: i64,
        day: i64,
        hour: Option<i64>,
        min: Option<i64>,
        sec: Option<i64>,
        // TODO: It is not immediately clear what this flag actually does in
        // PUC-Lua. Naïve attempts to mess with it just do not seem to cause any
        // change to outputs at all.
        _is_dst: Option<bool>,
    ) -> Result<i64, piccolo::Error<'gc>> {
        let date = time::Date::from_calendar_date(
            year.try_into()?,
            time::Month::try_from(u8::try_from(month)?)?,
            day.try_into()?,
        )?;
        let time = time::Time::from_hms(
            hour.unwrap_or(12).try_into()?,
            min.unwrap_or(0).try_into()?,
            sec.unwrap_or(0).try_into()?,
        )?;
        let offset = time::UtcOffset::current_local_offset()?;
        Ok(time::OffsetDateTime::new_in_offset(date, time, offset).unix_timestamp())
    }

    /// The fast (direct) version of `os.time`.
    pub(crate) fn time_fast<'gc>(
        ctx: Context<'gc>,
        options: Table<'gc>,
    ) -> Result<i64, piccolo::Error<'gc>> {
        let year = options.get::<_, i64>(ctx, "year")?;
        let month = options.get::<_, i64>(ctx, "month")?;
        let day = options.get::<_, i64>(ctx, "day")?;
        let hour = options.get::<_, Option<i64>>(ctx, "hour")?;
        let min = options.get::<_, Option<i64>>(ctx, "min")?;
        let sec = options.get::<_, Option<i64>>(ctx, "sec")?;
        let is_dst = options.get::<_, Option<bool>>(ctx, "isdst")?;
        do_time(year, month, day, hour, min, sec, is_dst)
    }
}

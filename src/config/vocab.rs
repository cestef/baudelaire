//! The config vocabulary: what each shape of key is called, what [`Kind`] it
//! reports, what reads it off a node and what reads it back off the struct.
//!
//! One arm per shape, and the only place those four facts are tied together.
//! [`Section`] tables are derived from it by `#[derive(Table)]`, which hands
//! each `#[key]` field here as `rule!(@row Self, "name", field, "doc", <shape>)`.
//!
//! [`Kind`]: crate::config::dispatch::Kind
//! [`Section`]: crate::config::dispatch::Section

/// One row of a [`Block`](crate::config::dispatch::Block) table, by the shape of
/// the key it holds.
///
/// `@set` and `@opt` are the two write halves every shape goes through, and
/// `custom` is the escape hatch for a key no shape describes.
macro_rules! rule {
    (@set $t:ty, $key:expr, $field:ident, $doc:literal, $kind:expr, $get:expr, $put:expr) => {
        (
            $key,
            $kind,
            $doc,
            |c: &$t| ($get)(&c.$field),
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                c.$field = ($put)(n, t)?;
                ::core::result::Result::Ok(())
            },
        )
    };

    (@opt $t:ty, $key:expr, $field:ident, $doc:literal, $kind:expr, $get:expr, $put:expr) => {
        (
            $key,
            $kind,
            $doc,
            |c: &$t| c.$field.as_ref().map_or($crate::config::Value::Unset, $get),
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                c.$field = ::core::option::Option::Some(($put)(n, t)?);
                ::core::result::Result::Ok(())
            },
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, flag) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Flag,
            |v: &bool| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::boolean(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, text) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Text,
            |v: &::std::string::String| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::string(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, path) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Path,
            |v: &::std::path::PathBuf| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| {
                $crate::config::node::NodeExt::string(n, t, 0).map(::std::path::PathBuf::from)
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, contained) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Path,
            |v: &::std::string::String| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::contained(n, t)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, asset) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Asset,
            |v: &::std::path::PathBuf| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::asset(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, url) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Url,
            |v: &::std::string::String| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::url(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, base) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Url,
            |v: &::std::string::String| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::base_url(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, template) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Template,
            |v: &::std::string::String| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::template(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, segment) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Text,
            |v: &::std::string::String| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::template(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, count) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Number,
            |v: &usize| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::count(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, int) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Number,
            |v: &i64| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::int(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal,
     bounded($ty:ty, $min:expr, $max:expr)) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Number,
            |v: &$ty| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| {
                $crate::config::value::ValueExt::bounded::<$ty>(
                    $crate::config::node::NodeExt::arg(n, t, 0)?,
                    t,
                    $crate::config::node::NodeExt::span(n),
                    $min,
                    $max,
                )
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, port) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Number,
            |v: &u16| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::port(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, size) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Size,
            |v: &$crate::ui::Bytes| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::size(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, time) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Time,
            |v: &::std::time::Duration| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::duration(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, level) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Level(
                <$crate::config::Severity as $crate::config::Named>::names
            ),
            |v: &$crate::config::Level| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::level(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, choice($ty:ty)) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Choice(<$ty as $crate::config::Named>::names),
            |v: &$ty| $crate::config::Value::named(*v),
            |n: &::kdl::KdlNode, t: &str| {
                $crate::config::value::ValueExt::one::<$ty>(
                    $crate::config::node::NodeExt::arg(n, t, 0)?,
                    t,
                    $crate::config::node::NodeExt::span(n),
                )
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, version) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Version,
            |v: &$crate::config::Version| (*v).into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::version(n, t, 0)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, paths) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Texts,
            |v: &::std::vec::Vec<::std::path::PathBuf>| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| {
                $crate::config::node::NodeExt::words(n, t).map(|words| {
                    words
                        .into_iter()
                        .map(::std::path::PathBuf::from)
                        .collect::<::std::vec::Vec<_>>()
                })
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, texts) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Texts,
            |v: &::std::vec::Vec<::std::string::String>| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::words(n, t)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, toggles) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Toggles,
            |v: &::std::vec::Vec<::std::string::String>| v.clone().into(),
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::features(n, t)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, choices($ty:ty)) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Choices(<$ty as $crate::config::Named>::names),
            |v: &::std::vec::Vec<$ty>| {
                v.iter().copied().map($crate::config::Value::named).collect()
            },
            |n: &::kdl::KdlNode, t: &str| $crate::config::node::NodeExt::mapped::<$ty>(n, t)
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal,
     toggled($ty:ty, $defaults:expr, $on:expr)) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Toggled(<$ty as $crate::config::Named>::names, $on),
            |v: &::std::vec::Vec<$ty>| {
                v.iter().copied().map($crate::config::Value::named).collect()
            },
            |n: &::kdl::KdlNode, t: &str| {
                $crate::config::node::NodeExt::toggled::<$ty>(n, t, &$defaults)
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal,
     numbers($ty:ty, $min:expr, $max:expr)) => {
        $crate::config::vocab::rule!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Numbers,
            |v: &::std::vec::Vec<$ty>| v.iter().copied().collect(),
            |n: &::kdl::KdlNode, t: &str| {
                $crate::config::node::NodeExt::bounds::<$ty>(n, t, $min, $max)
            }
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal,
     items($s:ty, $noun:literal, $item:expr)) => {
        (
            $key,
            $crate::config::dispatch::Kind::Items(
                <$s as $crate::config::dispatch::Section>::rows
            ),
            $doc,
            |c: &$t| {
                $crate::config::Value::each(
                    &c.$field,
                    <$s as $crate::config::dispatch::Section>::values,
                )
            },
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                c.$field = $crate::config::node::NodeExt::unique(n, t, $noun, $item)?;
                ::core::result::Result::Ok(())
            },
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal,
     lines($s:ty, $noun:literal, $item:expr)) => {
        (
            $key,
            $crate::config::dispatch::Kind::Lines(
                <$s as $crate::config::dispatch::Attributed>::rows
            ),
            $doc,
            |c: &$t| {
                $crate::config::Value::each(
                    &c.$field,
                    <$s as $crate::config::dispatch::Attributed>::values,
                )
            },
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                c.$field = $crate::config::node::NodeExt::unique(n, t, $noun, $item)?;
                ::core::result::Result::Ok(())
            },
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, shorthand($s:ty, $stands_for:literal)) => {
        (
            $key,
            $crate::config::dispatch::Kind::Block(
                <$s as $crate::config::dispatch::Section>::rows
            ),
            $doc,
            |c: &$t| <$s as $crate::config::dispatch::Section>::values(&c.$field),
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                <$s as $crate::config::dispatch::Section>::shorthand(
                    &mut c.$field,
                    n,
                    t,
                    $stands_for,
                )
            },
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, opt nested($s:ty)) => {
        (
            $key,
            $crate::config::dispatch::Kind::Block(
                <$s as $crate::config::dispatch::Section>::rows
            ),
            $doc,
            |c: &$t| {
                c.$field.as_ref().map_or(
                    $crate::config::Value::Unset,
                    <$s as $crate::config::dispatch::Section>::values,
                )
            },
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                <$s as $crate::config::dispatch::Section>::optional(&mut c.$field, n, t)
            },
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, nested($s:ty)) => {
        (
            $key,
            $crate::config::dispatch::Kind::Block(
                <$s as $crate::config::dispatch::Section>::rows
            ),
            $doc,
            |c: &$t| <$s as $crate::config::dispatch::Section>::values(&c.$field),
            |c: &mut $t, n: &::kdl::KdlNode, t: &str| {
                <$s as $crate::config::dispatch::Section>::fill(&mut c.$field, n, t)
            },
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal,
     custom($kind:expr, $get:expr, $put:expr $(,)?)) => {
        ($key, $kind, $doc, $get, $put)
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, opt $shape:ident $(($($arg:tt)*))?) => {
        $crate::config::vocab::rule!(@shape opt, $t, $key, $field, $doc, $shape $(($($arg)*))?)
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, $shape:ident $(($($arg:tt)*))?) => {
        $crate::config::vocab::rule!(@shape set, $t, $key, $field, $doc, $shape $(($($arg)*))?)
    };

    (@switch $t:ty, $field:ident) => {
        const SWITCH: ::core::option::Option<$crate::config::dispatch::Switch<Self>> =
            ::core::option::Option::Some($crate::config::dispatch::Switch {
                set: |c: &mut Self, on: bool| c.$field = on,
                on: |c: &Self| c.$field,
            });
    };
}

pub(crate) use rule;

/// One row of an [`Attrs`](crate::config::dispatch::Attrs) table, the
/// `key=value` counterpart of [`rule`]: same protocol, and a write half taking
/// the entry's value and span rather than the node.
macro_rules! attr {
    (@set $t:ty, $key:expr, $field:ident, $doc:literal, $kind:expr, $get:expr, $put:expr) => {
        (
            $key,
            $kind,
            $doc,
            |c: &$t| ($get)(&c.$field),
            |c: &mut $t, v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                c.$field = ($put)(v, t, s)?;
                ::core::result::Result::Ok(())
            },
        )
    };

    (@opt $t:ty, $key:expr, $field:ident, $doc:literal, $kind:expr, $get:expr, $put:expr) => {
        (
            $key,
            $kind,
            $doc,
            |c: &$t| c.$field.as_ref().map_or($crate::config::Value::Unset, $get),
            |c: &mut $t, v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                c.$field = ::core::option::Option::Some(($put)(v, t, s)?);
                ::core::result::Result::Ok(())
            },
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, flag) => {
        $crate::config::vocab::attr!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Flag,
            |v: &bool| (*v).into(),
            |v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                $crate::config::value::ValueExt::boolean(v, t, s)
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, text) => {
        $crate::config::vocab::attr!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Text,
            |v: &::std::string::String| v.clone().into(),
            |v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                $crate::config::value::ValueExt::as_str(v, t, s)
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, int) => {
        $crate::config::vocab::attr!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Number,
            |v: &i64| (*v).into(),
            |v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                $crate::config::value::ValueExt::integer(v, t, s)
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal,
     bounded($ty:ty, $min:expr, $max:expr)) => {
        $crate::config::vocab::attr!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Number,
            |v: &$ty| (*v).into(),
            |v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                $crate::config::value::ValueExt::bounded::<$ty>(v, t, s, $min, $max)
            }
        )
    };

    (@shape $how:ident, $t:ty, $key:expr, $field:ident, $doc:literal, choice($ty:ty)) => {
        $crate::config::vocab::attr!(
            @$how $t, $key, $field, $doc,
            $crate::config::dispatch::Kind::Choice(<$ty as $crate::config::Named>::names),
            |v: &$ty| $crate::config::Value::named(*v),
            |v: &::kdl::KdlValue, t: &str, s: ::miette::SourceSpan| {
                $crate::config::value::ValueExt::one::<$ty>(v, t, s)
            }
        )
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal,
     custom($kind:expr, $get:expr, $put:expr $(,)?)) => {
        ($key, $kind, $doc, $get, $put)
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, opt $shape:ident $(($($arg:tt)*))?) => {
        $crate::config::vocab::attr!(@shape opt, $t, $key, $field, $doc, $shape $(($($arg)*))?)
    };

    (@row $t:ty, $key:expr, $field:ident, $doc:literal, $shape:ident $(($($arg:tt)*))?) => {
        $crate::config::vocab::attr!(@shape set, $t, $key, $field, $doc, $shape $(($($arg)*))?)
    };
}

pub(crate) use attr;

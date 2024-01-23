use std::fmt;

#[cfg(any(ruby_lte_3_1, docsrs))]
use rb_sys::{rb_binding_new, VALUE};

use crate::{
    error::Error,
    into_value::IntoValue,
    object::Object,
    r_string::IntoRString,
    symbol::IntoSymbol,
    try_convert::TryConvert,
    value::{
        private::{self, ReprValue as _},
        NonZeroValue, ReprValue, Value,
    },
    Ruby,
};

/// A Value known to be an instance of Binding.
///
/// See the [`ReprValue`] and [`Object`] traits for additional methods
/// available on this type.
#[derive(Clone, Copy)]
#[repr(transparent)]
#[deprecated(since = "0.6.0", note = "Please use `Value` instead.")]
pub struct Binding(NonZeroValue);

#[allow(deprecated)]
impl Binding {
    /// Create a new `Binding` from the current Ruby execution context.
    ///
    /// # Panics
    ///
    /// Panics if called from a non-Ruby thread.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use magnus::Binding;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let binding = Binding::new();
    /// ```
    #[allow(clippy::new_without_default)]
    #[cfg(any(ruby_lte_3_1, docsrs))]
    #[cfg_attr(docsrs, doc(cfg(ruby_lte_3_1)))]
    #[deprecated(since = "0.2.0", note = "this will no longer function as of Ruby 3.2")]
    #[inline]
    pub fn new() -> Self {
        crate::error::protect(|| unsafe { Binding::from_rb_value_unchecked(rb_binding_new()) })
            .unwrap()
    }

    #[cfg(any(ruby_lte_3_1, docsrs))]
    #[inline]
    pub(crate) unsafe fn from_rb_value_unchecked(val: VALUE) -> Self {
        Self(NonZeroValue::new_unchecked(Value::new(val)))
    }

    /// Return `Some(Binding)` if `val` is a `Binding`, `None` otherwise.
    #[deprecated(since = "0.6.0")]
    #[inline]
    pub fn from_value(val: Value) -> Option<Self> {
        unsafe {
            val.is_kind_of(Ruby::get_with(val).class_binding())
                .then(|| Self(NonZeroValue::new_unchecked(val)))
        }
    }

    /// Evaluate a string of Ruby code within the binding's context.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use magnus::{eval, Binding};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let binding = eval::<Binding>("binding").unwrap();
    /// assert_eq!(binding.eval::<_, i64>("1 + 2").unwrap(), 3);
    /// ```
    #[deprecated(
        since = "0.6.0",
        note = "Please use `value.funcall(\"eval\", (s,))` instead."
    )]
    pub fn eval<T, U>(self, s: T) -> Result<U, Error>
    where
        T: IntoRString,
        U: TryConvert,
    {
        self.funcall("eval", (s.into_r_string_with(&Ruby::get_with(self)),))
    }

    /// Get the named local variable from the binding.
    ///
    /// Returns `Ok(T)` if the method returns without error and the return
    /// value converts to a `T`, or returns `Err` if the local variable does
    /// not exist or the conversion fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use magnus::{eval, Binding, Value};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let binding = eval::<Binding>("binding").unwrap();
    /// binding.local_variable_set("a", 1);
    /// assert_eq!(binding.local_variable_get::<_, i64>("a").unwrap(), 1);
    /// assert!(binding.local_variable_get::<_, Value>("b").is_err());
    /// ```
    #[deprecated(
        since = "0.6.0",
        note = "Please use `value.funcall(\"local_variable_get\", (name,))` instead."
    )]
    pub fn local_variable_get<N, T>(self, name: N) -> Result<T, Error>
    where
        N: IntoSymbol,
        T: TryConvert,
    {
        self.funcall(
            "local_variable_get",
            (name.into_symbol_with(&Ruby::get_with(self)),),
        )
    }

    /// Set the named local variable in the binding.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use magnus::{eval, Binding};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let binding = eval::<Binding>("binding").unwrap();
    /// binding.local_variable_set("a", 1);
    /// assert_eq!(binding.local_variable_get::<_, i64>("a").unwrap(), 1);
    /// ```
    #[deprecated(
        since = "0.6.0",
        note = "Please use `value.funcall(\"local_variable_set\", (name, val))` instead."
    )]
    pub fn local_variable_set<N, T>(self, name: N, val: T)
    where
        N: IntoSymbol,
        T: IntoValue,
    {
        self.funcall::<_, _, Value>(
            "local_variable_set",
            (name.into_symbol_with(&Ruby::get_with(self)), val),
        )
        .unwrap();
    }
}

#[allow(deprecated)]
impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", unsafe { self.to_s_infallible() })
    }
}

#[allow(deprecated)]
impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inspect())
    }
}

#[allow(deprecated)]
impl IntoValue for Binding {
    #[inline]
    fn into_value_with(self, _: &Ruby) -> Value {
        self.0.get()
    }
}

#[allow(deprecated)]
impl Object for Binding {}

#[allow(deprecated)]
unsafe impl private::ReprValue for Binding {}

#[allow(deprecated)]
impl ReprValue for Binding {}

#[allow(deprecated)]
impl TryConvert for Binding {
    fn try_convert(val: Value) -> Result<Self, Error> {
        Self::from_value(val).ok_or_else(|| {
            Error::new(
                Ruby::get_with(val).exception_type_error(),
                format!("no implicit conversion of {} into Binding", unsafe {
                    val.classname()
                },),
            )
        })
    }
}

/// Evaluate a literal string of Ruby code with the given local variables.
///
/// Any type that implements [`IntoValue`] can be passed to Ruby.
///
/// See also the [`eval`](fn@crate::eval) function and [`Binding`].
///
/// # Panics
///
/// Panics if called from a non-Ruby thread.
///
/// # Examples
///
/// ```
/// # let _cleanup = unsafe { magnus::embed::init() };
/// let result: i64 = magnus::eval!("a + b", a = 1, b = 2).unwrap();
/// assert_eq!(result, 3)
/// ```
/// ```
/// # let _cleanup = unsafe { magnus::embed::init() };
/// let a = 1;
/// let b = 2;
/// let result: i64 = magnus::eval!("a + b", a, b).unwrap();
/// assert_eq!(result, 3);
/// ```
#[macro_export]
macro_rules! eval {
    ($str:literal) => {{
        $crate::eval!($crate::Ruby::get().unwrap(), $str)
    }};
    ($str:literal, $($bindings:tt)*) => {{
        $crate::eval!($crate::Ruby::get().unwrap(), $str, $($bindings)*)
    }};
    ($ruby:expr, $str:literal) => {{
        use $crate::{r_string::IntoRString, value::ReprValue};
        $ruby
            .eval::<$crate::Value>("binding")
            .unwrap()
            .funcall("eval", ($str.into_r_string_with(&$ruby),))
    }};
    ($ruby:expr, $str:literal, $($bindings:tt)*) => {{
        use $crate::{r_string::IntoRString, value::ReprValue};
        let binding = $ruby.eval::<$crate::Value>("binding").unwrap();
        $crate::bind!(binding, $($bindings)*);
        binding.funcall("eval", ($str.into_r_string_with(&$ruby),))
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! bind {
    ($binding:ident,) => {};
    ($binding:ident, $k:ident = $v:expr) => {{
        use $crate::symbol::IntoSymbol;
        let _: $crate::Value = $binding.funcall(
            "local_variable_set",
            (stringify!($k).into_symbol_with(&$crate::Ruby::get_with($binding)), $v),
        )
        .unwrap();
    }};
    ($binding:ident, $k:ident) => {{
        use $crate::symbol::IntoSymbol;
        let _: $crate::Value = $binding.funcall(
            "local_variable_set",
            (stringify!($k).into_symbol_with(&$crate::Ruby::get_with($binding)), $k),
        )
        .unwrap();
    }};
    ($binding:ident, $k:ident = $v:expr, $($rest:tt)*) => {{
        use $crate::symbol::IntoSymbol;
        let _: $crate::Value = $binding.funcall(
            "local_variable_set",
            (stringify!($k).into_symbol_with(&$crate::Ruby::get_with($binding)), $v),
        )
        .unwrap();
        $crate::bind!($binding, $($rest)*);
    }};
    ($binding:ident, $k:ident, $($rest:tt)*) => {{
        use $crate::symbol::IntoSymbol;
        let _: $crate::Value = $binding.funcall(
            "local_variable_set",
            (stringify!($k).into_symbol_with(&$crate::Ruby::get_with($binding)), $k),
        )
        .unwrap();
        $crate::bind!($binding, $($rest)*);
    }};
}

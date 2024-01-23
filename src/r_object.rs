use std::fmt;

use rb_sys::ruby_value_type;

use crate::{
    error::Error,
    into_value::IntoValue,
    object::Object,
    try_convert::TryConvert,
    value::{
        private::{self, ReprValue as _},
        NonZeroValue, ReprValue, Value,
    },
    Ruby,
};

/// A Value pointer to a RObject struct, Ruby's internal representation of
/// generic objects, not covered by the other R* types.
///
/// See the [`ReprValue`] and [`Object`] traits for additional methods
/// available on this type.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct RObject(NonZeroValue);

impl RObject {
    /// Return `Some(RObject)` if `val` is a `RObject`, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RObject};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// assert!(RObject::from_value(eval("Object.new").unwrap()).is_some());
    ///
    /// // many built-in types have specialised implementations and are not
    /// // RObjects
    /// assert!(RObject::from_value(eval(r#""example""#).unwrap()).is_none());
    /// assert!(RObject::from_value(eval("1").unwrap()).is_none());
    /// assert!(RObject::from_value(eval("[]").unwrap()).is_none());
    /// ```
    #[inline]
    pub fn from_value(val: Value) -> Option<Self> {
        unsafe {
            (val.rb_type() == ruby_value_type::RUBY_T_OBJECT)
                .then(|| Self(NonZeroValue::new_unchecked(val)))
        }
    }
}

impl fmt::Display for RObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", unsafe { self.to_s_infallible() })
    }
}

impl fmt::Debug for RObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inspect())
    }
}

impl IntoValue for RObject {
    #[inline]
    fn into_value_with(self, _: &Ruby) -> Value {
        self.0.get()
    }
}

impl Object for RObject {}

unsafe impl private::ReprValue for RObject {}

impl ReprValue for RObject {}

impl TryConvert for RObject {
    fn try_convert(val: Value) -> Result<Self, Error> {
        Self::from_value(val).ok_or_else(|| {
            Error::new(
                Ruby::get_with(val).exception_type_error(),
                format!("no implicit conversion of {} into Object", unsafe {
                    val.classname()
                },),
            )
        })
    }
}

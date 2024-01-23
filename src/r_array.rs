//! Types and functions for working with Ruby’s Array class.

use std::{cmp::Ordering, convert::Infallible, fmt, marker::PhantomData, os::raw::c_long, slice};

#[cfg(ruby_gte_3_2)]
use rb_sys::rb_ary_hidden_new;
#[cfg(ruby_lt_3_2)]
use rb_sys::rb_ary_tmp_new as rb_ary_hidden_new;
#[cfg(ruby_gte_3_0)]
use rb_sys::ruby_rarray_consts::RARRAY_EMBED_LEN_SHIFT;
#[cfg(ruby_lt_3_0)]
use rb_sys::ruby_rarray_flags::RARRAY_EMBED_LEN_SHIFT;
use rb_sys::{
    self, rb_ary_assoc, rb_ary_cat, rb_ary_clear, rb_ary_cmp, rb_ary_concat, rb_ary_delete,
    rb_ary_delete_at, rb_ary_entry, rb_ary_includes, rb_ary_join, rb_ary_new, rb_ary_new_capa,
    rb_ary_new_from_values, rb_ary_plus, rb_ary_pop, rb_ary_push, rb_ary_rassoc, rb_ary_replace,
    rb_ary_resize, rb_ary_reverse, rb_ary_rotate, rb_ary_shared_with_p, rb_ary_shift,
    rb_ary_sort_bang, rb_ary_store, rb_ary_subseq, rb_ary_to_ary, rb_ary_unshift,
    rb_check_array_type, rb_obj_hide, rb_obj_reveal, ruby_rarray_flags, ruby_value_type,
    RARRAY_CONST_PTR, RARRAY_LEN, VALUE,
};
use seq_macro::seq;

use crate::{
    enumerator::Enumerator,
    error::{protect, Error},
    gc,
    into_value::{IntoValue, IntoValueFromNative},
    object::Object,
    r_string::{IntoRString, RString},
    try_convert::{TryConvert, TryConvertOwned},
    value::{
        private::{self, ReprValue as _},
        NonZeroValue, ReprValue, Value,
    },
    Ruby,
};

/// # `RArray`
///
/// Functions that can be used to create Ruby `Array`s.
///
/// See also the [`RArray`] type.
impl Ruby {
    /// Create a new empty `RArray`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.ary_new();
    ///     assert!(ary.is_empty());
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn ary_new(&self) -> RArray {
        unsafe { RArray::from_rb_value_unchecked(rb_ary_new()) }
    }

    /// Create a new empty `RArray` with capacity for `n` elements
    /// pre-allocated.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.ary_new_capa(16);
    ///     assert!(ary.is_empty());
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn ary_new_capa(&self, n: usize) -> RArray {
        unsafe { RArray::from_rb_value_unchecked(rb_ary_new_capa(n as c_long)) }
    }

    /// Create a new `RArray` from a Rust vector.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.ary_from_vec(vec![1, 2, 3]);
    ///     rb_assert!(ruby, "ary == [1, 2, 3]", ary);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn ary_from_vec<T>(&self, vec: Vec<T>) -> RArray
    where
        T: IntoValueFromNative,
    {
        self.ary_from_iter(vec)
    }

    /// Create a new `RArray` containing the elements in `slice`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{prelude::*, rb_assert, Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.ary_new_from_values(&[
    ///         ruby.to_symbol("a").as_value(),
    ///         ruby.integer_from_i64(1).as_value(),
    ///         ruby.qnil().as_value(),
    ///     ]);
    ///     rb_assert!(ruby, "ary == [:a, 1, nil]", ary);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    ///
    /// ```
    /// use magnus::{rb_assert, Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.ary_new_from_values(&[
    ///         ruby.to_symbol("a"),
    ///         ruby.to_symbol("b"),
    ///         ruby.to_symbol("c"),
    ///     ]);
    ///     rb_assert!(ruby, "ary == [:a, :b, :c]", ary);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn ary_new_from_values<T>(&self, slice: &[T]) -> RArray
    where
        T: ReprValue,
    {
        let ptr = slice.as_ptr() as *const VALUE;
        unsafe {
            RArray::from_rb_value_unchecked(rb_ary_new_from_values(slice.len() as c_long, ptr))
        }
    }

    /// Create a new `RArray` from a Rust iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.ary_from_iter((1..4).map(|i| i * 10));
    ///     rb_assert!(ruby, "ary == [10, 20, 30]", ary);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn ary_from_iter<I, T>(&self, iter: I) -> RArray
    where
        I: IntoIterator<Item = T>,
        T: IntoValue,
    {
        self.ary_try_from_iter(iter.into_iter().map(Result::<_, Infallible>::Ok))
            .unwrap()
    }

    /// Create a new `RArray` from a fallible Rust iterator.
    ///
    /// Returns `Ok(RArray)` on sucess or `Err(E)` with the first error
    /// encountered.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby
    ///         .ary_try_from_iter("1,2,3,4".split(',').map(|s| s.parse::<i64>()))
    ///         .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
    ///     rb_assert!(ruby, "ary == [1, 2, 3, 4]", ary);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    ///
    /// ```
    /// use magnus::{Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let err = ruby
    ///         .ary_try_from_iter("1,2,foo,4".split(',').map(|s| s.parse::<i64>()))
    ///         .unwrap_err();
    ///     assert_eq!(err.to_string(), "invalid digit found in string");
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn ary_try_from_iter<I, T, E>(&self, iter: I) -> Result<RArray, E>
    where
        I: IntoIterator<Item = Result<T, E>>,
        T: IntoValue,
    {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let ary = if lower > 0 {
            self.ary_new_capa(lower)
        } else {
            self.ary_new()
        };
        let mut buffer = [self.qnil().as_value(); 128];
        let mut i = 0;
        for v in iter {
            buffer[i] = self.into_value(v?);
            i += 1;
            if i >= buffer.len() {
                i = 0;
                ary.cat(&buffer).unwrap();
            }
        }
        ary.cat(&buffer[..i]).unwrap();
        Ok(ary)
    }

    /// Create a new Ruby Array that may only contain elements of type `T`.
    ///
    /// On creation this Array is hidden from Ruby, and must be consumed to
    /// pass it to Ruby (where it reverts to a regular untyped Array). It is
    /// then inaccessible to Rust.
    ///
    /// ```
    /// use magnus::{rb_assert, Error, Ruby};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ary = ruby.typed_ary_new::<f64>();
    ///     ary.push("1".parse().unwrap())?;
    ///     ary.push("2.3".parse().unwrap())?;
    ///     ary.push("4.5".parse().unwrap())?;
    ///     rb_assert!(ruby, "ary == [1.0, 2.3, 4.5]", ary);
    ///     // ary has moved and can no longer be used.
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn typed_ary_new<T>(&self) -> TypedArray<T> {
        unsafe {
            let ary = rb_ary_hidden_new(0);
            TypedArray(NonZeroValue::new_unchecked(Value::new(ary)), PhantomData)
        }
    }
}

/// A Value pointer to a RArray struct, Ruby's internal representation of an
/// Array.
///
/// See the [`ReprValue`] and [`Object`] traits for additional methods
/// available on this type. See [`Ruby`](Ruby#rarray) for methods to create an
/// `RArray`.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct RArray(NonZeroValue);

impl RArray {
    /// Return `Some(RArray)` if `val` is a `RArray`, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// assert!(RArray::from_value(eval(r#"[true, 0, "example"]"#).unwrap()).is_some());
    /// assert!(RArray::from_value(eval(r#"{"answer" => 42}"#).unwrap()).is_none());
    /// assert!(RArray::from_value(eval(r"nil").unwrap()).is_none());
    /// ```
    #[inline]
    pub fn from_value(val: Value) -> Option<Self> {
        unsafe {
            (val.rb_type() == ruby_value_type::RUBY_T_ARRAY)
                .then(|| Self(NonZeroValue::new_unchecked(val)))
        }
    }

    #[inline]
    pub(crate) unsafe fn from_rb_value_unchecked(val: VALUE) -> Self {
        Self(NonZeroValue::new_unchecked(Value::new(val)))
    }

    /// Create a new empty `RArray`.
    ///
    /// # Panics
    ///
    /// Panics if called from a non-Ruby thread. See [`Ruby::ary_new`] for the
    /// non-panicking version.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// assert!(ary.is_empty());
    /// ```
    #[cfg_attr(
        not(feature = "old-api"),
        deprecated(note = "please use `Ruby::ary_new` instead")
    )]
    #[inline]
    pub fn new() -> Self {
        get_ruby!().ary_new()
    }

    /// Create a new empty `RArray` with capacity for `n` elements
    /// pre-allocated.
    ///
    /// # Panics
    ///
    /// Panics if called from a non-Ruby thread. See [`Ruby::ary_new_capa`] for
    /// the non-panicking version.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::with_capacity(16);
    /// assert!(ary.is_empty());
    /// ```
    #[cfg_attr(
        not(feature = "old-api"),
        deprecated(note = "please use `Ruby::ary_new_capa` instead")
    )]
    #[inline]
    pub fn with_capacity(n: usize) -> Self {
        get_ruby!().ary_new_capa(n)
    }

    /// Convert or wrap a Ruby [`Value`] to a `RArray`.
    ///
    /// If `val` responds to `#to_ary` calls that and passes on the returned
    /// array, otherwise returns a single element array containing `val`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, IntoValue, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::to_ary(1.into_value()).unwrap();
    /// rb_assert!("[1] == ary", ary);
    ///
    /// let ary = RArray::to_ary(vec![1, 2, 3].into_value()).unwrap();
    /// rb_assert!("[1, 2, 3] == ary", ary);
    /// ```
    ///
    /// This can fail in the case of a misbehaving `#to_ary` method:
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let val = eval(
    ///     r#"
    /// o = Object.new
    /// def o.to_ary
    ///   "not an array"
    /// end
    /// o
    /// "#,
    /// )
    /// .unwrap();
    /// assert!(RArray::to_ary(val).is_err());
    /// ```
    pub fn to_ary(val: Value) -> Result<Self, Error> {
        protect(|| unsafe { Self::from_rb_value_unchecked(rb_ary_to_ary(val.as_rb_value())) })
    }

    /// Iterates though `self` and checks each element is convertable to a `T`.
    ///
    /// Returns a typed copy of `self`. Mutating the returned copy will not
    /// mutate `self`.
    ///
    /// This makes most sense when `T` is a Ruby type, although that is not
    /// enforced. If `T` is a Rust type then see [`RArray::to_vec`] for an
    /// alternative.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{function, prelude::*, typed_data, Error, RArray, Ruby};
    ///
    /// #[magnus::wrap(class = "Point")]
    /// struct Point {
    ///     x: isize,
    ///     y: isize,
    /// }
    ///
    /// impl Point {
    ///     fn new(x: isize, y: isize) -> Self {
    ///         Self { x, y }
    ///     }
    /// }
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let point_class = ruby.define_class("Point", ruby.class_object())?;
    ///     point_class.define_singleton_method("new", function!(Point::new, 2))?;
    ///
    ///     let ary: RArray = ruby.eval(
    ///         r#"
    ///           [
    ///             Point.new(1, 2),
    ///             Point.new(3, 4),
    ///             Point.new(5, 6),
    ///           ]
    ///         "#,
    ///     )?;
    ///
    ///     let typed = ary.typecheck::<typed_data::Obj<Point>>()?;
    ///     let point = typed.pop()?;
    ///     assert_eq!(point.x, 5);
    ///     assert_eq!(point.y, 6);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap();
    /// # let _ = Point { x: 1, y: 2 }.x + Point { x: 3, y: 4 }.y;
    /// ```
    pub fn typecheck<T>(self) -> Result<TypedArray<T>, Error>
    where
        T: TryConvert,
    {
        for r in self.each() {
            T::try_convert(r?)?;
        }
        unsafe {
            let ary = rb_ary_hidden_new(0);
            rb_ary_replace(ary, self.as_rb_value());
            Ok(TypedArray(
                NonZeroValue::new_unchecked(Value::new(ary)),
                PhantomData,
            ))
        }
    }

    /// Create a new `RArray` that is a duplicate of `self`.
    ///
    /// The new array is only a shallow clone.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let a = RArray::from_vec(vec![1, 2, 3]);
    /// let b = a.dup();
    /// rb_assert!("a == b", a, b);
    /// a.push(4).unwrap();
    /// b.push(5).unwrap();
    /// rb_assert!("a == [1, 2, 3, 4]", a);
    /// rb_assert!("b == [1, 2, 3, 5]", b);
    /// ```
    pub fn dup(self) -> Self {
        // rb_ary_subseq does a cheap copy-on-write
        unsafe { Self::from_rb_value_unchecked(rb_ary_subseq(self.as_rb_value(), 0, c_long::MAX)) }
    }

    /// Return the number of entries in `self` as a Rust [`usize`].
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// assert_eq!(ary.len(), 0);
    ///
    /// let ary: RArray = eval("[:a, :b, :c]").unwrap();
    /// assert_eq!(ary.len(), 3)
    /// ```
    pub fn len(self) -> usize {
        debug_assert_value!(self);
        unsafe { RARRAY_LEN(self.as_rb_value()) as _ }
    }

    /// Return whether self contains any entries or not.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// assert!(ary.is_empty());
    ///
    /// ary.push("foo").unwrap();
    /// assert!(!ary.is_empty());
    /// ```
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if `val` is in `self`, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, value::qnil, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval(r#"[:foo, "bar", 2]"#).unwrap();
    /// assert!(ary.includes(Symbol::new("foo")));
    /// assert!(ary.includes("bar"));
    /// assert!(ary.includes(2));
    /// // 2.0 == 2 in Ruby
    /// assert!(ary.includes(2.0));
    /// assert!(!ary.includes("foo"));
    /// assert!(!ary.includes(qnil()));
    /// ```
    pub fn includes<T>(self, val: T) -> bool
    where
        T: IntoValue,
    {
        let val = Ruby::get_with(self).into_value(val);
        unsafe { Value::new(rb_ary_includes(self.as_rb_value(), val.as_rb_value())).to_bool() }
    }

    /// Concatenate elements from the slice `s` to `self`.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{prelude::*, rb_assert, value::qnil, Integer, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// ary.cat(&[
    ///     Symbol::new("a").as_value(),
    ///     Integer::from_i64(1).as_value(),
    ///     qnil().as_value(),
    /// ])
    /// .unwrap();
    /// rb_assert!("ary == [:a, 1, nil]", ary);
    /// ```
    ///
    /// ```
    /// use magnus::{rb_assert, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// ary.cat(&[Symbol::new("a"), Symbol::new("b"), Symbol::new("c")])
    ///     .unwrap();
    /// rb_assert!("ary == [:a, :b, :c]", ary);
    /// ```
    pub fn cat<T>(self, s: &[T]) -> Result<(), Error>
    where
        T: ReprValue,
    {
        let ptr = s.as_ptr() as *const VALUE;
        protect(|| unsafe { Value::new(rb_ary_cat(self.as_rb_value(), ptr, s.len() as c_long)) })?;
        Ok(())
    }

    /// Concatenate elements from Ruby array `other` to `self`.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let a = RArray::from_vec(vec![1, 2, 3]);
    /// let b = RArray::from_vec(vec!["a", "b", "c"]);
    /// a.concat(b).unwrap();
    /// rb_assert!(r#"a == [1, 2, 3, "a", "b", "c"]"#, a);
    /// rb_assert!(r#"b == ["a", "b", "c"]"#, b);
    /// ```
    pub fn concat(self, other: Self) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_concat(self.as_rb_value(), other.as_rb_value())) })?;
        Ok(())
    }

    /// Create a new `RArray` containing the both the elements in `self` and
    /// `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let a = RArray::from_vec(vec![1, 2, 3]);
    /// let b = RArray::from_vec(vec!["a", "b", "c"]);
    /// let c = a.plus(b);
    /// rb_assert!(r#"c == [1, 2, 3, "a", "b", "c"]"#, c);
    /// rb_assert!(r#"a == [1, 2, 3]"#, a);
    /// rb_assert!(r#"b == ["a", "b", "c"]"#, b);
    /// ```
    pub fn plus(self, other: Self) -> Self {
        unsafe {
            Self::from_rb_value_unchecked(rb_ary_plus(self.as_rb_value(), other.as_rb_value()))
        }
    }

    /// Create a new `RArray` containing the elements in `slice`.
    ///
    /// # Panics
    ///
    /// Panics if called from a non-Ruby thread. See
    /// [`Ruby::ary_new_from_values`] for the non-panicking version.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{prelude::*, rb_assert, value::qnil, Integer, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_slice(&[
    ///     Symbol::new("a").as_value(),
    ///     Integer::from_i64(1).as_value(),
    ///     qnil().as_value(),
    /// ]);
    /// rb_assert!("ary == [:a, 1, nil]", ary);
    /// ```
    ///
    /// ```
    /// use magnus::{rb_assert, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_slice(&[Symbol::new("a"), Symbol::new("b"), Symbol::new("c")]);
    /// rb_assert!("ary == [:a, :b, :c]", ary);
    /// ```
    #[cfg_attr(
        not(feature = "old-api"),
        deprecated(note = "please use `Ruby::ary_new_from_values` instead")
    )]
    #[inline]
    pub fn from_slice<T>(slice: &[T]) -> Self
    where
        T: ReprValue,
    {
        get_ruby!().ary_new_from_values(slice)
    }

    /// Add `item` to the end of `self`.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// ary.push(Symbol::new("a")).unwrap();
    /// ary.push(1).unwrap();
    /// ary.push(()).unwrap();
    /// rb_assert!("ary == [:a, 1, nil]", ary);
    /// ```
    pub fn push<T>(self, item: T) -> Result<(), Error>
    where
        T: IntoValue,
    {
        let item = Ruby::get_with(self).into_value(item);
        protect(|| unsafe { Value::new(rb_ary_push(self.as_rb_value(), item.as_rb_value())) })?;
        Ok(())
    }

    /// Remove and return the last element of `self`, converting it to a `T`.
    ///
    /// Errors if `self` is frozen or if the conversion fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert_eq!(ary.pop::<i64>().unwrap(), 3);
    /// assert_eq!(ary.pop::<i64>().unwrap(), 2);
    /// assert_eq!(ary.pop::<i64>().unwrap(), 1);
    /// assert!(ary.pop::<i64>().is_err());
    /// ```
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert_eq!(ary.pop::<Option<i64>>().unwrap(), Some(3));
    /// assert_eq!(ary.pop::<Option<i64>>().unwrap(), Some(2));
    /// assert_eq!(ary.pop::<Option<i64>>().unwrap(), Some(1));
    /// assert_eq!(ary.pop::<Option<i64>>().unwrap(), None);
    /// ```
    pub fn pop<T>(self) -> Result<T, Error>
    where
        T: TryConvert,
    {
        protect(|| unsafe { Value::new(rb_ary_pop(self.as_rb_value())) })
            .and_then(TryConvert::try_convert)
    }

    /// Add `item` to the beginning of `self`.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::new();
    /// ary.unshift(Symbol::new("a")).unwrap();
    /// ary.unshift(1).unwrap();
    /// ary.unshift(()).unwrap();
    /// rb_assert!("ary == [nil, 1, :a]", ary);
    /// ```
    pub fn unshift<T>(self, item: T) -> Result<(), Error>
    where
        T: IntoValue,
    {
        let item = Ruby::get_with(self).into_value(item);
        protect(|| unsafe { Value::new(rb_ary_unshift(self.as_rb_value(), item.as_rb_value())) })?;
        Ok(())
    }

    /// Remove and return the first element of `self`, converting it to a `T`.
    ///
    /// Errors if `self` is frozen or if the conversion fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert_eq!(ary.shift::<i64>().unwrap(), 1);
    /// assert_eq!(ary.shift::<i64>().unwrap(), 2);
    /// assert_eq!(ary.shift::<i64>().unwrap(), 3);
    /// assert!(ary.shift::<i64>().is_err());
    /// ```
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert_eq!(ary.shift::<Option<i64>>().unwrap(), Some(1));
    /// assert_eq!(ary.shift::<Option<i64>>().unwrap(), Some(2));
    /// assert_eq!(ary.shift::<Option<i64>>().unwrap(), Some(3));
    /// assert_eq!(ary.shift::<Option<i64>>().unwrap(), None);
    /// ```
    pub fn shift<T>(self) -> Result<T, Error>
    where
        T: TryConvert,
    {
        protect(|| unsafe { Value::new(rb_ary_shift(self.as_rb_value())) })
            .and_then(TryConvert::try_convert)
    }

    /// Remove all elements from `self` that match `item`'s `==` method.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec(vec![1, 1, 2, 3]);
    /// ary.delete(1).unwrap();
    /// rb_assert!("ary == [2, 3]", ary);
    /// ```
    pub fn delete<T>(self, item: T) -> Result<(), Error>
    where
        T: IntoValue,
    {
        let item = Ruby::get_with(self).into_value(item);
        protect(|| unsafe { Value::new(rb_ary_delete(self.as_rb_value(), item.as_rb_value())) })?;
        Ok(())
    }

    /// Remove and return the element of `self` at `index`, converting it to a
    /// `T`.
    ///
    /// `index` may be negative, in which case it counts backward from the end
    /// of the array.
    ///
    /// Returns `Err` if `self` is frozen or if the conversion fails.
    ///
    /// The returned element will be Ruby's `nil` when `index` is out of bounds
    /// this makes it impossible to distingush between out of bounds and
    /// removing `nil` without an additional length check.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec(vec!["a", "b", "c"]);
    /// let removed: Option<String> = ary.delete_at(1).unwrap();
    /// assert_eq!(removed, Some(String::from("b")));
    /// rb_assert!(r#"ary == ["a", "c"]"#, ary);
    /// ```
    pub fn delete_at<T>(self, index: isize) -> Result<T, Error>
    where
        T: TryConvert,
    {
        protect(|| unsafe { Value::new(rb_ary_delete_at(self.as_rb_value(), index as c_long)) })
            .and_then(TryConvert::try_convert)
    }

    /// Remove all elements from `self`
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec::<i64>(vec![1, 2, 3]);
    /// assert!(!ary.is_empty());
    /// ary.clear().unwrap();
    /// assert!(ary.is_empty());
    /// ```
    pub fn clear(self) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_clear(self.as_rb_value())) })?;
        Ok(())
    }

    /// Expand or shrink the length of `self`.
    ///
    /// When increasing the length of the array empty positions will be filled
    /// with `nil`.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec::<i64>(vec![1, 2, 3]);
    /// ary.resize(5).unwrap();
    /// rb_assert!("ary == [1, 2, 3, nil, nil]", ary);
    /// ary.resize(2).unwrap();
    /// rb_assert!("ary == [1, 2]", ary);
    /// ```
    pub fn resize(self, len: usize) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_resize(self.as_rb_value(), len as c_long)) })?;
        Ok(())
    }

    /// Reverses the order of `self` in place.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec::<i64>(vec![1, 2, 3]);
    /// ary.reverse().unwrap();
    /// rb_assert!("ary == [3, 2, 1]", ary);
    /// ```
    pub fn reverse(self) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_reverse(self.as_rb_value())) })?;
        Ok(())
    }

    /// Rotates the elements of `self` in place by `rot` positions.
    ///
    /// If `rot` is positive elements are rotated to the left, if negative,
    /// to the right.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec::<i64>(vec![1, 2, 3, 4, 5, 6, 7]);
    /// ary.rotate(3).unwrap();
    /// rb_assert!("ary == [4, 5, 6, 7, 1, 2, 3]", ary);
    /// ```
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec::<i64>(vec![1, 2, 3, 4, 5, 6, 7]);
    /// ary.rotate(-3).unwrap();
    /// rb_assert!("ary == [5, 6, 7, 1, 2, 3, 4]", ary);
    /// ```
    pub fn rotate(self, rot: isize) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_rotate(self.as_rb_value(), rot as c_long)) })?;
        Ok(())
    }

    /// Storts the elements of `self` in place using Ruby's `<=>` operator.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec::<i64>(vec![2, 1, 3]);
    /// ary.sort().unwrap();
    /// rb_assert!("ary == [1, 2, 3]", ary);
    /// ```
    pub fn sort(self) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_sort_bang(self.as_rb_value())) })?;
        Ok(())
    }

    /// Create a new `RArray` from a Rust vector.
    ///
    /// # Panics
    ///
    /// Panics if called from a non-Ruby thread. See [`Ruby::ary_from_vec`] for
    /// the non-panicking version.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec(vec![1, 2, 3]);
    /// rb_assert!("ary == [1, 2, 3]", ary);
    /// ```
    #[cfg_attr(
        not(feature = "old-api"),
        deprecated(note = "please use `Ruby::ary_from_vec` instead")
    )]
    #[inline]
    pub fn from_vec<T>(vec: Vec<T>) -> Self
    where
        T: IntoValueFromNative,
    {
        get_ruby!().ary_from_vec(vec)
    }

    /// Return `self` as a slice of [`Value`]s.
    ///
    /// # Safety
    ///
    /// This is directly viewing memory owned and managed by Ruby. Ruby may
    /// modify or free the memory backing the returned slice, the caller must
    /// ensure this does not happen.
    ///
    /// Ruby must not be allowed to garbage collect or modify `self` while a
    /// refrence to the slice is held.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3, 4, 5]").unwrap();
    /// // must not call any Ruby api that may modify ary while we have a
    /// // refrence to the return value of ::from_slice()
    /// unsafe {
    ///     let middle = RArray::from_slice(&ary.as_slice()[1..4]);
    ///     rb_assert!("middle == [2, 3, 4]", middle);
    /// }
    /// ```
    pub unsafe fn as_slice(&self) -> &[Value] {
        self.as_slice_unconstrained()
    }

    pub(crate) unsafe fn as_slice_unconstrained<'a>(self) -> &'a [Value] {
        debug_assert_value!(self);
        slice::from_raw_parts(
            RARRAY_CONST_PTR(self.as_rb_value()) as *const Value,
            RARRAY_LEN(self.as_rb_value()) as usize,
        )
    }

    /// Convert `self` to a Rust vector of `T`s. Errors if converting any
    /// element in the array fails.
    ///
    /// This will only convert to a map of 'owned' Rust native types. The types
    /// representing Ruby objects can not be stored in a heap-allocated
    /// datastructure like a [`Vec`] as they are hidden from the mark phase
    /// of Ruby's garbage collector, and thus may be prematurely garbage
    /// collected in the following sweep phase.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert_eq!(ary.to_vec::<i64>().unwrap(), vec![1, 2, 3]);
    /// ```
    pub fn to_vec<T>(self) -> Result<Vec<T>, Error>
    where
        T: TryConvertOwned,
    {
        unsafe { self.as_slice().iter().map(|v| T::try_convert(*v)).collect() }
    }

    /// Convert `self` to a Rust array of [`Value`]s, of length `N`.
    ///
    /// Errors if the Ruby array is not of length `N`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert!(ary.to_value_array::<3>().is_ok());
    /// assert!(ary.to_value_array::<2>().is_err());
    /// assert!(ary.to_value_array::<4>().is_err());
    /// ```
    pub fn to_value_array<const N: usize>(self) -> Result<[Value; N], Error> {
        unsafe {
            self.as_slice().try_into().map_err(|_| {
                Error::new(
                    Ruby::get_with(self).exception_type_error(),
                    format!("expected Array of length {}", N),
                )
            })
        }
    }

    /// Convert `self` to a Rust array of `T`s, of length `N`.
    ///
    /// Errors if converting any element in the array fails, or if the Ruby
    /// array is not of length `N`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval("[1, 2, 3]").unwrap();
    /// assert_eq!(ary.to_array::<i64, 3>().unwrap(), [1, 2, 3]);
    /// assert!(ary.to_array::<i64, 2>().is_err());
    /// assert!(ary.to_array::<i64, 4>().is_err());
    /// ```
    pub fn to_array<T, const N: usize>(self) -> Result<[T; N], Error>
    where
        T: TryConvert,
    {
        unsafe {
            let slice = self.as_slice();
            if slice.len() != N {
                return Err(Error::new(
                    Ruby::get_with(self).exception_type_error(),
                    format!("expected Array of length {}", N),
                ));
            }
            // one day might be able to collect direct into an array, but for
            // now need to go via Vec
            slice
                .iter()
                .copied()
                .map(TryConvert::try_convert)
                .collect::<Result<Vec<T>, Error>>()
                .map(|v| v.try_into().ok().unwrap())
        }
    }

    /// Stringify the contents of `self` and join the sequence with `sep`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{prelude::*, value::qnil, Integer, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_slice(&[
    ///     Symbol::new("a").as_value(),
    ///     Integer::from_i64(1).as_value(),
    ///     qnil().as_value(),
    /// ]);
    /// assert_eq!(ary.join(", ").unwrap().to_string().unwrap(), "a, 1, ")
    /// ```
    pub fn join<T>(self, sep: T) -> Result<RString, Error>
    where
        T: IntoRString,
    {
        let sep = sep.into_r_string_with(&Ruby::get_with(self));
        protect(|| unsafe {
            RString::from_rb_value_unchecked(rb_ary_join(self.as_rb_value(), sep.as_rb_value()))
        })
    }

    /// Return the element at `offset`, converting it to a `T`.
    ///
    /// Errors if the conversion fails.
    ///
    /// An offset out of range will return `nil`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary: RArray = eval(r#"["a", "b", "c"]"#).unwrap();
    ///
    /// assert_eq!(ary.entry::<String>(0).unwrap(), String::from("a"));
    /// assert_eq!(ary.entry::<char>(0).unwrap(), 'a');
    /// assert_eq!(
    ///     ary.entry::<Option<String>>(0).unwrap(),
    ///     Some(String::from("a"))
    /// );
    /// assert_eq!(ary.entry::<String>(1).unwrap(), String::from("b"));
    /// assert_eq!(ary.entry::<String>(-1).unwrap(), String::from("c"));
    /// assert_eq!(ary.entry::<Option<String>>(3).unwrap(), None);
    ///
    /// assert!(ary.entry::<i64>(0).is_err());
    /// assert!(ary.entry::<String>(3).is_err());
    /// ```
    pub fn entry<T>(self, offset: isize) -> Result<T, Error>
    where
        T: TryConvert,
    {
        unsafe {
            T::try_convert(Value::new(rb_ary_entry(
                self.as_rb_value(),
                offset as c_long,
            )))
        }
    }

    /// Set the element at `offset`.
    ///
    /// If `offset` is beyond the current size of the array the array will be
    /// expanded and padded with `nil`.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray, Symbol};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_slice(&[Symbol::new("a"), Symbol::new("b"), Symbol::new("c")]);
    /// ary.store(0, Symbol::new("d")).unwrap();
    /// ary.store(5, Symbol::new("e")).unwrap();
    /// ary.store(6, Symbol::new("f")).unwrap();
    /// ary.store(-1, Symbol::new("g")).unwrap();
    /// rb_assert!("ary == [:d, :b, :c, nil, nil, :e, :g]", ary);
    /// ```
    pub fn store<T>(self, offset: isize, val: T) -> Result<(), Error>
    where
        T: IntoValue,
    {
        let handle = Ruby::get_with(self);
        let val = handle.into_value(val);
        protect(|| {
            unsafe { rb_ary_store(self.as_rb_value(), offset as c_long, val.as_rb_value()) };
            handle.qnil()
        })?;
        Ok(())
    }

    /// Returns an [`Enumerator`] over `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{eval, prelude::*, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let mut res = Vec::new();
    /// for i in eval::<RArray>("[1, 2, 3]").unwrap().each() {
    ///     res.push(i64::try_convert(i.unwrap()).unwrap());
    /// }
    /// assert_eq!(res, vec![1, 2, 3]);
    /// ```
    pub fn each(self) -> Enumerator {
        // TODO why doesn't rb_ary_each work?
        self.enumeratorize("each", ())
    }

    /// Returns true if both `self` and `other` share the same backing storage.
    ///
    /// It is possible for two Ruby Arrays to share the same backing storage,
    /// and only when one of them is modified will the copy-on-write cost be
    /// paid.
    ///
    /// Currently, this method will only return `true` if `self` and `other`
    /// are of the same length, even though Ruby may continue to use the same
    /// backing storage after popping a value from either of the arrays.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec((0..256).collect());
    /// let copy = RArray::new();
    /// copy.replace(ary).unwrap();
    /// assert!(ary.is_shared(copy));
    /// assert!(copy.is_shared(ary));
    /// copy.push(11).unwrap();
    /// assert!(!ary.is_shared(copy));
    /// assert!(!copy.is_shared(ary));
    /// ```
    pub fn is_shared(self, other: Self) -> bool {
        unsafe {
            Value::new(rb_ary_shared_with_p(
                self.as_rb_value(),
                other.as_rb_value(),
            ))
            .to_bool()
        }
    }

    /// Replace the contents of `self` with `from`.
    ///
    /// `from` is unmodified, and `self` becomes a copy of `from`. `self`'s
    /// former contents are abandoned.
    ///
    /// This is a very cheap operation, `self` will point at `from`'s backing
    /// storage until one is modified, and only then will the copy-on-write
    /// cost be paid.
    ///
    /// Returns `Err` if `self` is frozen.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec((0..256).collect());
    /// let copy = RArray::new();
    /// copy.replace(ary).unwrap();
    /// assert!(copy.is_shared(ary));
    /// copy.push(11).unwrap();
    /// assert!(!copy.is_shared(ary));
    /// ```
    pub fn replace(self, from: Self) -> Result<(), Error> {
        protect(|| unsafe { Value::new(rb_ary_replace(self.as_rb_value(), from.as_rb_value())) })?;
        Ok(())
    }

    /// Create a new array from a subsequence of `self`.
    ///
    /// This is a very cheap operation, as `self` and the new array will share
    /// their backing storage until one is modified.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{rb_assert, RArray};
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    /// let a = ary.subseq(0, 5).unwrap();
    /// let b = ary.subseq(5, 5).unwrap();
    /// rb_assert!("a == [1, 2, 3, 4, 5]", a);
    /// rb_assert!("b == [6, 7, 8, 9, 10]", b);
    /// ```
    // TODO maybe take a range instead of offset and length
    pub fn subseq(self, offset: usize, length: usize) -> Option<Self> {
        unsafe {
            let val = Value::new(rb_ary_subseq(
                self.as_rb_value(),
                offset as c_long,
                length as c_long,
            ));
            (!val.is_nil()).then(|| Self::from_rb_value_unchecked(val.as_rb_value()))
        }
    }

    /// Search `self` as an 'associative array' for `key`.
    ///
    /// Assumes `self` is an array of arrays, searching from the start of the
    /// outer array, returns the first inner array where the first element
    /// matches `key`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec(vec![("foo", 1), ("bar", 2), ("baz", 3), ("baz", 4)]);
    /// assert_eq!(
    ///     ary.assoc::<_, (String, i64)>("baz").unwrap(),
    ///     (String::from("baz"), 3)
    /// );
    /// assert_eq!(ary.assoc::<_, Option<(String, i64)>>("quz").unwrap(), None);
    /// ```
    pub fn assoc<K, T>(self, key: K) -> Result<T, Error>
    where
        K: IntoValue,
        T: TryConvert,
    {
        let key = Ruby::get_with(self).into_value(key);
        protect(|| unsafe { Value::new(rb_ary_assoc(self.as_rb_value(), key.as_rb_value())) })
            .and_then(TryConvert::try_convert)
    }

    /// Search `self` as an 'associative array' for `value`.
    ///
    /// Assumes `self` is an array of arrays, searching from the start of the
    /// outer array, returns the first inner array where the second element
    /// matches `value`.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let ary = RArray::from_vec(vec![("foo", 1), ("bar", 2), ("baz", 3), ("qux", 3)]);
    /// assert_eq!(
    ///     ary.rassoc::<_, (String, i64)>(3).unwrap(),
    ///     (String::from("baz"), 3)
    /// );
    /// assert_eq!(ary.rassoc::<_, Option<(String, i64)>>(4).unwrap(), None);
    /// ```
    pub fn rassoc<K, T>(self, value: K) -> Result<T, Error>
    where
        K: IntoValue,
        T: TryConvert,
    {
        let value = Ruby::get_with(self).into_value(value);
        protect(|| unsafe { Value::new(rb_ary_rassoc(self.as_rb_value(), value.as_rb_value())) })
            .and_then(TryConvert::try_convert)
    }

    /// Recursively compares elements of the two arrays using Ruby's `<=>`.
    ///
    /// Returns `Some(Ordering::Equal)` if `self` and `other` are equal.
    /// Returns `Some(Ordering::Less)` if `self` if less than `other`.
    /// Returns `Some(Ordering::Greater)` if `self` if greater than `other`.
    /// Returns `None` if `self` and `other` are not comparable.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::cmp::Ordering;
    ///
    /// use magnus::RArray;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// let a = RArray::from_vec(vec![1, 2, 3]);
    /// let b = RArray::from_vec(vec![1, 2, 3]);
    /// assert_eq!(a.cmp(b).unwrap(), Some(Ordering::Equal));
    ///
    /// let c = RArray::from_vec(vec![1, 2, 0]);
    /// assert_eq!(a.cmp(c).unwrap(), Some(Ordering::Greater));
    ///
    /// let d = RArray::from_vec(vec![1, 2, 4]);
    /// assert_eq!(a.cmp(d).unwrap(), Some(Ordering::Less));
    ///
    /// let e = RArray::from_vec(vec![1, 2]);
    /// e.push(()).unwrap();
    /// assert_eq!(a.cmp(e).unwrap(), None);
    /// ```
    ///
    /// Note that `std::cmp::Ordering` can be cast to `i{8,16,32,64,size}` to
    /// get the Ruby standard `-1`/`0`/`+1` for comparison results.
    ///
    /// ```
    /// assert_eq!(std::cmp::Ordering::Less as i64, -1);
    /// assert_eq!(std::cmp::Ordering::Equal as i64, 0);
    /// assert_eq!(std::cmp::Ordering::Greater as i64, 1);
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn cmp(self, other: Self) -> Result<Option<Ordering>, Error> {
        protect(|| unsafe { Value::new(rb_ary_cmp(self.as_rb_value(), other.as_rb_value())) })
            .and_then(<Option<i64>>::try_convert)
            .map(|opt| opt.map(|i| i.cmp(&0)))
    }
}

impl fmt::Display for RArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", unsafe { self.to_s_infallible() })
    }
}

impl fmt::Debug for RArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inspect())
    }
}

impl IntoValue for RArray {
    #[inline]
    fn into_value_with(self, _: &Ruby) -> Value {
        self.0.get()
    }
}

macro_rules! impl_into_value {
    ($n:literal) => {
        seq!(N in 0..=$n {
            impl<#(T~N,)*> IntoValue for (#(T~N,)*)
            where
                #(T~N: IntoValue,)*
            {
                fn into_value_with(self, handle: &Ruby) -> Value {
                    let ary = [
                        #(handle.into_value(self.N),)*
                    ];
                    handle.ary_new_from_values(&ary).into_value_with(handle)
                }
            }

            unsafe impl<#(T~N,)*> IntoValueFromNative for (#(T~N,)*) where #(T~N: IntoValueFromNative,)* {}
        });
    }
}

seq!(N in 0..12 {
    impl_into_value!(N);
});

impl<T> IntoValue for Vec<T>
where
    T: IntoValueFromNative,
{
    #[inline]
    fn into_value_with(self, handle: &Ruby) -> Value {
        handle.ary_from_vec(self).into_value_with(handle)
    }
}

#[cfg(feature = "old-api")]
impl<T> FromIterator<T> for RArray
where
    T: IntoValue,
{
    /// Creates a Ruby array from an iterator.
    ///
    /// # Panics
    ///
    /// Panics if called from a non-Ruby thread. See [`Ruby::ary_from_iter`]
    /// for the non-panicking version.
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        get_ruby!().ary_from_iter(iter)
    }
}

impl Object for RArray {}

unsafe impl private::ReprValue for RArray {}

impl ReprValue for RArray {}

impl TryConvert for RArray {
    fn try_convert(val: Value) -> Result<Self, Error> {
        if let Some(v) = Self::from_value(val) {
            return Ok(v);
        }
        unsafe {
            protect(|| Value::new(rb_check_array_type(val.as_rb_value()))).and_then(|res| {
                Self::from_value(res).ok_or_else(|| {
                    Error::new(
                        Ruby::get_with(val).exception_type_error(),
                        format!("no implicit conversion of {} into Array", val.class()),
                    )
                })
            })
        }
    }
}

/// A Ruby Array that may only contain elements of type `T`.
///
/// On creation this Array is hidden from Ruby, and must be consumed to
/// pass it to Ruby (where it reverts to a regular untyped Array). It is
/// then inaccessible to Rust.
///
/// See [`Ruby::typed_ary_new`] or [`RArray::typecheck`] for how to get a value
/// of `TypedArray`.
//
// Very deliberately not Copy or Clone so that values of this type are consumed
// when TypedArray::to_array is called, so you can either have typed access
// from Rust *or* expose it to Ruby.
#[repr(transparent)]
pub struct TypedArray<T>(NonZeroValue, PhantomData<T>);

macro_rules! proxy {
    ($method:ident($($arg:ident: $typ:ty),*) -> $ret:ty) => {
        #[doc=concat!("See [`RArray::", stringify!($method), "`].")]
        pub fn $method(&self, $($arg: $typ),*) -> $ret {
            unsafe { RArray::from_value_unchecked(self.0.get()) }.$method($($arg),*)
        }
    };
}

impl<T> TypedArray<T> {
    /// Consume `self`, returning it as an [`RArray`].
    pub fn to_r_array(self) -> RArray {
        let val = self.0.get();
        let ruby = Ruby::get_with(val);
        unsafe {
            rb_obj_reveal(val.as_rb_value(), ruby.class_array().as_rb_value());
            RArray::from_value_unchecked(val)
        }
    }

    proxy!(len() -> usize);
    proxy!(is_empty() -> bool);
    proxy!(clear() -> Result<(), Error>);
    proxy!(resize(len: usize) -> Result<(), Error>);
    proxy!(reverse() -> Result<(), Error>);
    proxy!(rotate(rot: isize) -> Result<(), Error>);
    proxy!(sort() -> Result<(), Error>);

    /// See [`RArray::dup`].
    pub fn dup(&self) -> Self {
        unsafe {
            let dup = RArray::from_value_unchecked(self.0.get()).dup();
            rb_obj_hide(dup.as_rb_value());
            TypedArray(NonZeroValue::new_unchecked(dup.as_value()), PhantomData)
        }
    }

    /// See [`RArray::concat`].
    pub fn concat(&self, other: Self) -> Result<(), Error> {
        unsafe {
            RArray::from_value_unchecked(self.0.get())
                .concat(RArray::from_value_unchecked(other.0.get()))
        }
    }

    /// See [`RArray::plus`].
    pub fn plus(&self, other: Self) -> Self {
        unsafe {
            let new_ary = RArray::from_value_unchecked(self.0.get())
                .plus(RArray::from_value_unchecked(other.0.get()));
            rb_obj_hide(new_ary.as_rb_value());
            TypedArray(NonZeroValue::new_unchecked(new_ary.as_value()), PhantomData)
        }
    }

    /// See [`RArray::as_slice`].
    pub unsafe fn as_slice(&self) -> &[Value] {
        RArray::from_value_unchecked(self.0.get()).as_slice_unconstrained()
    }

    /// See [`RArray::to_value_array`].
    pub fn to_value_array<const N: usize>(&self) -> Result<[Value; N], Error> {
        unsafe { RArray::from_value_unchecked(self.0.get()).to_value_array() }
    }

    /// See [`RArray::join`].
    pub fn join<S>(&self, sep: S) -> Result<RString, Error>
    where
        S: IntoRString,
    {
        unsafe { RArray::from_value_unchecked(self.0.get()).join(sep) }
    }

    // TODO is_shared

    /// See [`RArray::replace`].
    pub fn replace(&self, from: Self) -> Result<(), Error> {
        unsafe {
            RArray::from_value_unchecked(self.0.get())
                .replace(RArray::from_value_unchecked(from.0.get()))
        }
    }

    /// See [`RArray::subseq`].
    pub fn subseq(&self, offset: usize, length: usize) -> Option<Self> {
        unsafe {
            RArray::from_value_unchecked(self.0.get())
                .subseq(offset, length)
                .map(|ary| {
                    rb_obj_hide(ary.as_rb_value());
                    TypedArray(NonZeroValue::new_unchecked(ary.as_value()), PhantomData)
                })
        }
    }

    /// See [`RArray::subseq`].
    #[allow(clippy::should_implement_trait)]
    pub fn cmp(&self, other: Self) -> Result<Option<Ordering>, Error> {
        unsafe {
            RArray::from_value_unchecked(self.0.get())
                .cmp(RArray::from_value_unchecked(other.0.get()))
        }
    }
}

impl<T> TypedArray<T>
where
    T: IntoValue,
{
    proxy!(includes(val: T) -> bool);
    proxy!(push(item: T) -> Result<(), Error>);
    proxy!(unshift(item: T) -> Result<(), Error>);
    proxy!(delete(item: T) -> Result<(), Error>);
    proxy!(store(offset: isize, val: T) -> Result<(), Error>);
}

impl<T> TypedArray<T>
where
    T: ReprValue,
{
    proxy!(cat(s: &[T]) -> Result<(), Error>);
}

impl<T> TypedArray<T>
where
    T: TryConvert,
{
    proxy!(pop() -> Result<T, Error>);
    proxy!(shift() -> Result<T, Error>);
    proxy!(delete_at(index: isize) -> Result<T, Error>);
    proxy!(entry(offset: isize) -> Result<T, Error>);

    /// See [`RArray::to_array`].
    pub fn to_array<const N: usize>(&self) -> Result<[T; N], Error> {
        unsafe { RArray::from_value_unchecked(self.0.get()).to_array() }
    }

    // TODO? assoc & rassoc
}

impl<T> TypedArray<T>
where
    T: TryConvertOwned,
{
    /// See [`RArray::to_vec`].
    pub fn to_vec(&self) -> Vec<T> {
        unsafe { RArray::from_value_unchecked(self.0.get()).to_vec().unwrap() }
    }
}

impl<T> IntoValue for TypedArray<T>
where
    T: IntoValue,
{
    fn into_value_with(self, _: &Ruby) -> Value {
        self.to_r_array().as_value()
    }
}

impl<T> gc::private::Mark for TypedArray<T> {
    fn raw(self) -> VALUE {
        self.0.get().as_rb_value()
    }
}
impl<T> gc::Mark for TypedArray<T> {}

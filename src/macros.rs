/// Debug assertation that the Value hasn't been garbage collected.
// This isn't infallible, if the original object was gc'd and that slot
// reused already this won't panic like it should, but we're trying our
// best here.
macro_rules! debug_assert_value {
    ($value:expr) => {
        // The memory this points to is managed by Ruby's GC and we can't
        // really know if it's safe to access as with GC compaction this may
        // point to memory now outside that owned by the process. We will likly
        // segfault in that case, which is kind of OK, as we're trying to panic
        // anyway.
        #[allow(unused_unsafe)]
        #[cfg(debug_assertions)]
        match unsafe { $crate::value::private::ReprValue::rb_type($value) } {
            ::rb_sys::ruby_value_type::RUBY_T_NONE | ::rb_sys::ruby_value_type::RUBY_T_ZOMBIE => {
                panic!("Attempting to access garbage collected Object")
            }
            ::rb_sys::ruby_value_type::RUBY_T_MOVED => {
                panic!("Attempting to access garbage collected Object")
            }
            _ => (),
        };
    };
}

/// Get a `Ruby`.
///
/// Panics in debug mode if not on a Ruby thread.
/// Undefined behaviour in release if not on a Ruby thread.
///
/// The check is currently not performed in release mode as the Ruby API
/// hasn't been finalised and there isn't an alternative to this that avoids
/// the check and the associated perfomance hit.
macro_rules! get_ruby {
    () => {
        if cfg!(debug_assertions) {
            $crate::Ruby::get().unwrap()
        } else {
            #[allow(unused_unsafe)]
            unsafe {
                $crate::Ruby::get_unchecked()
            }
        }
    };
}

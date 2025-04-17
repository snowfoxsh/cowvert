use std::panic;

/// Allows use of a value pointed to by `&mut T` as though it was owned, as long as a `T` is made available afterwards,
/// and returns an extra value from the closure.
///
/// The closure must return a tuple `(T, R)`, where `T` replaces the old value and `R` is returned.
///
/// # Important
/// Will abort the program if the closure panics.
///
/// # Example
/// ```
/// struct Foo;
/// let mut foo = Foo;
/// // let message = take(&mut foo, |foo| {
/// //     drop(foo);
/// //     (Foo, "Done")
/// // });
/// // assert_eq!(message, "Done");
/// ```
pub fn take<T, F, R>(mut_ref: &mut T, closure: F) -> R
where
    F: FnOnce(T) -> (T, R),
{
    use std::ptr;

    unsafe {
        let old_t = ptr::read(mut_ref);
        let (new_t, result) = panic::catch_unwind(panic::AssertUnwindSafe(|| closure(old_t)))
            .unwrap_or_else(|_| ::std::process::abort());
        ptr::write(mut_ref, new_t);
        result
    }
}
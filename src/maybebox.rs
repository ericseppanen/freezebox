//! This is the MaybeBox implementation.

use alloc::boxed::Box;
use core::any::type_name;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

/// `MaybeBox` is a lazy-initialized container.
///
/// A `MaybeBox<T>` can have two possible states:
/// * uninitialized: it does not contain a `T`, and [`get`] will return `None`.
/// * initialized: it does contain a `T`, and [`get`] will return `Some(&T)`.
///
/// To upgrade a `MaybeBox` to the initialized state, call `lazy_init`.
/// `lazy_init` does not require a mutable reference, making `MaybeBox`
/// suitable for sharing objects first and initializing them later.
///
/// `MaybeBox` does not implement `Deref`; to access the contents call [`get`].
///
/// Attempting to `lazy_init` more than once will cause a panic.
pub struct MaybeBox<T> {
    inner: AtomicPtr<T>,
    phantom: PhantomData<T>,
}

impl<T> MaybeBox<T> {
    /// Create a new `MaybeBox` with optional initialization.
    ///
    /// A pre-inititialized `MaybeBox<T>` may not seem very useful, but it
    /// can be convenient when interacting with structs or interfaces that
    /// require a `MaybeBox`, e.g. unit tests.
    ///
    /// To always create an uninitialized `MaybeBox`, use
    /// `MaybeBox::default()`.
    ///
    pub fn new(val: Option<T>) -> Self {
        match val {
            None => Self::default(),
            Some(v) => {
                let fb = Self::default();
                fb.lazy_init(v);
                fb
            }
        }
    }

    /// Create a new `MaybeBox` in `const` context
    ///
    /// This is the same as `MaybeBox::default` except that it works in
    /// const context, which is desirable for global `static` singleton objects.
    ///
    /// # Examples
    /// ```
    /// # use freezebox::MaybeBox;
    /// static X: MaybeBox<String> = MaybeBox::const_default();
    /// X.lazy_init("hello".to_string());
    /// assert_eq!(X.get_as_deref(), Some("hello"));
    /// ```
    pub const fn const_default() -> Self {
        Self {
            inner: AtomicPtr::new(null_mut()),
            phantom: PhantomData,
        }
    }

    /// Initialize a `MaybeBox`.
    ///
    /// `lazy_init` will panic if the `MaybeBox` is already initialized.
    pub fn lazy_init(&self, val: T) {
        let ptr = Box::into_raw(Box::new(val));

        // Attempt to atomically swap from nullptr to `ptr`.
        //
        // Reasoning about the atomic ordering:
        // On the success side, we don't care about the load ordering,
        // only the store ordering, which must be `Release` or stronger.
        // This is because if we succeed, the previous value was null,
        // which is the state of a newly-created AtomicBox. So it's not
        // possible for us to race with another thread storing the null
        // pointer.
        //
        // On the failure side, we want to detect a race to double init,
        // so we want the load to be `Acquire` or stronger.
        //
        // Because the success ordering must be equal or stronger to the
        // failure ordering, we need to upgrade the success ordering to
        // `AcqRel`.
        //
        // If this succeeds, the MaybeBox is now initialized.
        if self
            .inner
            .compare_exchange(ptr::null_mut(), ptr, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // The compare_exchange failed, meaning a double-init was
            // attempted and we should panic.
            //
            // Before we do, retake ownership of the new pointer so that
            // we don't leak its memory.
            //
            // SAFETY: `ptr` was just created above using `Box::into_raw`.
            // Because compare_exchange failed, we know that it is still
            // the unique owner of the input value. So we can reclaim
            // ownership here and drop the result.

            let _val = unsafe { Box::<T>::from_raw(ptr) };

            panic!(
                "lazy_init on already-initialized MaybeBox<{}>",
                type_name::<T>()
            );
        }
    }

    /// Try to get a reference to the data in the `MaybeBox`.
    ///
    /// If the `MaybeBox` is initialized, this will return `Some(&T)`;
    /// otherwise it will return None.
    pub fn get(&self) -> Option<&T> {
        let ptr = self.inner.load(Ordering::Acquire);
        unsafe { ptr.as_ref() }
    }

    /// Test whether a MaybeBox is initialized.
    pub fn is_initialized(&self) -> bool {
        let ptr = self.inner.load(Ordering::Acquire);
        !ptr.is_null()
    }

    /// Consume the MaybeBox and return its contents.
    pub fn into_inner(self) -> Option<T> {
        let ptr = self.inner.load(Ordering::Acquire);
        // Prevent Drop::drop() from being called on the MaybeBox
        // because we are transferring ownership elsewhere.
        mem::forget(self);
        if ptr.is_null() {
            return None;
        }

        // SAFETY: because we are consuming self, we must have sole ownership
        // of the MaybeBox contents. `lazy_init` created `ptr` from an
        // owning `Box<T>`, so it's safe for us to recreate that Box and drop
        // it.

        let tmp_box = unsafe { Box::from_raw(ptr) };
        Some(*tmp_box)
    }
}

impl<T: Deref> MaybeBox<T> {
    /// Try to get the `Deref::Target` for the data in the `MaybeBox`.
    ///
    /// This is helpful when you want the `Deref` form of the
    /// data in the `MaybeBox`. For example, when called on a
    /// `MaybeBox<String>`, this will return `Option<&str>`.
    pub fn get_as_deref(&self) -> Option<&T::Target> {
        let ptr = self.inner.load(Ordering::Acquire);
        match unsafe { ptr.as_ref() } {
            Some(t) => Some(t.deref()),
            None => None,
        }
    }
}

impl<T> Default for MaybeBox<T> {
    fn default() -> Self {
        Self {
            inner: AtomicPtr::default(),
            phantom: PhantomData,
        }
    }
}

impl<T> Drop for MaybeBox<T> {
    fn drop(&mut self) {
        // We have exclusive access to the container, so this doesn't need
        // to be atomic.
        let inner = self.inner.get_mut();

        if !inner.is_null() {
            // We own an inner object.  Re-hydrate into a Box<T> so that
            // T's destructor may run.
            //
            // SAFETY: We have exclusive access to the inner value, so we can
            // safely drop the contents. We could also reset the pointer, but
            // since this data structure is being dropped, this is the last
            // time that pointer will be seen; so there's no point.

            let _owned = unsafe { Box::<T>::from_raw(*inner) };
            // _owned will drop here.
        }
    }
}

/// Must fail to compile because MaybeBox<Rc> must not be Send.
/// ```compile_fail
/// use freezebox::MaybeBox;
/// use std::rc::Rc;
///
/// fn require_send<T: Send>(_t: &T) {}
///
/// let x = MaybeBox::<Rc<u32>>::new();
/// require_send(&x); // <- must fail to compile.
/// ```
///
/// Must fail to compile because MaybeBox<Cell> must not be Sync.
/// ```compile_fail
/// use freezebox::MaybeBox;
/// use std::cell::Cell;
///
/// fn require_sync<T: Sync>(_t: &T) {}
///
/// let x = MaybeBox::<Cell<u32>>::new();
/// require_sync(&x); // must fail to compile.
/// ```
struct _Unused {} // Only exists to get the compile-fail doctest

#[cfg(test)]
mod tests {
    use super::MaybeBox;
    use alloc::string::String;
    use alloc::string::ToString;
    use alloc::sync::Arc;

    #[test]
    fn freezebox_test() {
        // Arc is used to check whether drop occurred.
        let x = Arc::new("hello".to_string());
        let y: MaybeBox<Arc<String>> = MaybeBox::default();
        assert!(!y.is_initialized());
        assert!(y.get().is_none());
        y.lazy_init(x.clone());
        assert!(y.is_initialized());
        assert_eq!(**y.get().unwrap(), "hello");

        // Verify that dropping the MaybeBox caused its inner value to be dropped too.
        assert_eq!(Arc::strong_count(&x), 2);
        drop(y);
        assert_eq!(Arc::strong_count(&x), 1);
    }

    #[test]
    #[should_panic]
    fn panic_double_init() {
        let x = MaybeBox::<String>::default();
        x.lazy_init("first".to_string());
        x.lazy_init("second".to_string());
    }

    #[test]
    fn consume_test() {
        let x = MaybeBox::<String>::default();
        x.lazy_init("hello".to_string());
        let x2: Option<String> = x.into_inner();
        assert_eq!(x2, Some("hello".to_string()));

        let y = MaybeBox::<String>::default();
        let y2: Option<String> = y.into_inner();
        assert_eq!(y2, None);
    }

    #[test]
    fn const_test() {
        static X: MaybeBox<String> = MaybeBox::const_default();
        X.lazy_init("hello".to_string());
        assert_eq!(X.get().unwrap(), "hello");
    }
}

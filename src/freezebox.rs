//! This is the FreezeBox implementation.

extern crate alloc;
use alloc::boxed::Box;
use core::any::type_name;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

/// `FreezeBox` is a deref'able lazy-initialized container.
///
/// A `FreezeBox<T>` can have two possible states:
/// * uninitialized: deref is not allowed.
/// * initialized: deref to a `&T` is possible.
///
/// To upgrade a `FreezeBox` to the initialized state, call `lazy_init`.
/// `lazy_init` does not require a mutable reference, making `FreezeBox`
/// suitable for sharing objects first and initializing them later.
///
/// Attempting to `lazy_init` more than once, or deref while uninitialized
/// will cause a panic.
pub struct FreezeBox<T> {
    inner: AtomicPtr<T>,
    phantom: PhantomData<T>,
}

impl<T> FreezeBox<T> {
    /// Create a new `FreezeBox` with optional initialization.
    ///
    /// A pre-inititialized `FreezeBox<T>` may not seem very useful, but it
    /// can be convenient when interacting with structs or interfaces that
    /// require a `FreezeBox`, e.g. unit tests.
    ///
    /// To always create an uninitialized `FreezeBox`, use
    /// `FreezeBox::default()`.
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

    /// Create a new `FreezeBox` in `const` context
    ///
    /// This is the same as `FreezeBox::default` except that it works in
    /// const context, which is desirable for global `static` singleton objects.
    ///
    /// # Examples
    /// ```
    /// # use freezebox::FreezeBox;
    /// static X: FreezeBox<String> = FreezeBox::const_default();
    /// X.lazy_init("hello".to_string());
    /// assert_eq!(*X, "hello");
    /// ```
    pub const fn const_default() -> Self {
        Self {
            inner: AtomicPtr::new(null_mut()),
            phantom: PhantomData,
        }
    }

    /// Initialize a `FreezeBox`.
    ///
    /// `lazy_init` will panic if the `FreezeBox` is already initialized.
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
        // If this succeeds, the FreezeBox is now initialized.
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
                "lazy_init on already-initialized FreezeBox<{}>",
                type_name::<T>()
            );
        }
    }

    /// Test whether a FreezeBox is initialized.
    pub fn is_initialized(&self) -> bool {
        let ptr = self.inner.load(Ordering::Acquire);
        !ptr.is_null()
    }

    /// Consume the FreezeBox and return its contents.
    pub fn into_inner(self) -> Option<T> {
        let ptr = self.inner.load(Ordering::Acquire);
        // Prevent Drop::drop() from being called on the FreezeBox
        // because we are transferring ownership elsewhere.
        mem::forget(self);
        if ptr.is_null() {
            return None;
        }

        // SAFETY: because we are consuming self, we must have sole ownership
        // of the FreezeBox contents. `lazy_init` created `ptr` from an
        // owning `Box<T>`, so it's safe for us to recreate that Box and drop
        // it.

        let tmp_box = unsafe { Box::from_raw(ptr) };
        Some(*tmp_box)
    }
}

impl<T> Deref for FreezeBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let inner = self.inner.load(Ordering::Acquire);

        // SAFETY: the inner pointer can only be in two states:
        // 1. Uninitialized (null pointer): inner_ref will be None, and we
        //    should panic, because deref of an uninitialized FreezeBox is
        //    not allowed. Note we never create an actual &T (which would be
        //    undefined behavior).
        // 2. Initialized (valid shared pointer): inner_ref will be Some(&T).
        //    Because we own the inner memory, it's safe for us to hand out
        //    shared references to the inner T for as long as the FreezeBox
        //    lives.

        let inner_ref = unsafe { inner.as_ref() };
        inner_ref.unwrap_or_else(|| {
            panic!(
                "attempted to deref uninitialized FreezeBox<{}>",
                type_name::<T>(),
            )
        })
    }
}

impl<T> Default for FreezeBox<T> {
    fn default() -> Self {
        Self {
            inner: AtomicPtr::default(),
            phantom: PhantomData,
        }
    }
}

impl<T> Drop for FreezeBox<T> {
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

/// Must fail to compile because FreezeBox<Rc> must not be Send.
/// ```compile_fail
/// use freezebox::FreezeBox;
/// use std::rc::Rc;
///
/// fn require_send<T: Send>(_t: &T) {}
///
/// let x = FreezeBox::<Rc<u32>>::new();
/// require_send(&x); // <- must fail to compile.
/// ```
///
/// Must fail to compile because FreezeBox<Cell> must not be Sync.
/// ```compile_fail
/// use freezebox::FreezeBox;
/// use std::cell::Cell;
///
/// fn require_sync<T: Sync>(_t: &T) {}
///
/// let x = FreezeBox::<Cell<u32>>::new();
/// require_sync(&x); // must fail to compile.
/// ```
struct _Unused; // Only exists to get the compile-fail doctest

#[cfg(test)]
mod tests {
    use super::FreezeBox;
    use alloc::string::String;
    use alloc::string::ToString;
    use alloc::sync::Arc;

    #[test]
    fn freezebox_test() {
        // Arc is used to check whether drop occurred.
        let x = Arc::new("hello".to_string());
        let y: FreezeBox<Arc<String>> = FreezeBox::default();
        assert!(!y.is_initialized());
        y.lazy_init(x.clone());
        assert!(y.is_initialized());
        // explicit deref once for FreezeBox and once for Arc.
        assert_eq!(**y, "hello");
        // implicit deref sees through both layers.
        assert_eq!(&y[2..], "llo");

        // Verify that dropping the FreezeBox caused its inner value to be dropped too.
        assert_eq!(Arc::strong_count(&x), 2);
        drop(y);
        assert_eq!(Arc::strong_count(&x), 1);
    }

    #[test]
    #[should_panic]
    fn panic_deref() {
        let x = FreezeBox::<String>::default();
        // dot-operator implicitly deref's the FreezeBox.
        let _y = x.len();
    }

    #[test]
    #[should_panic]
    fn panic_double_init() {
        let x = FreezeBox::<String>::default();
        x.lazy_init("first".to_string());
        x.lazy_init("second".to_string());
    }

    #[test]
    fn consume_test() {
        let x = FreezeBox::<String>::default();
        x.lazy_init("hello".to_string());
        let x2: Option<String> = x.into_inner();
        assert_eq!(x2, Some("hello".to_string()));

        let y = FreezeBox::<String>::default();
        let y2: Option<String> = y.into_inner();
        assert_eq!(y2, None);
    }

    #[test]
    fn const_test() {
        static X: FreezeBox<String> = FreezeBox::const_default();
        X.lazy_init("hello".to_string());
        assert_eq!(*X, "hello");
    }
}

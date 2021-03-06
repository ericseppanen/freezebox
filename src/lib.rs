//! # Rust FreezeBox: a deref'able lazy-initialized container.
//!
//! `FreezeBox<T>` is a container that can have two possible states:
//! * uninitialized: deref is not allowed.
//! * initialized: deref to a `&T` is possible.
//!
//! To upgrade a `FreezeBox` to the initialized state, call `lazy_init`.
//! `lazy_init` does not require a mutable reference, making `FreezeBox`
//! suitable for sharing objects first and initializing them later.
//!
//! Attempting to `lazy_init` more than once, or deref while uninitialized
//! will cause a panic.
//!
//! FreezeBox is compatible with `no_std` projects (no feature flags needed).
//! It may be used in any environment with a memory allocator.
//!
//! # Examples
//!
//! This example creates a shared data structure, then circles back to
//! initialize one member.
//!
//! ```
//! use freezebox::FreezeBox;
//! use std::sync::Arc;
//!
//! #[derive(Default)]
//! struct Resources {
//!     name: FreezeBox<String>
//! }
//!
//! let resources = Arc::new(Resources::default());
//! let res2 = resources.clone();
//!
//! let func = move || {
//!     // explicit deref
//!     assert_eq!(*res2.name, "Hello!");
//!     // implicit deref allows transparent access
//!     assert_eq!(res2.name.len(), 6);
//!     assert_eq!(&res2.name[2..], "llo!");
//! };
//!
//! resources.name.lazy_init("Hello!".to_string());
//! func();
//! ```

#![no_std]

extern crate alloc;
use alloc::boxed::Box;
use core::any::type_name;
use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, Ordering};

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

    /// Initialize a `FreezeBox`.
    ///
    /// `lazy_init` will panic if the `FreezeBox` is already initialized.
    pub fn lazy_init(&self, val: T) {
        let ptr = Box::into_raw(Box::new(val));
        let prev = self.inner.swap(ptr, Ordering::Release);
        if prev != null_mut() {
            // Note we will leak the value in prev.
            panic!(
                "lazy_init on already-initialized FreezeBox<{}>",
                type_name::<T>()
            );
        }
    }

    /// Test whether a FreezeBox is initialized.
    pub fn is_initialized(&self) -> bool {
        let inner = self.inner.load(Ordering::Acquire);
        inner != null_mut()
    }

    /// Consume the FreezeBox and return its contents.
    pub fn into_inner(self) -> Option<T> {
        let ptr = self.inner.load(Ordering::Acquire);
        // Prevent Drop::drop() from being called on the FreezeBox
        // because we are transferring ownership elsewhere.
        mem::forget(self);
        if ptr == null_mut() {
            return None;
        }
        let tmp_box = unsafe { Box::from_raw(ptr) };
        Some(*tmp_box)
    }
}

impl<T> Deref for FreezeBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let inner = self.inner.load(Ordering::Acquire);
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
            inner: AtomicPtr::new(null_mut()),
            phantom: PhantomData,
        }
    }
}

impl<T> Drop for FreezeBox<T> {
    fn drop(&mut self) {
        // We have exclusive access to the container, so this doesn't need
        // to be atomic.
        let inner = self.inner.get_mut();

        if *inner != null_mut() {
            // We own an inner object.  Re-hydrate into a Box<T> so that
            // T's destructor may run.
            let _owned = unsafe { Box::<T>::from_raw(*inner) };
            // _owned will drop here.
        }
    }
}

/// Must fail to compile because FreezeBox<Rc> munt not be Send.
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
struct _Unused {} // Only exists to get the compile-fail doctest

#[cfg(test)]
mod tests {
    use super::FreezeBox;
    use alloc::sync::Arc;
    use alloc::string::String;
    use alloc::string::ToString;

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
    #[cfg_attr(miri, ignore)] // Miri doesn't understand should_panic
    fn panic_deref() {
        let x = FreezeBox::<String>::default();
        // dot-operator implicitly deref's the FreezeBox.
        let _y = x.len();
    }

    #[test]
    #[should_panic]
    #[cfg_attr(miri, ignore)] // Miri doesn't understand should_panic
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
}

//! # FreezeBox: atomic lazy-initialized ref-able containers.
//!
//! This crate contains two similar container types: [`FreezeBox`] and
//! [`MaybeBox`]. Both containers can be late-initialized using only a
//! shared reference, and both allow a caller to get a reference to the
//! value inside.
//!
//! ```
//! # use freezebox::FreezeBox;
//! let x = FreezeBox::<String>::default();
//! x.lazy_init(String::from("hello"));
//! assert_eq!(x.len(), 5);
//! ```
//!
//! This is useful for data structures that are shared first, but some
//! members of that data structure gets initialized later.
//!
//! ```
//! # use freezebox::FreezeBox;
//! # use std::sync::Arc;
//! let x = FreezeBox::<String>::default();
//! let shared_x = Arc::new(x);
//! shared_x.lazy_init(String::from("hello"));
//! assert_eq!(shared_x.len(), 5);
//! ```
//!
//! [`FreezeBox`] and [`MaybeBox`] share some behavior: they can only be
//! initialized once; initialization is atomic; and the initialized value
//! may never be removed, except by consuming the container with
//! `into_inner()`.
//!
//! The main difference between `FreezeBox` and `MaybeBox` is that
//! `FreezeBox` implements `Deref`. `FreezeBox` is intended to be used in situations
//! where the inner value is always expected to be present before an attempted
//! use. In this scenario, trying to read an uninitialized `FreezeBox` is a
//! bug, so the attempted read will cause a panic.
//!
//! `MaybeBox` is meant to be used in a situation where the inner value may
//! sometimes be missing. `MaybeBox` does not implement `Deref`; instead we
//! need to call [`get`][`MaybeBox::get`], which returns `Option<&T>`.
//!
//! ```
//! # use freezebox::MaybeBox;
//! # let some_runtime_config = true;
//! let x = MaybeBox::<String>::default();
//! if some_runtime_config {
//!     x.lazy_init(String::from("hello"));
//! }
//! if let Some(val) = x.get() {
//!     println!("{}", val);
//! }
//! ```
//!
//! # Examples
//!
//! This example creates a shared data structure, then initializes a member
//! variable later.
//!
//! ```
//! use freezebox::FreezeBox;
//! use std::sync::Arc;
//!
//! /// A data structure that we will initialize late.
//! #[derive(Default)]
//! struct Resources {
//!     name: FreezeBox<String>
//! }
//!
//! // Create an instance of the `Resources` struct, which contains an
//! // uninitialized `name` field.
//! let resources = Arc::new(Resources::default());
//!
//! // Clone the Arc to emulate sharing with other threads, contexts,
//! // or data structures.
//! let res2 = resources.clone();
//!
//! // Here we emulate another thread accessing the shared data structure.
//! // NOTE: it's still our responsibility to ensure that the FreezeBox
//! // is initialized before anyone dereferences it.
//! //
//! let func = move || {
//!     // explicit deref
//!     assert_eq!(*res2.name, "Hello!");
//!     // implicit deref allows transparent access to inner methods
//!     assert_eq!(res2.name.len(), 6);
//! };
//!
//! resources.name.lazy_init("Hello!".to_string());
//! func();
//! ```
//!
//! ## Comparison to other approaches
//!
//! 1. `Option<T>`
//!
//! Late initialization requires mutable access to the `Option`. This is fine
//! unless the parent struct is already shared.
//!
//! 2. `Mutex<Option<T>>`
//!
//! This solves the problem of late-initialization, but requires every caller
//! to lock the `Mutex` and unwrap the `Option`. The added code and runtime
//! overhead might not be desirable, paticularly if all we need is a shared
//! reference to the inner `T`.
//!
//! 3. [`lazy_static`]
//!
//! `lazy_static!` declares a hidden `static` variable, so it's not suitable
//! for lazy-initialized struct members. It also requires the initialization
//! code to be placed at the point of declaration, and uses a spinlock
//! internally.
//!
//! 4. [`once_cell`]
//!
//! `once_cell` is generally preferable to `lazy_static` in new Rust code, and
//! would be a good choice in the case where multiple threads are racing to
//! initialize the inner value.
//!
//! `OnceCell` doesn't implement `Deref`, and requires explicit calls to
//! `get()` or `get_or_init()`. This is similar to `MaybeBox`, but is more
//! verbose in the use case `FreezeBox` was designed for, where readers expect
//! the value to be already initialized.
//!
//! `OnceCell` does not `Box` the internal value, but this makes the atomic
//! initialization more complicated, so `once_cell::sync::OnceCell` is not
//! available in `no_std` contexts.
//!
//! [`lazy_static`]: https://docs.rs/lazy_static
//! [`once_cell`]: https://docs.rs/once_cell

#![no_std]

extern crate alloc;

mod freezebox;
mod maybebox;

pub use self::freezebox::FreezeBox;
pub use self::maybebox::MaybeBox;

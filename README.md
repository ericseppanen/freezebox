<!-- cargo-sync-readme start -->

# Rust FreezeBox: a deref'able lazy-initialized container.

`FreezeBox<T>` is a container that can have two possible states:
* uninitialized: deref is not allowed.
* initialized: deref to a `&T` is possible.

To upgrade a `FreezeBox` to the initialized state, call `lazy_init`.
`lazy_init` does not require a mutable reference, making `FreezeBox`
suitable for sharing objects first and initializing them later.

Attempting to `lazy_init` more than once, or deref while uninitialized
will cause a panic.

# Examples

This example creates a shared data structure, then circles back to
initialize one member.

```rust
use freezebox::FreezeBox;
use std::sync::Arc;

/// A data structure that we will initialize lazily.
#[derive(Default)]
struct Resources {
    name: FreezeBox<String>
}

// Create an instance of the `Resources` struct, which contains an
// uninitialized `name` field.
let resources = Arc::new(Resources::default());

// Clone the Arc to emulate sharing with other threads, contexts,
// or data structures.
let res2 = resources.clone();

// Here we emulate another thread accessing the shared data structure.
// NOTE: it's still our responsibility to ensure that the FreezeBox
// is initialized before anyone dereferences it.
//
let func = move || {
    // explicit deref
    assert_eq!(*res2.name, "Hello!");
    // implicit deref allows transparent access to inner methods
    assert_eq!(res2.name.len(), 6);
};

resources.name.lazy_init("Hello!".to_string());
func();
```
## Not quite what you were looking for?

There are many similar crates out there:
- [lazy_static](https://docs.rs/lazy_static)
- [once_cell](https://docs.rs/once_cell)
- [double-checked-cell](https://docs.rs/double-checked-cell)

<!-- cargo-sync-readme end -->

# Safety and Compatibility

FreezeBox is compatible with `no_std` projects (no feature flags needed).
It may be used in any environment with a memory allocator.

FreezeBox uses unsafe code internally. To ensure soundness, the unit
tests pass under Miri, and the unsafe code is simple and easy to
understand.

The minimum supported Rust version is 1.48.

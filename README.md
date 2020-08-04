## Rust FreezeBox: a deref'able lazy-initialized container.

`FreezeBox<T>` is a container that can have two possible states:
* uninitialized: deref is not allowed.
* initialized: deref to a `&T` is possible.

To upgrade a `FreezeBox` to the initialized state, call `lazy_init`.
`lazy_init` does not require a mutable reference, making `FreezeBox`
suitable for sharing objects first and initializing them later.

Attempting to `lazy_init` more than once, or deref while uninitialized
will cause a panic.

FreezeBox is compatible with `no_std` projects (no feature flags needed).
It may be used in any environment with a memory allocator.

### Example:
This example creates a shared data structure, then circles back to
initialize one member.

```rust
use freezebox::FreezeBox;
use std::sync::Arc;

#[derive(Default)]
struct Resources {
    name: FreezeBox<String>
}

let resources = Arc::new(Resources::default());
let res2 = resources.clone();

let func = move || {
    assert_eq!(*res2.name, "Hello!");
};

resources.name.lazy_init("Hello!".to_string());
func();
```

### Not quite what you were looking for?

There are many similar crates out there:
- [lazy_static](https://docs.rs/lazy_static/latest)
- [once_cell](https://docs.rs/once_cell/latest)
- [double-checked-cell](https://docs.rs/double-checked-cell/latest)
- [mitochondria](https://docs.rs/mitochondria/latest)
